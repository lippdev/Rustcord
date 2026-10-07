//! Domain and backend boundary. No network or user-account authentication.
mod backend;
mod ids;
pub use backend::{
    BACKEND_QUEUE_CAPACITY, Backend, BackendCommand, BackendConnection, BackendEvent,
    BackendWorker, QueueError, SessionEvent, SessionGeneration, backend_channel,
};
pub use ids::{AccountId, ChannelId, ConversationId, MessageId, ServerId};
use std::collections::HashSet;
#[derive(Clone, Debug)]
pub struct Message {
    pub id: MessageId,
    pub author: String,
    pub text: String,
    pub time: String,
    pub reactions: u32,
    /// Local reference to an earlier message in the same channel.
    pub reply_to: Option<MessageId>,
}
#[derive(Clone, Debug)]
pub struct Channel {
    pub id: ChannelId,
    pub name: String,
    pub topic: String,
    pub messages: Vec<Message>,
}
#[derive(Clone, Debug)]
pub struct Server {
    pub id: ServerId,
    pub name: String,
    pub initials: String,
    pub channels: Vec<Channel>,
}
#[derive(Clone, Debug)]
pub struct Snapshot {
    pub account: AccountId,
    pub servers: Vec<Server>,
    pub profile: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SnapshotError {
    DuplicateServer,
    DuplicateChannel,
    DuplicateMessage,
}
impl Snapshot {
    /// Reject ambiguous identity before a backend snapshot reaches the reducer.
    pub fn validate(&self) -> Result<(), SnapshotError> {
        let mut servers = HashSet::new();
        let mut channels = HashSet::new();
        let mut messages = HashSet::new();
        for server in &self.servers {
            if !servers.insert(server.id) {
                return Err(SnapshotError::DuplicateServer);
            }
            for channel in &server.channels {
                if !channels.insert(channel.id) {
                    return Err(SnapshotError::DuplicateChannel);
                }
                for message in &channel.messages {
                    if !messages.insert(message.id) {
                        return Err(SnapshotError::DuplicateMessage);
                    }
                }
            }
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, Default)]
pub struct Capabilities {
    pub local_messages: bool,
    pub remote_messages: bool,
    pub voice: bool,
}
pub struct MockBackend {
    worker: BackendWorker,
    pending: Option<BackendEvent>,
}
impl Backend for MockBackend {
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            local_messages: true,
            ..Capabilities::default()
        }
    }
    fn poll(&mut self) {
        if self.worker.is_cancelled() {
            self.pending = None;
            return;
        }
        if self.pending.is_none()
            && matches!(
                self.worker.try_command(),
                Ok(Some(BackendCommand::RequestSnapshot))
            )
        {
            self.pending = Some(BackendEvent::Snapshot(Self::snapshot()));
        }
        if let Some(event) = self.pending.take()
            && let Err(std::sync::mpsc::TrySendError::Full(event)) = self.worker.try_publish(event)
        {
            self.pending = Some(event.event);
        }
    }
}
impl MockBackend {
    pub fn connect(
        generation: SessionGeneration,
        wake: impl Fn() + Send + Sync + 'static,
    ) -> (Self, BackendConnection) {
        let (connection, worker) = backend_channel(generation, wake);
        (
            Self {
                worker,
                pending: None,
            },
            connection,
        )
    }
    pub fn snapshot() -> Snapshot {
        let servers = ["Rust Community", "Rust Collective", "Friends", "Open Source"]
            .iter().enumerate().map(|(si, name)| Server {
                id: ServerId::new(si as u64 + 1),
                name: (*name).into(),
                initials: ["RS", "RC", "FR", "OS"][si].into(),
                channels: ["lounge", "looking-for-group", "projects", "off-topic"]
                    .iter().enumerate().map(|(ci, channel)| Channel {
                        id: ChannelId::new((si * 100 + ci + 1) as u64),
                        name: (*channel).into(),
                        topic: "A little space to connect. Pick a message and join in.".into(),
                        messages: [
                            ("Maya", "Welcome in. This is our space to hang out, build things and find the next adventure."),
                            ("Theo", "The native build is running. Has anyone tried it on Windows yet?"),
                            ("Lena", "Yes! The layout feels familiar and I can switch channels without opening a browser."),
                            ("You", "Sounds like a plan. Let's keep it simple."),
                            ("Maya", "Select a server, open a channel, and type a message below. This prototype uses local mock data."),
                            ("Theo", "Next step: validate text selection, scrolling, and large message histories."),
                        ].iter().enumerate().map(|(mi, (author, text))| Message {
                            id: MessageId::new((si * 1000 + ci * 100 + mi) as u64),
                            author: (*author).into(), text: (*text).into(),
                            time: format!("20:{:02}", 12 + mi * 3), reactions: 0, reply_to: None,
                        }).collect(),
                    }).collect(),
            }).collect();
        Snapshot {
            account: AccountId::new(1),
            servers,
            profile: "Player One".into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mock_retains_a_snapshot_when_the_event_queue_is_full() {
        let (mut backend, mut connection) = MockBackend::connect(SessionGeneration::INITIAL, || {});
        assert!(!backend.capabilities().remote_messages);
        for _ in 0..BACKEND_QUEUE_CAPACITY {
            connection
                .try_command(BackendCommand::RequestSnapshot)
                .unwrap();
            backend.poll();
        }
        connection
            .try_command(BackendCommand::RequestSnapshot)
            .unwrap();
        backend.poll();
        assert!(backend.pending.is_some());
        assert!(connection.try_event().unwrap().is_some());
        backend.poll();
        assert!(backend.pending.is_none());
        for _ in 0..BACKEND_QUEUE_CAPACITY {
            let event = connection.try_event().unwrap().unwrap();
            assert_eq!(event.generation, SessionGeneration::INITIAL);
            let BackendEvent::Snapshot(snapshot) = event.event else {
                panic!("expected snapshot");
            };
            snapshot.validate().unwrap();
        }
        assert!(connection.try_event().unwrap().is_none());
        connection.cancel();
        backend.poll();
        assert!(backend.pending.is_none());
    }
}
