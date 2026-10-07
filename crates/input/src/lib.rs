//! Device-independent commands. No Discord or UI dependency.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Region {
    Servers,
    Channels,
    Conversation,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AppAction {
    Navigate(Direction),
    NextRegion,
    PreviousRegion,
    Confirm,
    Back,
    SwitchServer(i32),
    SwitchChannel(i32),
    Context,
    Reply,
    Menu,
    Select(Region, usize),
    SelectMenu(usize),
    ToggleMetrics,
    Submit(String),
    UpdateDraft(String),
    SendDraft,
    ReplyTo(u64),
    CancelReply,
    React,
}
#[derive(Clone, Copy)]
pub enum Key {
    Up,
    Down,
    Left,
    Right,
    Enter,
    Escape,
    Tab,
    Context,
    Reply,
    Menu,
    PreviousServer,
    NextServer,
    PreviousChannel,
    NextChannel,
}
pub fn keyboard(key: Key, shift: bool) -> AppAction {
    match key {
        Key::Up => AppAction::Navigate(Direction::Up),
        Key::Down => AppAction::Navigate(Direction::Down),
        Key::Left => AppAction::Navigate(Direction::Left),
        Key::Right => AppAction::Navigate(Direction::Right),
        Key::Enter => AppAction::Confirm,
        Key::Escape => AppAction::Back,
        Key::Tab if shift => AppAction::PreviousRegion,
        Key::Tab => AppAction::NextRegion,
        Key::Context => AppAction::Context,
        Key::Reply => AppAction::Reply,
        Key::Menu => AppAction::Menu,
        Key::PreviousServer => AppAction::SwitchServer(-1),
        Key::NextServer => AppAction::SwitchServer(1),
        Key::PreviousChannel => AppAction::SwitchChannel(-1),
        Key::NextChannel => AppAction::SwitchChannel(1),
    }
}

#[cfg(feature = "gamepad")]
pub mod gamepad;
