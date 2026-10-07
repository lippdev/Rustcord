use discord_core::{
    AccountId, Attachment, Channel, ChannelDetails, ChannelId, ChannelKind, Embed, Message,
    MessageDetails, MessageId, Server, ServerId, Snapshot,
};
use reqwest::{
    Client, Response, StatusCode,
    header::{AUTHORIZATION, HeaderValue},
};
use serde_json::Value;
use std::time::{Duration, Instant};
use zeroize::Zeroizing;
pub const UNAUTHORIZED: &str = "Sessão expirada. Entre novamente.";
const INVALID: &str = "O Discord retornou dados incompatíveis.";
const LIMIT: usize = 4 * 1024 * 1024;
/// Stream and bound the decoded body before constructing a JSON tree.
pub(crate) async fn bounded_json(
    mut response: Response,
    limit: usize,
) -> Result<Value, &'static str> {
    if response.content_length().is_some_and(|n| n > limit as u64) {
        return Err("Resposta excedeu o limite de memória.");
    }
    let mut bytes = Zeroizing::new(Vec::new());
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "Leitura interrompida. Tente atualizar.")?
    {
        if bytes.len().saturating_add(chunk.len()) > limit {
            return Err("Resposta excedeu o limite de memória.");
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|_| INVALID)
}
fn id(value: &Value) -> Result<u64, &'static str> {
    value
        .as_str()
        .and_then(|s| s.parse().ok())
        .filter(|n| *n > 0)
        .ok_or(INVALID)
}
fn text(value: &Value, field: &str, max: usize) -> Result<String, &'static str> {
    let value = value[field].as_str().ok_or(INVALID)?;
    if value.len() > max {
        return Err(INVALID);
    }
    Ok(value.to_owned())
}
pub struct Api {
    http: Client,
    authorization: HeaderValue,
    cooldown: Option<Instant>,
    #[cfg(test)]
    origin: String,
}
impl Api {
    pub fn new(http: Client, token: Zeroizing<String>) -> Result<Self, &'static str> {
        let mut authorization = HeaderValue::from_str(&token).map_err(|_| UNAUTHORIZED)?;
        authorization.set_sensitive(true);
        Ok(Self {
            http,
            authorization,
            cooldown: None,
            #[cfg(test)]
            origin: String::new(),
        })
    }
    async fn get(&mut self, path: &str) -> Result<Value, &'static str> {
        if self
            .cooldown
            .is_some_and(|deadline| Instant::now() < deadline)
        {
            return Err("Limite de requisições. Aguarde antes de atualizar.");
        }
        #[cfg(not(test))]
        let origin = "https://discord.com/api/v9";
        #[cfg(test)]
        let origin = self.origin.as_str();
        let response = self
            .http
            .get(format!("{origin}{path}"))
            .header(AUTHORIZATION, self.authorization.clone())
            .send()
            .await
            .map_err(|_| "Falha de rede. Tente atualizar.")?;
        match response.status() {
            StatusCode::UNAUTHORIZED => return Err(UNAUTHORIZED),
            StatusCode::FORBIDDEN => return Err("Você não tem acesso a este canal."),
            StatusCode::TOO_MANY_REQUESTS => {
                let data = bounded_json(response, 64 * 1024).await?;
                let seconds = data["retry_after"]
                    .as_f64()
                    .filter(|s| s.is_finite() && *s >= 0.0)
                    .unwrap_or(60.0)
                    .clamp(1.0, 3600.0);
                self.cooldown = Some(Instant::now() + Duration::from_secs_f64(seconds));
                return Err("Limite de requisições. Aguarde antes de atualizar.");
            }
            status if !status.is_success() => {
                return Err("O Discord não conseguiu atender à leitura. Tente atualizar.");
            }
            _ => {}
        }
        bounded_json(response, LIMIT).await
    }
    pub async fn snapshot(&mut self) -> Result<Snapshot, &'static str> {
        let user = self.get("/users/@me").await?;
        let account = AccountId::new(id(&user["id"])?);
        let profile = user["global_name"]
            .as_str()
            .filter(|s| s.len() <= 128)
            .map(str::to_owned)
            .unwrap_or(text(&user, "username", 128)?);
        let mut servers = Vec::new();
        let mut after = 0;
        loop {
            let data = self
                .get(&format!("/users/@me/guilds?limit=200&after={after}"))
                .await?;
            let guilds = data.as_array().filter(|a| a.len() <= 200).ok_or(INVALID)?;
            for guild in guilds {
                let server_id = id(&guild["id"])?;
                if server_id <= after || servers.iter().any(|s: &Server| s.id.get() == server_id) {
                    return Err(INVALID);
                }
                let name = text(guild, "name", 512)?;
                let initials = name
                    .split_whitespace()
                    .filter_map(|part| part.chars().next())
                    .take(2)
                    .collect();
                servers.push(Server {
                    icon: cdn_hash(guild, "icon").map(|hash| {
                        format!("https://cdn.discordapp.com/icons/{server_id}/{hash}.png?size=64")
                    }),
                    id: ServerId::new(server_id),
                    name,
                    initials,
                    channels: Vec::new(),
                });
            }
            if guilds.len() < 200 {
                break;
            }
            if servers.len() >= 1000 {
                return Err("Lista de servidores excedeu o limite deste protótipo.");
            }
            let next = guilds
                .iter()
                .map(|g| id(&g["id"]))
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .max()
                .ok_or(INVALID)?;
            if next <= after {
                return Err(INVALID);
            }
            after = next;
        }
        let snapshot = Snapshot {
            avatar: user_avatar(&user),
            account,
            servers,
            profile,
        };
        snapshot.validate().map_err(|_| INVALID)?;
        Ok(snapshot)
    }
    pub async fn channels(&mut self, server: ServerId) -> Result<Vec<Channel>, &'static str> {
        let data = self
            .get(&format!("/guilds/{}/channels", server.get()))
            .await?;
        channels(&data)
    }
    pub async fn history(&mut self, channel: ChannelId) -> Result<Vec<Message>, &'static str> {
        let data = self
            .get(&format!("/channels/{}/messages?limit=50", channel.get()))
            .await?;
        history(&data)
    }
}
fn channels(data: &Value) -> Result<Vec<Channel>, &'static str> {
    let values = data.as_array().filter(|a| a.len() <= 1000).ok_or(INVALID)?;
    let mut channels = Vec::new();
    for value in values {
        let kind = match value["type"].as_u64() {
            Some(0 | 5 | 10 | 11 | 12) => ChannelKind::Text,
            Some(2) => ChannelKind::Voice,
            Some(4) => ChannelKind::Category,
            Some(13) => ChannelKind::Stage,
            Some(15 | 16) => ChannelKind::Forum,
            _ => continue,
        };
        let channel = Channel {
            details: ChannelDetails {
                kind,
                parent: value["parent_id"]
                    .as_str()
                    .map(|_| id(&value["parent_id"]).map(ChannelId::new))
                    .transpose()?,
                position: value["position"].as_i64().unwrap_or(0),
            },
            id: ChannelId::new(id(&value["id"])?),
            name: text(value, "name", 512)?,
            topic: value["topic"]
                .as_str()
                .filter(|s| s.len() <= 4096)
                .unwrap_or("")
                .to_owned(),
            messages: Vec::new(),
        };
        if channels.iter().any(|c: &Channel| c.id == channel.id) {
            return Err(INVALID);
        }
        channels.push(channel);
    }
    channels.sort_by_key(|c| (c.details.position, c.id));
    Ok(channels)
}
pub(crate) fn history(data: &Value) -> Result<Vec<Message>, &'static str> {
    let values = data.as_array().filter(|a| a.len() <= 50).ok_or(INVALID)?;
    let mut messages = Vec::new();
    for value in values {
        let message = parse_message(value)?;
        if messages.iter().any(|m: &Message| m.id == message.id) {
            return Err(INVALID);
        }
        messages.push(message);
    }
    messages.sort_by_key(|m| m.id);
    Ok(messages)
}
fn optional_text(value: &Value, key: &str, max: usize) -> String {
    value[key]
        .as_str()
        .filter(|s| s.len() <= max)
        .unwrap_or("")
        .to_owned()
}
fn cdn_hash<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value[key].as_str().filter(|s| {
        !s.is_empty() && s.len() <= 128 && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
    })
}
pub(crate) fn user_avatar(user: &Value) -> Option<String> {
    let user_id = id(&user["id"]).ok()?;
    Some(match cdn_hash(user, "avatar") {
        Some(hash) => format!("https://cdn.discordapp.com/avatars/{user_id}/{hash}.png?size=64"),
        None => {
            let index = user["discriminator"]
                .as_str()
                .and_then(|s| s.parse::<u64>().ok())
                .filter(|n| *n > 0)
                .map_or((user_id >> 22) % 6, |n| n % 5);
            format!("https://cdn.discordapp.com/embed/avatars/{index}.png")
        }
    })
}
pub(crate) fn safe_media(value: &Value) -> Option<String> {
    let raw = value.as_str().filter(|s| s.len() <= 4096)?;
    let url = reqwest::Url::parse(raw).ok()?;
    (url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.port().is_none()
        && matches!(
            url.host_str(),
            Some("cdn.discordapp.com" | "media.discordapp.net")
        ))
    .then(|| raw.to_owned())
}
pub(crate) fn parse_message(value: &Value) -> Result<Message, &'static str> {
    let user = &value["author"];
    let author = value["member"]["nick"]
        .as_str()
        .or(user["global_name"].as_str())
        .or(user["username"].as_str())
        .filter(|s| s.len() <= 128)
        .ok_or(INVALID)?
        .to_owned();
    let mut attachments = Vec::new();
    if let Some(items) = value["attachments"].as_array() {
        for item in items.iter().take(10) {
            if let Some(url) = safe_media(&item["url"]) {
                let image = item["content_type"].as_str().is_some_and(|s| {
                    matches!(s, "image/png" | "image/jpeg" | "image/webp" | "image/gif")
                });
                attachments.push(Attachment {
                    name: optional_text(item, "filename", 512),
                    url,
                    image,
                });
            }
        }
    }
    let embeds = value["embeds"]
        .as_array()
        .into_iter()
        .flatten()
        .take(10)
        .map(|item| Embed {
            title: optional_text(item, "title", 512),
            description: optional_text(item, "description", 4096),
            url: item["url"]
                .as_str()
                .filter(|s| s.len() <= 4096)
                .and_then(|s| reqwest::Url::parse(s).ok())
                .filter(|u| u.scheme() == "https")
                .map(|u| u.to_string()),
            image: safe_media(&item["image"]["proxy_url"])
                .or_else(|| safe_media(&item["image"]["url"]))
                .or_else(|| safe_media(&item["thumbnail"]["proxy_url"])),
        })
        .collect();
    Ok(Message {
        id: MessageId::new(id(&value["id"])?),
        author,
        text: text(value, "content", 32 * 1024)?,
        time: text(value, "timestamp", 64)?,
        reactions: 0,
        reply_to: value["message_reference"]["message_id"]
            .as_str()
            .map(|_| id(&value["message_reference"]["message_id"]).map(MessageId::new))
            .transpose()?,
        details: MessageDetails {
            author_id: id(&user["id"]).unwrap_or(0),
            avatar: user_avatar(user),
            attachments,
            embeds,
            edited: value["edited_timestamp"].as_str().is_some(),
        },
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::{Read, Write};
    #[test]
    fn history_preserves_snowflakes_sorts_and_rejects_duplicates() {
        let message = |id: &str| json!({"id":id,"author":{"username":"Test"},"content":"Olá","timestamp":"2026-10-07T10:00:00Z"});
        let data = json!([message("9007199254740994"), message("9007199254740993")]);
        let parsed = history(&data).unwrap();
        assert_eq!(parsed[0].id.get(), 9007199254740993);
        assert_eq!(parsed[1].id.get(), 9007199254740994);
        assert!(history(&json!([message("1"), message("1")])).is_err());
        assert!(history(&json!([message("0")])).is_err());
    }
    #[test]
    fn voice_channels_are_retained_and_duplicates_rejected() {
        assert_eq!(
            channels(
                &json!([{"id":"1","name":"voice","type":2},{"id":"2","name":"chat","type":0}])
            )
            .unwrap()
            .len(),
            2
        );
        assert!(
            channels(&json!([{"id":"1","name":"a","type":0},{"id":"1","name":"b","type":0}]))
                .is_err()
        );
    }
    #[test]
    fn message_media_and_author_metadata_survive_parsing() {
        let parsed = parse_message(&json!({"id":"20","author":{"id":"10","username":"Test","avatar":"abc123"},"member":{"nick":"Nickname"},"content":"","timestamp":"2026-10-07T10:00:00Z","attachments":[{"filename":"photo.png","url":"https://cdn.discordapp.com/attachments/1/2/photo.png?ex=123","content_type":"image/png"}],"embeds":[{"title":"Preview","description":"Description","image":{"proxy_url":"https://media.discordapp.net/test.png"}}]})).unwrap();
        assert_eq!(parsed.author, "Nickname");
        assert_eq!(parsed.details.author_id, 10);
        assert!(parsed.details.avatar.unwrap().contains("/10/abc123.png"));
        assert_eq!(parsed.details.attachments.len(), 1);
        assert!(parsed.details.attachments[0].image);
        assert!(parsed.details.embeds[0].image.is_some());
    }
    #[test]
    fn media_only_accepts_discord_https_hosts_and_preserves_signed_queries() {
        for url in [
            "http://cdn.discordapp.com/a.png",
            "https://cdn.discordapp.com.evil.test/a",
            "https://evil.test/a",
            "https://user@cdn.discordapp.com/a",
            "https://cdn.discordapp.com:444/a",
        ] {
            assert!(safe_media(&json!(url)).is_none());
        }
        assert_eq!(
            safe_media(&json!("https://cdn.discordapp.com/a?ex=1&hm=2")).unwrap(),
            "https://cdn.discordapp.com/a?ex=1&hm=2"
        );
        assert!(
            user_avatar(&json!({"id":"10","avatar":"../../evil"}))
                .unwrap()
                .contains("/embed/avatars/")
        );
    }
    async fn mock_request(
        status: u16,
        body: &'static str,
    ) -> (Api, std::thread::JoinHandle<String>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let worker = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut request = [0; 4096];
            let read = stream.read(&mut request).unwrap();
            write!(
                stream,
                "HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
            String::from_utf8_lossy(&request[..read]).into_owned()
        });
        let client = Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap();
        let mut api = Api::new(client, Zeroizing::new("synthetic-session".into())).unwrap();
        api.origin = format!("http://{address}");
        (api, worker)
    }
    #[tokio::test]
    async fn authorization_and_access_errors_never_echo_response_body() {
        for (status, expected) in [
            (401, UNAUTHORIZED),
            (403, "Você não tem acesso a este canal."),
        ] {
            let (mut api, worker) = mock_request(status, "secret-service-detail").await;
            assert_eq!(api.get("/test").await.unwrap_err(), expected);
            assert!(
                worker
                    .join()
                    .unwrap()
                    .to_lowercase()
                    .contains("authorization: synthetic-session")
            );
        }
    }
    #[tokio::test]
    async fn rate_limit_prevents_immediate_followup_without_retrying() {
        let (mut api, worker) = mock_request(429, r#"{"retry_after":30}"#).await;
        assert!(api.get("/test").await.unwrap_err().starts_with("Limite"));
        worker.join().unwrap();
        // Listener is gone: this succeeds only if cooldown stops the second request locally.
        assert!(api.get("/test").await.unwrap_err().starts_with("Limite"));
    }
    #[tokio::test]
    async fn oversized_body_is_rejected_before_json_decode() {
        let (mut api, worker) = mock_request(200, "not-json").await;
        let response = api
            .http
            .get(format!("{}/test", api.origin))
            .send()
            .await
            .unwrap();
        assert_eq!(
            bounded_json(response, 4).await.unwrap_err(),
            "Resposta excedeu o limite de memória."
        );
        worker.join().unwrap();
        api.cooldown = None;
    }
}
