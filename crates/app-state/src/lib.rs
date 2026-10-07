//! Pure application reducer, shared by any future frontend.
use discord_core::{Backend, BackendEvent, Message, MockBackend, Snapshot};
use input::{AppAction, Direction, Region};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Main,
    Settings,
    Context,
    Reply,
}
pub struct AppState {
    pub data: Snapshot,
    pub server: usize,
    pub channel: usize,
    pub message: usize,
    pub region: Region,
    pub screen: Screen,
    pub menu_item: usize,
    pub metrics: bool,
    pub notice: String,
    next_id: u64,
}
impl Default for AppState {
    fn default() -> Self {
        let BackendEvent::Snapshot(data) = MockBackend.initial_event() else {
            unreachable!()
        };
        Self::new(data)
    }
}
impl AppState {
    pub fn new(data: Snapshot) -> Self {
        let next_id = data
            .servers
            .iter()
            .flat_map(|s| &s.channels)
            .flat_map(|c| &c.messages)
            .map(|m| m.id)
            .max()
            .unwrap_or(0)
            + 1;
        Self {
            data,
            server: 0,
            channel: 0,
            message: 0,
            region: Region::Servers,
            screen: Screen::Main,
            menu_item: 0,
            metrics: cfg!(debug_assertions),
            notice: "Local demo · no Discord connection".into(),
            next_id,
        }
    }
    pub fn channels(&self) -> &[discord_core::Channel] {
        self.data
            .servers
            .get(self.server)
            .map_or(&[], |s| s.channels.as_slice())
    }
    pub fn messages(&self) -> &[Message] {
        self.channels()
            .get(self.channel)
            .map_or(&[], |c| c.messages.as_slice())
    }
    pub fn menu_len(&self) -> usize {
        match self.screen {
            Screen::Reply => 3,
            Screen::Settings => 2,
            Screen::Context => 2,
            Screen::Main => 0,
        }
    }
    fn region_step(&mut self, delta: i32) {
        self.region = match (self.region, delta > 0) {
            (Region::Servers, true) | (Region::Conversation, false) => Region::Channels,
            (Region::Channels, true) => Region::Conversation,
            (Region::Channels, false) => Region::Servers,
            (r, _) => r,
        };
    }
    fn open(&mut self, screen: Screen) {
        self.screen = screen;
        self.menu_item = 0;
    }
    fn send(&mut self, text: String) {
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        if let Some(c) = self
            .data
            .servers
            .get_mut(self.server)
            .and_then(|s| s.channels.get_mut(self.channel))
        {
            c.messages.push(Message {
                id: self.next_id,
                author: "You".into(),
                text: text.chars().take(2000).collect(),
                time: "now".into(),
                reactions: 0,
            });
            self.next_id += 1;
            if c.messages.len() > 200 {
                c.messages.remove(0);
            }
            self.message = c.messages.len() - 1;
            self.region = Region::Conversation;
        }
        self.screen = Screen::Main;
        self.notice = "Message added locally".into();
    }
    fn react(&mut self) {
        if let Some(m) = self
            .data
            .servers
            .get_mut(self.server)
            .and_then(|s| s.channels.get_mut(self.channel))
            .and_then(|c| c.messages.get_mut(self.message))
        {
            m.reactions = u32::from(m.reactions == 0);
        }
        self.screen = Screen::Main;
        self.notice = "Local reaction toggled".into();
    }
    pub fn dispatch(&mut self, action: AppAction) {
        // Settings changes and text submission are explicit commands from any adapter.
        match action {
            AppAction::ToggleMetrics => {
                self.metrics = !self.metrics;
                return;
            }
            AppAction::Submit(text) => {
                self.send(text);
                return;
            }
            AppAction::React => {
                self.react();
                return;
            }
            AppAction::Menu => {
                if self.screen == Screen::Settings {
                    self.screen = Screen::Main;
                } else {
                    self.open(Screen::Settings);
                }
                return;
            }
            _ => {}
        }
        if self.screen != Screen::Main {
            match action {
                AppAction::SelectMenu(item) => {
                    self.menu_item = item.min(self.menu_len().saturating_sub(1))
                }
                AppAction::Back => self.screen = Screen::Main,
                AppAction::Navigate(Direction::Up) | AppAction::PreviousRegion => {
                    self.menu_item = self.menu_item.saturating_sub(1)
                }
                AppAction::Navigate(Direction::Down) | AppAction::NextRegion => {
                    self.menu_item = (self.menu_item + 1).min(self.menu_len().saturating_sub(1))
                }
                AppAction::Confirm => match (self.screen, self.menu_item) {
                    (Screen::Settings, 0) => self.metrics = !self.metrics,
                    (Screen::Reply, i @ 0..=1) => {
                        self.send(["Count me in!", "Let's play tonight."][i].into())
                    }
                    (Screen::Context, 0) => self.react(),
                    _ => self.screen = Screen::Main,
                },
                _ => {}
            }
            return;
        }
        match action {
            AppAction::Navigate(Direction::Left) | AppAction::PreviousRegion | AppAction::Back => {
                self.region_step(-1)
            }
            AppAction::Navigate(Direction::Right) | AppAction::NextRegion | AppAction::Confirm => {
                self.region_step(1)
            }
            AppAction::Navigate(direction) => {
                let delta = if direction == Direction::Up { -1 } else { 1 };
                match self.region {
                    Region::Servers => {
                        self.server = bounded(self.server, delta, self.data.servers.len());
                        self.channel = 0;
                        self.message = 0;
                    }
                    Region::Channels => {
                        self.channel = bounded(self.channel, delta, self.channels().len());
                        self.message = 0;
                    }
                    Region::Conversation => {
                        self.message = bounded(self.message, delta, self.messages().len())
                    }
                }
            }
            AppAction::SwitchServer(delta) => {
                self.server = wrapped(self.server, delta, self.data.servers.len());
                self.channel = 0;
                self.message = 0;
            }
            AppAction::SwitchChannel(delta) => {
                self.channel = wrapped(self.channel, delta, self.channels().len());
                self.message = 0;
            }
            AppAction::Select(region, item) => {
                self.region = region;
                match region {
                    Region::Servers => {
                        self.server = item.min(self.data.servers.len().saturating_sub(1));
                        self.channel = 0;
                        self.message = 0;
                    }
                    Region::Channels => {
                        self.channel = item.min(self.channels().len().saturating_sub(1));
                        self.message = 0;
                    }
                    Region::Conversation => {
                        self.message = item.min(self.messages().len().saturating_sub(1))
                    }
                }
            }
            AppAction::Context => self.open(Screen::Context),
            AppAction::Reply => self.open(Screen::Reply),
            _ => {}
        }
    }
}
fn bounded(index: usize, delta: i32, len: usize) -> usize {
    (index as i64 + i64::from(delta)).clamp(0, len.saturating_sub(1) as i64) as usize
}
fn wrapped(index: usize, delta: i32, len: usize) -> usize {
    if len == 0 {
        0
    } else {
        (index as i64 + i64::from(delta)).rem_euclid(len as i64) as usize
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn actions_navigate_reply_react_and_restore_selection() {
        let mut s = AppState::default();
        s.dispatch(AppAction::Confirm);
        assert_eq!(s.region, Region::Channels);
        s.dispatch(AppAction::Navigate(Direction::Down));
        assert_eq!(s.channel, 1);
        s.dispatch(AppAction::Confirm);
        assert_eq!(s.region, Region::Conversation);
        s.dispatch(AppAction::Reply);
        s.dispatch(AppAction::Confirm);
        assert_eq!(s.messages().last().unwrap().text, "Count me in!");
        s.dispatch(AppAction::Context);
        s.dispatch(AppAction::Confirm);
        assert_eq!(s.messages().last().unwrap().reactions, 1);
        s.dispatch(AppAction::Menu);
        s.dispatch(AppAction::Confirm);
        assert!(!s.metrics);
        s.dispatch(AppAction::Back);
        assert_eq!(s.region, Region::Conversation);
        s.dispatch(AppAction::Back);
        assert_eq!(s.region, Region::Channels);
    }
    #[test]
    fn modal_captures_navigation_and_switching() {
        let mut s = AppState::default();
        s.dispatch(AppAction::Menu);
        s.dispatch(AppAction::SwitchServer(1));
        assert_eq!(s.server, 0);
        s.dispatch(AppAction::Confirm);
        assert!(!s.metrics);
        s.dispatch(AppAction::Back);
        assert_eq!(s.screen, Screen::Main);
    }
    #[test]
    fn empty_snapshot_and_long_sequences_are_safe() {
        let mut s = AppState::new(Snapshot {
            servers: vec![],
            profile: "Test".into(),
        });
        for _ in 0..100 {
            for a in [
                AppAction::SwitchServer(-1),
                AppAction::SwitchChannel(1),
                AppAction::Select(Region::Conversation, usize::MAX),
                AppAction::Navigate(Direction::Down),
                AppAction::Submit("hello".into()),
                AppAction::React,
            ] {
                s.dispatch(a);
            }
        }
        assert!(s.messages().is_empty());
    }
    #[test]
    fn wrap_resets_dependent_selection_and_bounds_clicks() {
        let mut s = AppState::default();
        s.dispatch(AppAction::SwitchServer(-1));
        assert_eq!(s.server, 3);
        s.dispatch(AppAction::SwitchChannel(-1));
        assert_eq!(s.channel, 3);
        s.dispatch(AppAction::Select(Region::Conversation, 999));
        assert_eq!(s.message, 5);
        s.dispatch(AppAction::SwitchServer(1));
        assert_eq!((s.server, s.channel, s.message), (0, 0, 0));
    }
    #[test]
    fn local_history_and_text_are_bounded() {
        let mut s = AppState::default();
        for _ in 0..210 {
            s.dispatch(AppAction::Submit("a".repeat(3000)));
        }
        assert_eq!(s.messages().len(), 200);
        assert_eq!(s.messages().last().unwrap().text.len(), 2000);
        assert!(s.messages().windows(2).all(|w| w[0].id < w[1].id));
    }
}
