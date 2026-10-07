//! Stable identities. Wire adapters must parse decimal IDs without using floats.
macro_rules! id {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(u64);
        impl $name {
            pub const fn new(value: u64) -> Self {
                Self(value)
            }
            pub const fn get(self) -> u64 {
                self.0
            }
        }
    };
}
id!(AccountId);
id!(ServerId);
id!(ChannelId);
id!(MessageId);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ConversationId {
    pub account: AccountId,
    pub channel: ChannelId,
}
