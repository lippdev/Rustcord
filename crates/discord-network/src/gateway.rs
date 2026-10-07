//! Bounded selected-channel subscriptions; never requests an entire guild member list.
use crate::{Event, publish, rest};
use discord_core::{ChannelId, Member, MessageId, ServerId};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::{collections::BTreeMap, sync::Arc, time::Duration};
use tokio::sync::{mpsc, watch};
use tokio_tungstenite::{
    connect_async_with_config,
    tungstenite::{Message, protocol::WebSocketConfig},
};
use zeroize::Zeroizing;

pub(crate) type Selection = Option<(ServerId, ChannelId)>;
const FAILED: &str = "Atualização ao vivo desconectada. Use Atualizar ou reconecte pelo QR.";
const LIMIT: usize = 200;
#[derive(Default)]
struct MemberList {
    slots: Vec<Option<Member>>,
}
impl MemberList {
    fn apply(&mut self, data: &Value) {
        for op in data["ops"].as_array().into_iter().flatten().take(200) {
            let index = op["index"].as_u64().unwrap_or(u64::MAX) as usize;
            match op["op"].as_str() {
                Some("SYNC" | "INVALIDATE") => {
                    let start = op["range"][0].as_u64().unwrap_or(u64::MAX) as usize;
                    let end = op["range"][1].as_u64().unwrap_or(u64::MAX) as usize;
                    if start > end || start >= LIMIT {
                        continue;
                    }
                    let end = end.min(LIMIT - 1);
                    self.slots.resize(self.slots.len().max(end + 1), None);
                    self.slots[start..=end].fill(None);
                    if op["op"] == "SYNC" {
                        for (offset, item) in op["items"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .take(end - start + 1)
                            .enumerate()
                        {
                            self.slots[start + offset] = member(&item["member"]);
                        }
                    }
                }
                Some("INSERT") if index < LIMIT => {
                    self.slots.resize(self.slots.len().max(index), None);
                    self.slots.insert(index, member(&op["item"]["member"]));
                    self.slots.truncate(LIMIT);
                }
                Some("UPDATE") if index < LIMIT => {
                    self.slots.resize(self.slots.len().max(index + 1), None);
                    self.slots[index] = member(&op["item"]["member"]);
                }
                Some("DELETE") if index < self.slots.len() => {
                    self.slots.remove(index);
                }
                _ => {}
            }
        }
    }
    fn members(&self) -> Vec<Member> {
        let mut seen = std::collections::HashSet::new();
        self.slots
            .iter()
            .flatten()
            .filter(|m| seen.insert(m.id))
            .cloned()
            .collect()
    }
}
fn snowflake(value: &Value) -> Option<u64> {
    value.as_str()?.parse().ok().filter(|id| *id > 0)
}
fn member(value: &Value) -> Option<Member> {
    let user = &value["user"];
    let id = snowflake(&user["id"])?;
    let name = value["nick"]
        .as_str()
        .or(user["global_name"].as_str())
        .or(user["username"].as_str())
        .filter(|s| s.len() <= 128)?
        .to_owned();
    let status = value["presence"]["status"]
        .as_str()
        .filter(|s| matches!(*s, "online" | "idle" | "dnd" | "offline"))
        .unwrap_or("unknown")
        .to_owned();
    Some(Member {
        id,
        name,
        avatar: rest::user_avatar(user),
        status,
    })
}
fn subscription(selection: Selection, previous: Selection) -> Value {
    let mut subscriptions = serde_json::Map::new();
    if let Some((guild, _)) = previous {
        subscriptions.insert(
            guild.get().to_string(),
            json!({"channels":{},"typing":false,"activities":false,"threads":false}),
        );
    }
    if let Some((guild, channel)) = selection {
        subscriptions.insert(guild.get().to_string(), json!({"channels":{channel.get().to_string():[[0,99],[100,199]]},"typing":false,"activities":false,"threads":false,"member_updates":true}));
    }
    json!({"op":37,"d":{"subscriptions":subscriptions}})
}
#[derive(Default)]
struct Resume {
    session: Option<Zeroizing<String>>,
    sequence: Option<u64>,
    url: Option<String>,
}
impl Resume {
    fn reset(&mut self) {
        self.session = None;
        self.sequence = None;
        self.url = None;
    }
    fn identify(&self, token: &str) -> Zeroizing<String> {
        Zeroizing::new(if let Some(session)=&self.session {
            json!({"op":6,"d":{"token":token,"session_id":session.as_str(),"seq":self.sequence}})
        } else {
            json!({"op":2,"d":{"token":token,"properties":{"os":std::env::consts::OS,"browser":"Rustcord","device":"Rustcord"},"compress":false,"large_threshold":50}})
        }.to_string())
    }
}
pub(crate) async fn run(
    token: Zeroizing<String>,
    mut selected: watch::Receiver<Selection>,
    tx: mpsc::Sender<Event>,
    wake: Arc<dyn Fn() + Send + Sync>,
) {
    let mut resume = Resume::default();
    for attempt in 0..6 {
        let url = resume
            .url
            .clone()
            .unwrap_or_else(|| "wss://gateway.discord.gg/?v=9&encoding=json".into());
        let result = connection(&token, &mut selected, &tx, &wake, &url, &mut resume).await;
        let _ = publish(&tx, &wake, Event::Realtime(false)).await;
        if result == Err(rest::UNAUTHORIZED) {
            let _ = publish(&tx, &wake, Event::Error(rest::UNAUTHORIZED)).await;
            return;
        }
        if attempt < 5 {
            tokio::time::sleep(Duration::from_secs(1 << attempt)).await;
        }
    }
}
async fn connection(
    token: &str,
    selected: &mut watch::Receiver<Selection>,
    tx: &mpsc::Sender<Event>,
    wake: &Arc<dyn Fn() + Send + Sync>,
    url: &str,
    resume: &mut Resume,
) -> Result<(), &'static str> {
    let config = WebSocketConfig::default()
        .max_message_size(Some(8 * 1024 * 1024))
        .max_frame_size(Some(8 * 1024 * 1024));
    let (mut socket, _) = tokio::time::timeout(
        Duration::from_secs(20),
        connect_async_with_config(url, Some(config), false),
    )
    .await
    .map_err(|_| FAILED)?
    .map_err(|_| FAILED)?;
    let hello = tokio::time::timeout(Duration::from_secs(20), socket.next())
        .await
        .map_err(|_| FAILED)?
        .ok_or(FAILED)?
        .map_err(|_| FAILED)?;
    let hello: Value =
        serde_json::from_str(hello.to_text().map_err(|_| FAILED)?).map_err(|_| FAILED)?;
    if hello["op"] != 10 {
        return Err(FAILED);
    }
    let milliseconds = hello["d"]["heartbeat_interval"]
        .as_u64()
        .filter(|n| (1000..=120000).contains(n))
        .ok_or(FAILED)?;
    let identify = resume.identify(token);
    socket
        .send(Message::Text(identify.as_str().into()))
        .await
        .map_err(|_| FAILED)?;
    let mut heartbeat = tokio::time::interval(Duration::from_millis(milliseconds));
    heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut ack = true;
    let mut sequence: Option<u64> = resume.sequence;
    let mut ready = false;
    let mut active = None;
    let mut members = MemberList::default();
    let mut voice: BTreeMap<u64, ChannelId> = BTreeMap::new();
    let readiness = tokio::time::sleep(Duration::from_secs(30));
    tokio::pin!(readiness);
    loop {
        tokio::select! {
            _ = &mut readiness, if !ready => return Err(FAILED),
            _ = heartbeat.tick() => {
                if !ack { return Err(FAILED); }
                ack = false;
                socket.send(Message::Text(json!({"op":1,"d":sequence}).to_string().into())).await.map_err(|_|FAILED)?;
            }
            changed = selected.changed(), if ready => {
                changed.map_err(|_|FAILED)?;
                let next = *selected.borrow_and_update();
                socket.send(Message::Text(subscription(next, active).to_string().into())).await.map_err(|_|FAILED)?;
                active = next; members = MemberList::default(); voice.clear();
                if let Some((guild,channel))=active { publish(tx,wake,Event::Members(guild,channel,Vec::new())).await?; }
            }
            frame = socket.next() => {
                let frame = frame.ok_or(FAILED)?.map_err(|_|FAILED)?;
                let Message::Text(raw) = frame else {
                    if let Message::Close(close)=frame {
                        if let Some(close)=close {
                            let code=u16::from(close.code);
                            if code==4004 {return Err(rest::UNAUTHORIZED);}
                            if matches!(code,4007|4009) {resume.reset();}
                        }
                        return Err(FAILED);
                    }
                    continue;
                };
                let payload:Value = serde_json::from_str(&raw).map_err(|_|FAILED)?;
                match payload["op"].as_u64() {
                    Some(11) => ack=true,
                    Some(1) => { socket.send(Message::Text(json!({"op":1,"d":sequence}).to_string().into())).await.map_err(|_|FAILED)?; }
                    Some(7) => return Err(FAILED),
                    Some(9) => { if payload["d"]!=true {resume.reset();} return Err(FAILED); },
                    Some(0) => {
                        sequence=payload["s"].as_u64().or(sequence); resume.sequence=sequence;
                        let data=&payload["d"];
                        if payload["t"]=="READY" || payload["t"]=="RESUMED" {
                            if payload["t"]=="READY" {
                                resume.session=data["session_id"].as_str().filter(|s|s.len()<=128).map(|s|Zeroizing::new(s.to_owned()));
                                resume.url=data["resume_gateway_url"].as_str().and_then(|s|reqwest::Url::parse(s).ok()).filter(|url|url.scheme()=="wss" && url.username().is_empty() && url.password().is_none() && url.host_str().is_some_and(|host|host=="gateway.discord.gg" || (host.starts_with("gateway-") && host.ends_with(".discord.gg")))).map(|mut url|{url.set_query(Some("v=9&encoding=json"));url.to_string()});
                            }
                            ready=true; publish(tx,wake,Event::Realtime(true)).await?;
                            let next = *selected.borrow_and_update();
                            socket.send(Message::Text(subscription(next,None).to_string().into())).await.map_err(|_|FAILED)?;
                            active=next;
                        }
                        if let Some((guild,channel))=active {
                            match payload["t"].as_str() {
                                Some("MESSAGE_CREATE") if snowflake(&data["channel_id"])==Some(channel.get()) => {
                                    if let Ok(message)=rest::parse_message(data) { publish(tx,wake,Event::Message(channel,message)).await?; }
                                }
                                Some("MESSAGE_UPDATE") if snowflake(&data["channel_id"])==Some(channel.get()) => { publish(tx,wake,Event::Refresh(channel)).await?; }
                                Some("MESSAGE_DELETE") if snowflake(&data["channel_id"])==Some(channel.get()) => {
                                    if let Some(id)=snowflake(&data["id"]) { publish(tx,wake,Event::Deleted(channel,vec![MessageId::new(id)])).await?; }
                                }
                                Some("MESSAGE_DELETE_BULK") if snowflake(&data["channel_id"])==Some(channel.get()) => {
                                    let ids=data["ids"].as_array().into_iter().flatten().take(100).filter_map(snowflake).map(MessageId::new).collect();
                                    publish(tx,wake,Event::Deleted(channel,ids)).await?;
                                }
                                Some("GUILD_MEMBER_LIST_UPDATE") if snowflake(&data["guild_id"])==Some(guild.get()) => {
                                    members.apply(data); publish(tx,wake,Event::Members(guild,channel,members.members())).await?;
                                }
                                Some("VOICE_STATE_UPDATE") if snowflake(&data["guild_id"])==Some(guild.get()) => {
                                    if let Some(id)=snowflake(&data["user_id"]) {
                                        voice.remove(&id);
                                        if let Some(channel)=snowflake(&data["channel_id"]) && voice.len()<200 { voice.insert(id,ChannelId::new(channel)); }
                                        publish(tx,wake,Event::Voice(guild,voice.iter().map(|(user,channel)|(*user,*channel)).collect())).await?;
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    _=>{}
                }
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn websocket_identifies_heartbeats_and_delivers_selected_channel_events() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
            ws.send(Message::Text(
                json!({"op":10,"d":{"heartbeat_interval":1000}})
                    .to_string()
                    .into(),
            ))
            .await
            .unwrap();
            let identify: Value =
                serde_json::from_str(ws.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
            assert_eq!(identify["d"]["token"], "synthetic-token");
            assert_eq!(identify["d"]["properties"]["browser"], "Rustcord");
            ws.send(Message::Text(
                json!({"op":0,"t":"READY","s":1,"d":{}}).to_string().into(),
            ))
            .await
            .unwrap();
            loop {
                let payload: Value =
                    serde_json::from_str(ws.next().await.unwrap().unwrap().to_text().unwrap())
                        .unwrap();
                if payload["op"] == 1 {
                    ws.send(Message::Text(json!({"op":11,"d":null}).to_string().into()))
                        .await
                        .unwrap();
                }
                if payload["op"] == 37 {
                    break;
                }
            }
            for event in [
                json!({"op":0,"s":2,"t":"MESSAGE_CREATE","d":{"channel_id":"20","id":"30","author":{"id":"40","username":"Test"},"content":"Live","timestamp":"2026-10-07T10:00:00Z"}}),
                json!({"op":0,"s":3,"t":"GUILD_MEMBER_LIST_UPDATE","d":{"guild_id":"10","ops":[{"op":"SYNC","range":[0,0],"items":[{"member":{"user":{"id":"40","username":"Test"}}}]}]}}),
                json!({"op":0,"s":4,"t":"MESSAGE_DELETE","d":{"channel_id":"20","id":"30"}}),
            ] {
                ws.send(Message::Text(event.to_string().into()))
                    .await
                    .unwrap();
            }
            ws.close(None).await.unwrap();
        });
        let (_selection_tx, mut selection) =
            watch::channel(Some((ServerId::new(10), ChannelId::new(20))));
        let (tx, mut rx) = mpsc::channel(8);
        let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(|| {});
        assert!(
            tokio::time::timeout(
                Duration::from_secs(3),
                connection(
                    "synthetic-token",
                    &mut selection,
                    &tx,
                    &wake,
                    &format!("ws://{address}"),
                    &mut Resume::default(),
                )
            )
            .await
            .unwrap()
            .is_err()
        );
        assert!(matches!(rx.try_recv().unwrap(), Event::Realtime(true)));
        assert!(matches!(rx.try_recv().unwrap(),Event::Message(_,message) if message.text=="Live"));
        assert!(matches!(rx.try_recv().unwrap(),Event::Members(_,_,members) if members[0].id==40));
        assert!(matches!(rx.try_recv().unwrap(),Event::Deleted(_,ids) if ids[0].get()==30));
        server.await.unwrap();
    }
    #[tokio::test]
    async fn resume_reuses_sequence_and_restores_subscription_without_new_identify() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
            ws.send(Message::Text(
                json!({"op":10,"d":{"heartbeat_interval":1000}})
                    .to_string()
                    .into(),
            ))
            .await
            .unwrap();
            let resume: Value =
                serde_json::from_str(ws.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
            assert_eq!(resume["op"], 6);
            assert_eq!(resume["d"]["seq"], 42);
            assert_eq!(resume["d"]["session_id"], "synthetic-session");
            ws.send(Message::Text(
                json!({"op":0,"s":43,"t":"RESUMED","d":{}})
                    .to_string()
                    .into(),
            ))
            .await
            .unwrap();
            loop {
                let payload: Value =
                    serde_json::from_str(ws.next().await.unwrap().unwrap().to_text().unwrap())
                        .unwrap();
                if payload["op"] == 1 {
                    ws.send(Message::Text(json!({"op":11,"d":null}).to_string().into()))
                        .await
                        .unwrap();
                }
                if payload["op"] == 37 {
                    assert_eq!(
                        payload["d"]["subscriptions"]["10"]["channels"]["20"],
                        json!([[0, 99], [100, 199]])
                    );
                    break;
                }
            }
            ws.close(None).await.unwrap();
        });
        let (_selection_tx, mut selection) =
            watch::channel(Some((ServerId::new(10), ChannelId::new(20))));
        let (tx, mut rx) = mpsc::channel(8);
        let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(|| {});
        let mut resume = Resume {
            session: Some(Zeroizing::new("synthetic-session".into())),
            sequence: Some(42),
            url: None,
        };
        assert!(
            tokio::time::timeout(
                Duration::from_secs(3),
                connection(
                    "synthetic-token",
                    &mut selection,
                    &tx,
                    &wake,
                    &format!("ws://{address}"),
                    &mut resume
                )
            )
            .await
            .unwrap()
            .is_err()
        );
        assert_eq!(resume.sequence, Some(43));
        assert!(matches!(rx.try_recv().unwrap(), Event::Realtime(true)));
        server.await.unwrap();
        resume.reset();
        assert_eq!(
            serde_json::from_str::<Value>(&resume.identify("synthetic-token")).unwrap()["op"],
            2
        );
    }
    #[test]
    fn member_list_reduces_sync_insert_update_delete_invalidate_in_order() {
        let item = |id: &str| json!({"member":{"user":{"id":id,"username":id},"presence":{"status":"online"}}});
        let mut list = MemberList::default();
        list.apply(&json!({"ops":[{"op":"SYNC","range":[0,2],"items":[{"group":{"id":"online"}},item("1"),item("2")]},{"op":"INSERT","index":2,"item":item("3")},{"op":"UPDATE","index":1,"item":item("4")},{"op":"DELETE","index":3}]}));
        assert_eq!(
            list.members().iter().map(|m| m.id).collect::<Vec<_>>(),
            vec![4, 3]
        );
        list.apply(&json!({"ops":[{"op":"INVALIDATE","range":[0,199]},{"op":"INSERT","index":1000000,"item":item("5")},{"op":"SYNC","range":[1000000,1000001],"items":[]}]}));
        assert!(list.members().is_empty());
        assert!(list.slots.len() <= 200);
    }
    #[test]
    fn changing_guild_unsubscribes_previous_and_bounds_visible_range() {
        let data = subscription(
            Some((ServerId::new(2), ChannelId::new(20))),
            Some((ServerId::new(1), ChannelId::new(10))),
        );
        assert_eq!(data["d"]["subscriptions"]["1"]["channels"], json!({}));
        assert_eq!(
            data["d"]["subscriptions"]["2"]["channels"]["20"],
            json!([[0, 99], [100, 199]])
        );
    }
}
