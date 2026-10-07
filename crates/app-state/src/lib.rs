//! Pure application reducer, shared by any future frontend.
use discord_core::{
    BackendEvent, Message, MessageId, MockBackend, SessionEvent, Snapshot, SnapshotError,
};
pub use discord_core::{ConversationId, SessionGeneration};
use input::{AppAction, Direction, Region};
use std::collections::HashMap;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ComposerDraft {
    pub text: String,
    pub reply_to: Option<MessageId>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Main,
    Settings,
    Context,
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
    next_id: Option<u64>,
    generation: SessionGeneration,
    drafts: HashMap<ConversationId, ComposerDraft>,
}
impl Default for AppState {
    fn default() -> Self {
        Self::new(MockBackend::snapshot())
    }
}
impl AppState {
    pub fn new(data: Snapshot) -> Self {
        let next_id = data
            .servers
            .iter()
            .flat_map(|s| &s.channels)
            .flat_map(|c| &c.messages)
            .map(|m| m.id.get())
            .max()
            .unwrap_or(0)
            .checked_add(1);
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
            generation: SessionGeneration::INITIAL,
            drafts: HashMap::new(),
        }
    }
    pub fn channels(&self) -> &[discord_core::Channel] {
        self.data
            .servers
            .get(self.server)
            .map_or(&[], |s| s.channels.as_slice())
    }
    pub fn conversation(&self) -> Option<ConversationId> {
        Some(ConversationId {
            account: self.data.account,
            channel: self.channels().get(self.channel)?.id,
        })
    }
    pub fn generation(&self) -> SessionGeneration {
        self.generation
    }
    /// New account/login lifetime, not a transient reconnect of the same session.
    pub fn begin_session(&mut self) -> Option<SessionGeneration> {
        let next = self.generation.next()?;
        self.generation = next;
        self.data.servers.clear();
        self.data.profile.clear();
        self.drafts.clear();
        self.server = 0;
        self.channel = 0;
        self.message = 0;
        self.region = Region::Servers;
        self.screen = Screen::Main;
        self.menu_item = 0;
        self.next_id = Some(1);
        self.notice = "Waiting for connection".into();
        Some(next)
    }
    /// Late events must not cross logout or an account switch, even for the same ID.
    pub fn apply_backend_event(&mut self, event: SessionEvent) -> Result<bool, SnapshotError> {
        if event.generation != self.generation {
            return Ok(false);
        }
        match event.event {
            BackendEvent::Snapshot(data) => self.replace_snapshot(data)?,
            BackendEvent::Unavailable(reason) => self.notice = reason.into(),
        }
        Ok(true)
    }
    /// Preserve selection and drafts by identity when navigation/history changes.
    /// Validate first so an ambiguous snapshot cannot discard existing state.
    pub fn replace_snapshot(&mut self, data: Snapshot) -> Result<(), SnapshotError> {
        data.validate()?;
        let same_account = data.account == self.data.account;
        let server = self.data.servers.get(self.server).map(|s| s.id);
        let channel = self.conversation().map(|c| c.channel);
        let message = self.messages().get(self.message).map(|m| m.id);
        let incoming_next = data
            .servers
            .iter()
            .flat_map(|s| &s.channels)
            .flat_map(|c| &c.messages)
            .map(|m| m.id.get())
            .max()
            .unwrap_or(0)
            .checked_add(1);
        self.next_id = if same_account {
            self.next_id
                .and_then(|next| incoming_next.map(|n| n.max(next)))
        } else {
            self.drafts.clear();
            incoming_next
        };
        self.data = data;
        self.server = 0;
        self.channel = 0;
        self.message = 0;
        if same_account {
            if let Some((si, ci)) = self.data.servers.iter().enumerate().find_map(|(si, s)| {
                s.channels
                    .iter()
                    .position(|c| Some(c.id) == channel)
                    .map(|ci| (si, ci))
            }) {
                self.server = si;
                self.channel = ci;
                self.message = self
                    .messages()
                    .iter()
                    .position(|m| Some(m.id) == message)
                    .unwrap_or(0);
            } else if let Some(si) = self.data.servers.iter().position(|s| Some(s.id) == server) {
                self.server = si;
            }
        }
        let channels: std::collections::HashSet<_> = self
            .data
            .servers
            .iter()
            .flat_map(|s| &s.channels)
            .map(|c| c.id)
            .collect();
        self.drafts
            .retain(|key, _| key.account == self.data.account && channels.contains(&key.channel));
        // A menu opened on a vanished message must never act on its replacement.
        self.screen = Screen::Main;
        self.menu_item = 0;
        Ok(())
    }
    pub fn messages(&self) -> &[Message] {
        self.channels()
            .get(self.channel)
            .map_or(&[], |c| c.messages.as_slice())
    }
    pub fn menu_len(&self) -> usize {
        match self.screen {
            Screen::Settings => 2,
            Screen::Context => 4,
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
    pub fn draft(&self) -> Option<&ComposerDraft> {
        self.drafts.get(&self.conversation()?)
    }
    pub fn reply_target(&self) -> Option<&Message> {
        let id = self.draft()?.reply_to?;
        self.messages().iter().find(|m| m.id == id)
    }
    fn update_draft(&mut self, text: String) {
        let Some(key) = self.conversation() else {
            return;
        };
        self.drafts.entry(key).or_default().text = text.chars().take(2000).collect();
        self.remove_empty_draft();
    }
    fn remove_empty_draft(&mut self) {
        let Some(key) = self.conversation() else {
            return;
        };
        if self
            .drafts
            .get(&key)
            .is_some_and(|d| d.text.is_empty() && d.reply_to.is_none())
        {
            self.drafts.remove(&key);
        }
    }
    fn reply_to(&mut self, id: MessageId) {
        if self.messages().iter().any(|m| m.id == id) {
            let Some(key) = self.conversation() else {
                return;
            };
            self.drafts.entry(key).or_default().reply_to = Some(id);
            self.screen = Screen::Main;
        }
    }
    fn send(&mut self, text: String) {
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        let Some(key) = self.conversation() else {
            return;
        };
        let Some(id) = self.next_id else {
            self.notice = "Local message IDs exhausted".into();
            return;
        };
        let reply_to = self.reply_target().map(|m| m.id);
        let Some(c) = self
            .data
            .servers
            .get_mut(self.server)
            .and_then(|s| s.channels.get_mut(self.channel))
        else {
            return;
        };
        c.messages.push(Message {
            details: Default::default(),
            id: MessageId::new(id),
            author: "You".into(),
            text: text.chars().take(2000).collect(),
            time: "now".into(),
            reactions: 0,
            reply_to,
        });
        self.next_id = id.checked_add(1);
        if c.messages.len() > 200 {
            c.messages.remove(0);
        }
        self.message = c.messages.len() - 1;
        self.region = Region::Conversation;
        self.drafts.remove(&key);
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
            AppAction::UpdateDraft(text) => {
                self.update_draft(text);
                return;
            }
            AppAction::SendDraft => {
                let text = self.draft().map_or_else(String::new, |d| d.text.clone());
                self.send(text);
                return;
            }
            AppAction::ReplyTo(id) => {
                self.reply_to(MessageId::new(id));
                return;
            }
            AppAction::CancelReply => {
                if let Some(draft) = self
                    .conversation()
                    .and_then(|key| self.drafts.get_mut(&key))
                {
                    draft.reply_to = None;
                }
                self.remove_empty_draft();
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
                    (Screen::Context, 0) => {
                        if let Some(id) = self.messages().get(self.message).map(|m| m.id) {
                            self.reply_to(id);
                        } else {
                            self.screen = Screen::Main;
                        }
                    }
                    (Screen::Context, 1) => self.react(),
                    _ => self.screen = Screen::Main,
                },
                _ => {}
            }
            return;
        }
        match action {
            AppAction::Back if self.draft().is_some_and(|d| d.reply_to.is_some()) => {
                self.dispatch(AppAction::CancelReply);
            }
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
            AppAction::Reply => {
                if let Some(id) = self.messages().get(self.message).map(|m| m.id) {
                    self.reply_to(id);
                }
            }
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
    fn stale_session_events_cannot_restore_channels_drafts_or_notices() {
        let mut s = AppState::default();
        let old = s.generation();
        let data = s.data.clone();
        s.dispatch(AppAction::UpdateDraft("old session draft".into()));
        let new = s.begin_session().unwrap();
        assert_ne!(new, old);
        for event in [
            BackendEvent::Snapshot(data.clone()),
            BackendEvent::Unavailable("old failure"),
        ] {
            assert!(
                !s.apply_backend_event(SessionEvent {
                    generation: old,
                    event
                })
                .unwrap()
            );
        }
        assert!(s.data.servers.is_empty());
        assert!(s.drafts.is_empty());
        assert_eq!(s.notice, "Waiting for connection");
        assert!(
            s.apply_backend_event(SessionEvent {
                generation: new,
                event: BackendEvent::Snapshot(data)
            })
            .unwrap()
        );
        assert!(s.conversation().is_some());
        assert!(s.draft().is_none());
    }
    #[test]
    fn snapshot_reorder_and_channel_move_keep_selection_draft_and_reply() {
        let mut s = AppState::default();
        s.dispatch(AppAction::Select(Region::Conversation, 2));
        s.dispatch(AppAction::Reply);
        s.dispatch(AppAction::UpdateDraft("belongs to this channel".into()));
        let conversation = s.conversation();
        let selected = s.messages()[s.message].id;
        let mut next = s.data.clone();
        next.servers.reverse();
        let mut moved = next.servers.last_mut().unwrap().channels.remove(0);
        moved.name = "renamed".into();
        moved.messages.reverse();
        next.servers[0].channels.insert(1, moved);
        s.replace_snapshot(next).unwrap();
        assert_eq!(s.conversation(), conversation);
        assert_eq!(s.messages()[s.message].id, selected);
        assert_eq!(s.reply_target().unwrap().id, selected);
        assert_eq!(s.draft().unwrap().text, "belongs to this channel");
        assert_eq!((s.server, s.channel), (0, 1));
    }
    #[test]
    fn removal_drops_draft_and_context_without_retargeting_a_reply() {
        let mut s = AppState::default();
        s.dispatch(AppAction::Reply);
        s.dispatch(AppAction::UpdateDraft("removed channel".into()));
        s.dispatch(AppAction::Context);
        let removed = s.channels()[0].clone();
        let mut next = s.data.clone();
        next.servers[0].channels.remove(0);
        s.replace_snapshot(next).unwrap();
        assert!(s.drafts.is_empty());
        assert!(s.draft().is_none());
        assert_eq!(s.screen, Screen::Main);
        let mut next = s.data.clone();
        next.servers[0].channels.insert(0, removed);
        s.replace_snapshot(next).unwrap();
        s.dispatch(AppAction::Select(Region::Channels, 0));
        assert!(s.draft().is_none());
    }
    #[test]
    fn switching_accounts_clears_drafts_even_with_identical_channel_ids() {
        let mut s = AppState::default();
        s.dispatch(AppAction::UpdateDraft("private account draft".into()));
        let old = s.conversation();
        let mut next = s.data.clone();
        next.account = discord_core::AccountId::new(2);
        s.replace_snapshot(next).unwrap();
        assert_ne!(s.conversation(), old);
        assert!(s.drafts.is_empty());
    }
    #[test]
    fn malformed_snapshot_does_not_replace_navigation_or_drafts() {
        let mut s = AppState::default();
        s.dispatch(AppAction::UpdateDraft("preserve me".into()));
        let old = s.conversation();
        for expected in [
            SnapshotError::DuplicateServer,
            SnapshotError::DuplicateChannel,
            SnapshotError::DuplicateMessage,
        ] {
            let mut next = s.data.clone();
            match expected {
                SnapshotError::DuplicateServer => next.servers[1].id = next.servers[0].id,
                SnapshotError::DuplicateChannel => {
                    next.servers[1].channels[0].id = next.servers[0].channels[0].id
                }
                SnapshotError::DuplicateMessage => {
                    next.servers[1].channels[0].messages[0].id =
                        next.servers[0].channels[0].messages[0].id
                }
            }
            assert_eq!(s.replace_snapshot(next), Err(expected));
            assert_eq!(s.conversation(), old);
            assert_eq!(s.draft().unwrap().text, "preserve me");
        }
    }
    #[test]
    fn exhausted_local_ids_preserve_unsent_text_without_overflow() {
        let mut data = AppState::default().data;
        data.servers[0].channels[0].messages[0].id = MessageId::new(u64::MAX);
        let mut s = AppState::new(data);
        let count = s.messages().len();
        s.dispatch(AppAction::UpdateDraft("still unsent".into()));
        s.dispatch(AppAction::SendDraft);
        assert_eq!(s.messages().len(), count);
        assert_eq!(s.draft().unwrap().text, "still unsent");
        assert_eq!(s.notice, "Local message IDs exhausted");
    }
    #[test]
    fn drafts_are_isolated_by_channel_and_server() {
        let mut s = AppState::default();
        s.dispatch(AppAction::UpdateDraft("one\ntwo".into()));
        s.dispatch(AppAction::ReplyTo(0));
        s.dispatch(AppAction::SwitchChannel(1));
        assert!(s.draft().is_none());
        s.dispatch(AppAction::UpdateDraft("other channel".into()));
        s.dispatch(AppAction::SwitchServer(1));
        assert!(s.draft().is_none());
        s.dispatch(AppAction::SwitchServer(-1));
        assert_eq!(s.draft().unwrap().text, "one\ntwo");
        assert_eq!(s.reply_target().unwrap().id, MessageId::new(0));
        s.dispatch(AppAction::SwitchChannel(1));
        assert_eq!(s.draft().unwrap().text, "other channel");
    }
    #[test]
    fn sending_keeps_line_breaks_and_reply_reference_and_clears_only_this_draft() {
        let mut s = AppState::default();
        s.dispatch(AppAction::SwitchChannel(1));
        s.dispatch(AppAction::UpdateDraft("keep this draft".into()));
        s.dispatch(AppAction::SwitchChannel(-1));
        s.dispatch(AppAction::ReplyTo(1));
        s.dispatch(AppAction::UpdateDraft("first line\nsecond line".into()));
        s.dispatch(AppAction::SendDraft);
        let sent = s.messages().last().unwrap();
        assert_eq!(sent.text, "first line\nsecond line");
        assert_eq!(sent.reply_to, Some(MessageId::new(1)));
        assert!(s.draft().is_none());
        s.dispatch(AppAction::SwitchChannel(1));
        assert_eq!(s.draft().unwrap().text, "keep this draft");
    }
    #[test]
    fn blank_send_and_cancel_preserve_draft_text() {
        let mut s = AppState::default();
        let count = s.messages().len();
        s.dispatch(AppAction::ReplyTo(0));
        s.dispatch(AppAction::UpdateDraft(" \n ".into()));
        s.dispatch(AppAction::SendDraft);
        assert_eq!(s.messages().len(), count);
        assert_eq!(s.draft().unwrap().reply_to, Some(MessageId::new(0)));
        s.dispatch(AppAction::CancelReply);
        assert_eq!(s.draft().unwrap().text, " \n ");
        assert!(s.draft().unwrap().reply_to.is_none());
        s.dispatch(AppAction::UpdateDraft(String::new()));
        assert!(s.drafts.is_empty());
    }
    #[test]
    fn expired_or_invalid_reply_never_points_to_another_message() {
        let mut s = AppState::default();
        s.dispatch(AppAction::ReplyTo(999999));
        assert!(s.draft().is_none());
        for _ in 0..194 {
            s.dispatch(AppAction::Submit("filler".into()));
        }
        s.dispatch(AppAction::ReplyTo(0));
        s.data.servers[0].channels[0].messages.remove(0);
        assert!(s.reply_target().is_none());
        s.dispatch(AppAction::UpdateDraft("a reply after eviction".into()));
        s.dispatch(AppAction::SendDraft);
        assert!(s.messages().last().unwrap().reply_to.is_none());
    }
    #[test]
    fn unicode_drafts_have_a_character_budget_and_invalid_channels_allocate_nothing() {
        let mut s = AppState::default();
        s.dispatch(AppAction::UpdateDraft("漢".repeat(2100)));
        assert_eq!(s.draft().unwrap().text.chars().count(), 2000);
        let mut empty = AppState::new(Snapshot {
            avatar: Default::default(),
            account: discord_core::AccountId::new(1),
            servers: vec![],
            profile: "Test".into(),
        });
        empty.dispatch(AppAction::UpdateDraft("ignored".into()));
        empty.dispatch(AppAction::ReplyTo(0));
        empty.dispatch(AppAction::SendDraft);
        assert!(empty.drafts.is_empty());
    }
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
        assert_eq!(s.screen, Screen::Main);
        let target = s.messages()[s.message].id;
        s.dispatch(AppAction::UpdateDraft("Written reply".into()));
        s.dispatch(AppAction::SendDraft);
        assert_eq!(s.messages().last().unwrap().reply_to, Some(target));
        s.dispatch(AppAction::Context);
        s.dispatch(AppAction::SelectMenu(1));
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
    fn escape_cancels_reply_without_losing_text_or_selection() {
        let mut s = AppState::default();
        s.dispatch(AppAction::Select(Region::Conversation, 2));
        s.dispatch(AppAction::Reply);
        s.dispatch(AppAction::UpdateDraft("still writing".into()));
        s.dispatch(AppAction::Back);
        assert!(s.reply_target().is_none());
        assert_eq!(s.draft().unwrap().text, "still writing");
        assert_eq!((s.region, s.message), (Region::Conversation, 2));
        s.dispatch(AppAction::Context);
        s.dispatch(AppAction::Confirm);
        assert_eq!(s.reply_target().unwrap().id, s.messages()[2].id);
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
            avatar: Default::default(),
            account: discord_core::AccountId::new(1),
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
