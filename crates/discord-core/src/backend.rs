//! Nonblocking UI boundary; network workers may wait on the command receiver.
use crate::{Capabilities, Snapshot};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::{Receiver, SyncSender, TryRecvError, TrySendError, sync_channel},
};

pub const BACKEND_QUEUE_CAPACITY: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SessionGeneration(u64);
impl SessionGeneration {
    pub const INITIAL: Self = Self(1);
    pub fn next(self) -> Option<Self> {
        self.0.checked_add(1).map(Self)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackendCommand {
    RequestSnapshot,
}
pub enum BackendEvent {
    Snapshot(Snapshot),
    Unavailable(&'static str),
}
pub struct SessionEvent {
    pub generation: SessionGeneration,
    pub event: BackendEvent,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueueError {
    Full,
    Closed,
    Cancelled,
}

/// GUI-independent adapter. Poll must never wait for I/O.
pub trait Backend {
    fn capabilities(&self) -> Capabilities;
    fn poll(&mut self);
}

pub struct BackendConnection {
    command_tx: Option<SyncSender<BackendCommand>>,
    event_rx: Option<Receiver<SessionEvent>>,
    cancelled: Arc<AtomicBool>,
}
pub struct BackendWorker {
    generation: SessionGeneration,
    command_rx: Receiver<BackendCommand>,
    event_tx: SyncSender<SessionEvent>,
    cancelled: Arc<AtomicBool>,
    wake: Arc<dyn Fn() + Send + Sync>,
}

/// Bounded slot counts; adapters must also bound decoded payload bytes.
pub fn backend_channel(
    generation: SessionGeneration,
    wake: impl Fn() + Send + Sync + 'static,
) -> (BackendConnection, BackendWorker) {
    let (command_tx, command_rx) = sync_channel(BACKEND_QUEUE_CAPACITY);
    let (event_tx, event_rx) = sync_channel(BACKEND_QUEUE_CAPACITY);
    let cancelled = Arc::new(AtomicBool::new(false));
    (
        BackendConnection {
            command_tx: Some(command_tx),
            event_rx: Some(event_rx),
            cancelled: cancelled.clone(),
        },
        BackendWorker {
            generation,
            command_rx,
            event_tx,
            cancelled,
            wake: Arc::new(wake),
        },
    )
}
impl BackendConnection {
    pub fn try_command(&self, command: BackendCommand) -> Result<(), QueueError> {
        let Some(tx) = &self.command_tx else {
            return Err(QueueError::Cancelled);
        };
        tx.try_send(command).map_err(|error| match error {
            TrySendError::Full(_) => QueueError::Full,
            TrySendError::Disconnected(_) => QueueError::Closed,
        })
    }
    pub fn try_event(&self) -> Result<Option<SessionEvent>, QueueError> {
        let Some(rx) = &self.event_rx else {
            return Err(QueueError::Cancelled);
        };
        match rx.try_recv() {
            Ok(event) => Ok(Some(event)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => Err(QueueError::Closed),
        }
    }
    /// Cancellation is independent of queue capacity and wakes a waiting worker
    /// by closing the only command sender. Dropping the receiver releases backlog.
    pub fn cancel(&mut self) {
        self.cancelled.store(true, Ordering::Release);
        self.command_tx.take();
        self.event_rx.take();
    }
}
impl Drop for BackendConnection {
    fn drop(&mut self) {
        self.cancel();
    }
}
impl BackendWorker {
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
    pub fn try_command(&self) -> Result<Option<BackendCommand>, QueueError> {
        if self.is_cancelled() {
            return Err(QueueError::Cancelled);
        }
        match self.command_rx.try_recv() {
            Ok(command) => Ok(Some(command)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => Err(QueueError::Closed),
        }
    }
    /// Only for a dedicated background worker, never for the GUI or Tokio tasks.
    pub fn wait_command(&self) -> Result<BackendCommand, QueueError> {
        if self.is_cancelled() {
            return Err(QueueError::Cancelled);
        }
        let command = self.command_rx.recv().map_err(|_| QueueError::Closed)?;
        if self.is_cancelled() {
            return Err(QueueError::Cancelled);
        }
        Ok(command)
    }
    /// Full queues return the event to the producer. Never silently drop state.
    pub fn try_publish(&self, event: BackendEvent) -> Result<(), TrySendError<SessionEvent>> {
        let event = SessionEvent {
            generation: self.generation,
            event,
        };
        if self.is_cancelled() {
            return Err(TrySendError::Disconnected(event));
        }
        self.event_tx.try_send(event)?;
        (self.wake)();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{sync::atomic::AtomicUsize, time::Duration};

    #[test]
    fn queues_report_pressure_and_preserve_event_generation() {
        let wakes = Arc::new(AtomicUsize::new(0));
        let callback = wakes.clone();
        let (connection, worker) = backend_channel(SessionGeneration::INITIAL, move || {
            callback.fetch_add(1, Ordering::Relaxed);
        });
        for _ in 0..BACKEND_QUEUE_CAPACITY {
            connection
                .try_command(BackendCommand::RequestSnapshot)
                .unwrap();
            assert!(
                worker
                    .try_publish(BackendEvent::Unavailable("synthetic event"))
                    .is_ok()
            );
        }
        assert_eq!(
            connection.try_command(BackendCommand::RequestSnapshot),
            Err(QueueError::Full)
        );
        assert!(matches!(
            worker.try_publish(BackendEvent::Unavailable("retained")),
            Err(TrySendError::Full(SessionEvent {
                event: BackendEvent::Unavailable("retained"),
                ..
            }))
        ));
        assert_eq!(wakes.load(Ordering::Relaxed), BACKEND_QUEUE_CAPACITY);
        let event = connection.try_event().unwrap().unwrap();
        assert_eq!(event.generation, SessionGeneration::INITIAL);
        assert!(
            worker
                .try_publish(BackendEvent::Unavailable("retained"))
                .is_ok()
        );
    }
    #[test]
    fn cancellation_ignores_full_queues_and_releases_pending_events() {
        let (mut connection, worker) = backend_channel(SessionGeneration::INITIAL, || {});
        for _ in 0..BACKEND_QUEUE_CAPACITY {
            connection
                .try_command(BackendCommand::RequestSnapshot)
                .unwrap();
            assert!(
                worker
                    .try_publish(BackendEvent::Unavailable("queued"))
                    .is_ok()
            );
        }
        connection.cancel();
        assert!(worker.is_cancelled());
        assert_eq!(worker.try_command(), Err(QueueError::Cancelled));
        assert!(matches!(connection.try_event(), Err(QueueError::Cancelled)));
        assert!(matches!(
            worker.try_publish(BackendEvent::Unavailable("late")),
            Err(TrySendError::Disconnected(_))
        ));
    }
    #[test]
    fn dropping_connection_wakes_a_waiting_background_worker() {
        let (connection, worker) = backend_channel(SessionGeneration::INITIAL, || {});
        let (tx, rx) = std::sync::mpsc::channel();
        let thread = std::thread::spawn(move || {
            tx.send(worker.wait_command()).unwrap();
        });
        drop(connection);
        assert!(matches!(
            rx.recv_timeout(Duration::from_secs(2)).unwrap(),
            Err(QueueError::Closed | QueueError::Cancelled)
        ));
        thread.join().unwrap();
    }
}
