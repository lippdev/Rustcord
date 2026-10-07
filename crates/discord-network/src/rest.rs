use discord_core::{AccountId, Channel, ChannelId, Message, MessageId, Server, ServerId, Snapshot};
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
        // Text and announcement channels only; forums/threads/voice need separate UX.
        if !matches!(value["type"].as_u64(), Some(0 | 5)) {
            continue;
        }
        let channel = Channel {
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
    Ok(channels)
}
fn history(data: &Value) -> Result<Vec<Message>, &'static str> {
    let values = data.as_array().filter(|a| a.len() <= 50).ok_or(INVALID)?;
    let mut messages = Vec::new();
    for value in values {
        let author = value["author"]["global_name"]
            .as_str()
            .filter(|s| s.len() <= 128)
            .map(str::to_owned)
            .unwrap_or(text(&value["author"], "username", 128)?);
        let message = Message {
            id: MessageId::new(id(&value["id"])?),
            author,
            text: text(value, "content", 32 * 1024)?,
            time: text(value, "timestamp", 64)?,
            reactions: 0,
            reply_to: value["message_reference"]["message_id"]
                .as_str()
                .map(|_| id(&value["message_reference"]["message_id"]).map(MessageId::new))
                .transpose()?,
        };
        if messages.iter().any(|m: &Message| m.id == message.id) {
            return Err(INVALID);
        }
        messages.push(message);
    }
    messages.sort_by_key(|m| m.id);
    Ok(messages)
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
    fn unsupported_channels_are_filtered_and_duplicates_rejected() {
        assert_eq!(
            channels(
                &json!([{"id":"1","name":"voice","type":2},{"id":"2","name":"chat","type":0}])
            )
            .unwrap()
            .len(),
            1
        );
        assert!(
            channels(&json!([{"id":"1","name":"a","type":0},{"id":"1","name":"b","type":0}]))
                .is_err()
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
