//! Independent Remote Auth v2 implementation; no browser impersonation or challenge bypass.
use crate::{Event, publish};
use base64::{
    Engine,
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
};
use futures_util::{SinkExt, StreamExt};
use rsa::{Oaep, RsaPrivateKey, RsaPublicKey, pkcs8::EncodePublicKey};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{sync::Arc, time::Duration};
use tokio::{
    sync::mpsc,
    time::{Instant, interval_at},
};
use tokio_tungstenite::{
    connect_async_with_config,
    tungstenite::{Message, client::IntoClientRequest, protocol::WebSocketConfig},
};
use zeroize::Zeroizing;
const PROTOCOL: &str = "Resposta de login inválida. Tente novamente.";
const EXPIRED: &str = "QR expirado ou sem resposta. Gere outro QR.";
const NETWORK: &str = "Não foi possível conectar ao login do Discord.";

fn decrypt(key: &RsaPrivateKey, encoded: &str) -> Result<Zeroizing<Vec<u8>>, &'static str> {
    let ciphertext = STANDARD.decode(encoded).map_err(|_| PROTOCOL)?;
    if ciphertext.len() != 256 {
        return Err(PROTOCOL);
    }
    key.decrypt(Oaep::new::<Sha256>(), &ciphertext)
        .map(Zeroizing::new)
        .map_err(|_| PROTOCOL)
}
fn string<'a>(value: &'a Value, name: &str) -> Result<&'a str, &'static str> {
    value[name]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or(PROTOCOL)
}
fn qr_url(fingerprint: &str) -> Result<String, &'static str> {
    if !(16..=128).contains(&fingerprint.len())
        || !fingerprint
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
    {
        return Err(PROTOCOL);
    }
    Ok(format!("https://discord.com/ra/{fingerprint}"))
}
#[derive(Clone, Copy, PartialEq)]
enum Phase {
    Hello,
    Nonce,
    Qr,
    Scan,
    Approval,
}

pub async fn login(
    http: &reqwest::Client,
    tx: &mpsc::Sender<Event>,
    wake: &Arc<dyn Fn() + Send + Sync>,
) -> Result<Zeroizing<String>, &'static str> {
    tokio::time::timeout(
        Duration::from_secs(600),
        login_at(
            http,
            tx,
            wake,
            "wss://remote-auth-gateway.discord.gg/?v=2",
            "https://discord.com/api/v9/users/@me/remote-auth/login",
        ),
    )
    .await
    .map_err(|_| EXPIRED)?
}

