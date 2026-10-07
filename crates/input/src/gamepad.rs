use crate::{AppAction, Direction};
use gilrs::{Axis, Button, EventType, Gilrs};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    thread,
    time::{Duration, Instant},
};
pub fn button(button: Button) -> Option<AppAction> {
    Some(match button {
        Button::South => AppAction::Confirm,
        Button::East => AppAction::Back,
        Button::West => AppAction::Context,
        Button::North => AppAction::Reply,
        Button::Start => AppAction::Menu,
        Button::LeftTrigger => AppAction::SwitchServer(-1),
        Button::RightTrigger => AppAction::SwitchServer(1),
        Button::LeftTrigger2 => AppAction::SwitchChannel(-1),
        Button::RightTrigger2 => AppAction::SwitchChannel(1),
        Button::DPadUp => AppAction::Navigate(Direction::Up),
        Button::DPadDown => AppAction::Navigate(Direction::Down),
        Button::DPadLeft => AppAction::Navigate(Direction::Left),
        Button::DPadRight => AppAction::Navigate(Direction::Right),
        _ => return None,
    })
}
/// Hysteresis avoids noise around the deadzone; timers provide deterministic repeat.
#[derive(Default)]
pub struct Stick {
    x: f32,
    y: f32,
    held: Option<Direction>,
    next: Option<Instant>,
}
impl Stick {
    pub fn axis(&mut self, axis: Axis, value: f32, now: Instant) -> Option<AppAction> {
        match axis {
            Axis::LeftStickX => self.x = value,
            Axis::LeftStickY => self.y = value,
            _ => return None,
        }
        let magnitude = self.x.abs().max(self.y.abs());
        let threshold = if self.held.is_some() { 0.35 } else { 0.55 };
        let direction = if magnitude < threshold {
            None
        } else if self.x.abs() > self.y.abs() {
            Some(if self.x > 0.0 {
                Direction::Right
            } else {
                Direction::Left
            })
        } else {
            Some(if self.y > 0.0 {
                Direction::Up
            } else {
                Direction::Down
            })
        };
        if direction == self.held {
            return None;
        }
        self.held = direction;
        self.next = direction.map(|_| now + Duration::from_millis(320));
        direction.map(AppAction::Navigate)
    }
    pub fn tick(&mut self, now: Instant) -> Option<AppAction> {
        if self.next.is_some_and(|next| now >= next) {
            self.next = Some(now + Duration::from_millis(110));
            return self.held.map(AppAction::Navigate);
        }
        None
    }
}
pub enum DeviceEvent {
    Action(AppAction),
    Status(String),
}
pub struct Gamepad {
    pub events: Receiver<DeviceEvent>,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Gamepad {
    pub fn start(wake: impl Fn() + Send + 'static) -> Self {
        let (tx, events) = mpsc::sync_channel(128);
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        let worker = thread::spawn(move || {
            let send = |event| {
                if tx.try_send(event).is_ok() {
                    wake();
                }
            };
            let mut gilrs = match Gilrs::new() {
                Ok(g) => g,
                Err(e) => {
                    send(DeviceEvent::Status(format!("Gamepad unavailable: {e}")));
                    return;
                }
            };
            let status = |g: &Gilrs| {
                let names: Vec<_> = g
                    .gamepads()
                    .filter(|(_, p)| p.is_connected())
                    .map(|(_, p)| p.name().to_owned())
                    .collect();
                if names.is_empty() {
                    "No controller connected".into()
                } else {
                    names.join(" · ")
                }
            };
            send(DeviceEvent::Status(status(&gilrs)));
            let mut stick = Stick::default();
            let mut owner = None;
            while !stopped.load(Ordering::Relaxed) {
                if let Some(event) = gilrs.next_event_blocking(Some(if stick.held.is_some() {
                    Duration::from_millis(16)
                } else {
                    Duration::from_millis(250)
                })) {
                    match event.event {
                        EventType::Connected | EventType::Disconnected => {
                            stick = Stick::default();
                            owner = None;
                            send(DeviceEvent::Status(status(&gilrs)));
                        }
                        EventType::ButtonPressed(b, _) => {
                            if let Some(action) = button(b) {
                                send(DeviceEvent::Action(action));
                            }
                        }
                        EventType::AxisChanged(axis, value, _) => {
                            if owner != Some(event.id) {
                                stick = Stick::default();
                                owner = Some(event.id);
                            }
                            if let Some(action) = stick.axis(axis, value, Instant::now()) {
                                send(DeviceEvent::Action(action));
                            }
                        }
                        _ => {}
                    }
                }
                if let Some(action) = stick.tick(Instant::now()) {
                    send(DeviceEvent::Action(action));
                }
            }
        });
        Self {
            events,
            stop,
            worker: Some(worker),
        }
    }
}
impl Drop for Gamepad {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn devices_share_confirm() {
        assert_eq!(
            crate::keyboard(crate::Key::Enter, false),
            button(Button::South).unwrap()
        );
    }
    #[test]
    fn stick_deadzone_repeat_and_release() {
        let now = Instant::now();
        let mut stick = Stick::default();
        assert_eq!(stick.axis(Axis::LeftStickX, 0.4, now), None);
        assert_eq!(
            stick.axis(Axis::LeftStickX, 0.8, now),
            Some(AppAction::Navigate(Direction::Right))
        );
        assert_eq!(stick.tick(now + Duration::from_millis(319)), None);
        assert!(stick.tick(now + Duration::from_millis(320)).is_some());
        assert_eq!(stick.axis(Axis::LeftStickX, 0.4, now), None);
        assert_eq!(stick.axis(Axis::LeftStickX, 0.2, now), None);
        assert_eq!(stick.tick(now + Duration::from_secs(2)), None);
    }
    #[test]
    fn shoulders_and_triggers_have_distinct_actions() {
        assert_eq!(
            button(Button::LeftTrigger),
            Some(AppAction::SwitchServer(-1))
        );
        assert_eq!(
            button(Button::RightTrigger2),
            Some(AppAction::SwitchChannel(1))
        );
    }
}
