//! Native, read-only Discord connection. Credentials never cross into the UI.
mod auth;
mod rest;
use discord_core::{Channel, ChannelId, Message, ServerId, Snapshot};
use std::{sync::Arc, thread};
use tokio::sync::{mpsc, oneshot};

pub enum Event {
    Qr(String),
    Scanned,
    Connected(Snapshot),
    Channels(ServerId, Vec<Channel>),
    History(ChannelId, Vec<Message>),
    Error(&'static str),
}
#[derive(Clone, Copy)]
pub enum Command {
    Channels(ServerId),
    History(ChannelId),
}
/// One handle per login attempt. Drop cancels even while I/O or queues are blocked.
pub struct Connection {
    events: mpsc::Receiver<Event>,
    commands: mpsc::Sender<Command>,
    cancel: Option<oneshot::Sender<()>>,
}
impl Connection {
    pub fn start(wake: impl Fn() + Send + Sync + 'static) -> std::io::Result<Self> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        let (event_tx, events) = mpsc::channel(8);
        let (commands, command_rx) = mpsc::channel(8);
        let (cancel, stopped) = oneshot::channel();
        let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(wake);
        thread::Builder::new()
            .name("discord-session".into())
            .spawn(move || {
                runtime.block_on(async move {
                    until_cancelled(stopped, async {
                        if let Err(error) = session(&event_tx, command_rx, &wake).await {
                            let _ = publish(&event_tx, &wake, Event::Error(error)).await;
                        }
                    })
                    .await;
                });
            })?;
        Ok(Self {
            events,
            commands,
            cancel: Some(cancel),
        })
    }
    pub fn event(&mut self) -> Result<Option<Event>, &'static str> {
        match self.events.try_recv() {
            Ok(event) => Ok(Some(event)),
            Err(mpsc::error::TryRecvError::Empty) => Ok(None),
            Err(mpsc::error::TryRecvError::Disconnected) => {
                Err("Conexão encerrada. Entre novamente.")
            }
        }
    }
    pub fn command(&self, command: Command) -> Result<(), &'static str> {
        self.commands
            .try_send(command)
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => "Aguarde a leitura em andamento.",
                mpsc::error::TrySendError::Closed(_) => "Conexão encerrada. Entre novamente.",
            })
    }
}
async fn until_cancelled(
    stopped: oneshot::Receiver<()>,
    work: impl std::future::Future<Output = ()>,
) {
    tokio::select! { biased; _ = stopped => {}, _ = work => {} }
}
impl Drop for Connection {
    fn drop(&mut self) {
        if let Some(cancel) = self.cancel.take() {
            let _ = cancel.send(());
        }
    }
}
async fn publish(
    tx: &mpsc::Sender<Event>,
    wake: &Arc<dyn Fn() + Send + Sync>,
    event: Event,
) -> Result<(), &'static str> {
    tx.send(event).await.map_err(|_| "Conexão cancelada.")?;
    wake();
    Ok(())
}
async fn session(
    tx: &mpsc::Sender<Event>,
    mut commands: mpsc::Receiver<Command>,
    wake: &Arc<dyn Fn() + Send + Sync>,
) -> Result<(), &'static str> {
    let http = reqwest::Client::builder()
        .user_agent("Rustcord/0.1 (native open-source client)")
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|_| "Não foi possível iniciar HTTPS.")?;
    let token = auth::login(&http, tx, wake).await?;
    let mut api = rest::Api::new(http, token)?;
    let snapshot = api.snapshot().await?;
    publish(tx, wake, Event::Connected(snapshot)).await?;
    while let Some(command) = commands.recv().await {
        let result = match command {
            Command::Channels(id) => api
                .channels(id)
                .await
                .map(|channels| Event::Channels(id, channels)),
            Command::History(id) => api
                .history(id)
                .await
                .map(|messages| Event::History(id, messages)),
        };
        match result {
            Ok(event) => publish(tx, wake, event).await?,
            Err(error) => {
                publish(tx, wake, Event::Error(error)).await?;
                if error == rest::UNAUTHORIZED {
                    return Ok(());
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn cancellation_interrupts_full_event_queue_and_idle_io() {
        let (tx, _rx) = mpsc::channel(1);
        tx.send(Event::Error("occupied")).await.unwrap();
        let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(|| {});
        let (cancel, stopped) = oneshot::channel();
        let waiting = tokio::spawn(async move {
            until_cancelled(stopped, async {
                publish(&tx, &wake, Event::Scanned).await.unwrap();
                panic!("full queue unexpectedly completed");
            })
            .await;
        });
        tokio::task::yield_now().await;
        cancel.send(()).unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(1), waiting)
            .await
            .unwrap()
            .unwrap();
        let (cancel, stopped) = oneshot::channel();
        let waiting = tokio::spawn(until_cancelled(stopped, std::future::pending()));
        drop(cancel);
        tokio::time::timeout(std::time::Duration::from_secs(1), waiting)
            .await
            .unwrap()
            .unwrap();
    }
}