async fn login_at(
    http: &reqwest::Client,
    tx: &mpsc::Sender<Event>,
    wake: &Arc<dyn Fn() + Send + Sync>,
    websocket_url: &str,
    ticket_url: &str,
) -> Result<Zeroizing<String>, &'static str> {
    // The dedicated session thread generates keys; never block the GUI.
    let key = RsaPrivateKey::new(&mut rand::thread_rng(), 2048)
        .map_err(|_| "Falha ao gerar chave de login.")?;
    let public = RsaPublicKey::from(&key)
        .to_public_key_der()
        .map_err(|_| PROTOCOL)?;
    let mut request = websocket_url.into_client_request().map_err(|_| PROTOCOL)?;
    request.headers_mut().insert(
        "Origin",
        "https://discord.com".parse().map_err(|_| PROTOCOL)?,
    );
    request
        .headers_mut()
        .insert("User-Agent", "Rustcord/0.1".parse().map_err(|_| PROTOCOL)?);
    let config = WebSocketConfig::default()
        .max_message_size(Some(64 * 1024))
        .max_frame_size(Some(64 * 1024));
    let (mut ws, _) = tokio::time::timeout(
        Duration::from_secs(15),
        connect_async_with_config(request, Some(config), false),
    )
    .await
    .map_err(|_| NETWORK)?
    .map_err(|_| NETWORK)?;
    let mut phase = Phase::Hello;
    let mut deadline = Instant::now() + Duration::from_secs(15);
    let mut heartbeat = interval_at(
        Instant::now() + Duration::from_secs(3600),
        Duration::from_secs(3600),
    );
    heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut awaiting_ack = false;
    loop {
        tokio::select! {
            biased;
            _ = tokio::time::sleep_until(deadline) => return Err(EXPIRED),
            _ = heartbeat.tick(), if phase != Phase::Hello => {
                if awaiting_ack { return Err(EXPIRED); }
                ws.send(Message::Text(json!({"op":"heartbeat"}).to_string().into())).await.map_err(|_| NETWORK)?;
                awaiting_ack = true;
            },
            frame = ws.next() => {
                let frame = frame.ok_or(EXPIRED)?.map_err(|_| NETWORK)?;
                let text = match frame {
                    Message::Text(text) => text,
                    Message::Close(_) => return Err(EXPIRED),
                    Message::Ping(data) => { ws.send(Message::Pong(data)).await.map_err(|_| NETWORK)?; continue; },
                    _ => continue,
                };
                let payload: Value = serde_json::from_str(&text).map_err(|_| PROTOCOL)?;
                match string(&payload, "op")? {
                    "hello" if phase == Phase::Hello => {
                        let period = payload["heartbeat_interval"].as_u64().filter(|n| (1000..=120_000).contains(n)).ok_or(PROTOCOL)?;
                        let timeout = payload["timeout_ms"].as_u64().filter(|n| (1000..=600_000).contains(n)).ok_or(PROTOCOL)?;
                        deadline = Instant::now() + Duration::from_millis(timeout);
                        heartbeat = interval_at(Instant::now() + Duration::from_millis(period), Duration::from_millis(period));
                        heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
                        ws.send(Message::Text(json!({"op":"init", "encoded_public_key":STANDARD.encode(public.as_bytes())}).to_string().into())).await.map_err(|_| NETWORK)?;
                        phase = Phase::Nonce;
                    },
                    "nonce_proof" if phase == Phase::Nonce => {
                        let nonce = decrypt(&key, string(&payload, "encrypted_nonce")?)?;
                        let proof = URL_SAFE_NO_PAD.encode(Sha256::digest(&*nonce));
                        ws.send(Message::Text(json!({"op":"nonce_proof", "proof":proof}).to_string().into())).await.map_err(|_| NETWORK)?;
                        phase = Phase::Qr;
                    },
                    "pending_remote_init" if phase == Phase::Qr => {
                        publish(tx, wake, Event::Qr(qr_url(string(&payload, "fingerprint")?)?)).await?;
                        phase = Phase::Scan;
                    },
                    "pending_ticket" if phase == Phase::Scan => {
                        // Validate encrypted identity, but do not publish personal payloads.
                        let identity = decrypt(&key, string(&payload, "encrypted_user_payload")?)?;
                        std::str::from_utf8(&identity).map_err(|_| PROTOCOL)?;
                        publish(tx, wake, Event::Scanned).await?;
                        phase = Phase::Approval;
                    },
                    "pending_login" if phase == Phase::Approval => {
                        let ticket = Zeroizing::new(string(&payload, "ticket")?.to_owned());
                        let response = http.post(ticket_url)
                            .header("Origin", "https://discord.com")
                            .json(&json!({"ticket":ticket.as_str()})).send().await.map_err(|_| NETWORK)?;
                        if !response.status().is_success() {
                            return Err("Login recusado ou exige um desafio. Use o cliente oficial para resolver e tente novamente.");
                        }
                        let data = crate::rest::bounded_json(response, 64 * 1024).await?;
                        let plaintext = decrypt(&key, string(&data, "encrypted_token")?)?;
                        let token = std::str::from_utf8(&plaintext).map_err(|_| PROTOCOL)?;
                        if token.len() < 16 || token.len() > 512 || !token.bytes().all(|b| b.is_ascii_graphic()) { return Err(PROTOCOL); }
                        return Ok(Zeroizing::new(token.to_owned()));
                    },
                    "heartbeat_ack" => awaiting_ack = false,
                    "cancel" => return Err("Login cancelado no celular. Gere outro QR para tentar novamente."),
                    _ => return Err(PROTOCOL),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fingerprint_cannot_change_login_origin_or_inject_query() {
        assert!(
            qr_url("abcdefghijklmnop_123-ABC")
                .unwrap()
                .starts_with("https://discord.com/ra/")
        );
        for value in [
            "https://evil.test/",
            "abcdefghijklmnop?x=1",
            "short",
            "abcdefghijklmnop/../",
            "abcdefghijklmnop\n",
        ] {
            assert!(qr_url(value).is_err());
        }
    }
    #[test]
    fn nonce_proof_roundtrip_and_corrupt_ciphertext() {
        let private = RsaPrivateKey::new(&mut rand::thread_rng(), 2048).unwrap();
        let encrypted = RsaPublicKey::from(&private)
            .encrypt(
                &mut rand::thread_rng(),
                Oaep::new::<Sha256>(),
                b"synthetic-nonce",
            )
            .unwrap();
        let clear = decrypt(&private, &STANDARD.encode(&encrypted)).unwrap();
        assert_eq!(&*clear, b"synthetic-nonce");
        assert_eq!(
            URL_SAFE_NO_PAD.encode(Sha256::digest(&*clear)),
            URL_SAFE_NO_PAD.encode(Sha256::digest(b"synthetic-nonce"))
        );
        assert!(decrypt(&private, "invalid!").is_err());
        assert!(decrypt(&private, &STANDARD.encode([0; 256])).is_err());
    }
    async fn receive(ws: &mut tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>) -> Value {
        let message = ws.next().await.unwrap().unwrap();
        serde_json::from_str(message.to_text().unwrap()).unwrap()
    }
    async fn send(
        ws: &mut tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
        payload: Value,
    ) {
        ws.send(Message::Text(payload.to_string().into()))
            .await
            .unwrap();
    }
    #[tokio::test]
    async fn native_login_completes_nonce_qr_approval_and_ticket_exchange() {
        use rsa::pkcs8::DecodePublicKey;
        use std::io::{Read, Write};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let ws_url = format!("ws://{}", listener.local_addr().unwrap());
        let http_listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let ticket_url = format!("http://{}/ticket", http_listener.local_addr().unwrap());
        let (key_tx, key_rx) = std::sync::mpsc::channel::<RsaPublicKey>();
        let http_worker = std::thread::spawn(move || {
            let public = key_rx.recv_timeout(Duration::from_secs(15)).unwrap();
            let encrypted = public
                .encrypt(
                    &mut rand::thread_rng(),
                    Oaep::new::<Sha256>(),
                    b"synthetic-session-secret",
                )
                .unwrap();
            let body = json!({"encrypted_token":STANDARD.encode(encrypted)}).to_string();
            let (mut stream, _) = http_listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut bytes = Vec::new();
            loop {
                let mut buffer = [0; 2048];
                let count = stream.read(&mut buffer).unwrap();
                assert!(count > 0);
                bytes.extend_from_slice(&buffer[..count]);
                if String::from_utf8_lossy(&bytes).contains("synthetic-ticket") {
                    break;
                }
                assert!(bytes.len() < 8192);
            }
            let request = String::from_utf8_lossy(&bytes);
            assert!(request.starts_with("POST /ticket "));
            assert!(!request.to_lowercase().contains("authorization:"));
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        });
        let server = tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(tcp).await.unwrap();
            send(
                &mut ws,
                json!({"op":"hello","heartbeat_interval":1000,"timeout_ms":15000}),
            )
            .await;
            let init = receive(&mut ws).await;
            assert_eq!(init["op"], "init");
            let public = RsaPublicKey::from_public_key_der(
                &STANDARD
                    .decode(init["encoded_public_key"].as_str().unwrap())
                    .unwrap(),
            )
            .unwrap();
            key_tx.send(public.clone()).unwrap();
            let nonce = public
                .encrypt(
                    &mut rand::thread_rng(),
                    Oaep::new::<Sha256>(),
                    b"synthetic-nonce",
                )
                .unwrap();
            send(
                &mut ws,
                json!({"op":"nonce_proof","encrypted_nonce":STANDARD.encode(nonce)}),
            )
            .await;
            let proof = receive(&mut ws).await;
            assert_eq!(
                proof["proof"],
                URL_SAFE_NO_PAD.encode(Sha256::digest(b"synthetic-nonce"))
            );
            send(
                &mut ws,
                json!({"op":"pending_remote_init","fingerprint":"abcdefghijklmnop1234567890"}),
            )
            .await;
            let user = public
                .encrypt(
                    &mut rand::thread_rng(),
                    Oaep::new::<Sha256>(),
                    b"1:0:avatar:Test",
                )
                .unwrap();
            send(
                &mut ws,
                json!({"op":"pending_ticket","encrypted_user_payload":STANDARD.encode(user)}),
            )
            .await;
            send(
                &mut ws,
                json!({"op":"pending_login","ticket":"synthetic-ticket"}),
            )
            .await;
            tokio::time::sleep(Duration::from_millis(100)).await;
        });
        let (tx, mut events) = mpsc::channel(8);
        let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(|| {});
        let http = reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap();
        let token = login_at(&http, &tx, &wake, &ws_url, &ticket_url)
            .await
            .unwrap();
        assert_eq!(token.as_str(), "synthetic-session-secret");
        assert!(matches!(events.try_recv(), Ok(Event::Qr(_))));
        assert!(matches!(events.try_recv(), Ok(Event::Scanned)));
        assert!(events.try_recv().is_err());
        server.await.unwrap();
        http_worker.join().unwrap();
    }
    #[tokio::test]
    async fn cancellation_expiry_and_out_of_order_approval_stop_login() {
        for (hello, response, expected) in [
            (
                false,
                Some(json!({"op":"cancel"})),
                "Login cancelado no celular. Gere outro QR para tentar novamente.",
            ),
            (true, None, EXPIRED),
            (
                true,
                Some(json!({"op":"pending_login","ticket":"unapproved"})),
                PROTOCOL,
            ),
        ] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let url = format!("ws://{}", listener.local_addr().unwrap());
            let server = tokio::spawn(async move {
                let (tcp, _) = listener.accept().await.unwrap();
                let mut ws = tokio_tungstenite::accept_async(tcp).await.unwrap();
                if hello {
                    send(
                        &mut ws,
                        json!({"op":"hello","heartbeat_interval":1000,"timeout_ms":1000}),
                    )
                    .await;
                    receive(&mut ws).await;
                }
                if let Some(response) = response {
                    send(&mut ws, response).await;
                }
                tokio::time::sleep(Duration::from_millis(1200)).await;
            });
            let (tx, _) = mpsc::channel(8);
            let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(|| {});
            let result = login_at(
                &reqwest::Client::new(),
                &tx,
                &wake,
                &url,
                "http://127.0.0.1:1/no-exchange",
            )
            .await;
            assert_eq!(result.unwrap_err(), expected);
            server.await.unwrap();
        }
    }
}
