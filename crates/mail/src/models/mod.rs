//! Domain models for mail entities

mod label;
mod message;
mod sync_state;
mod thread;

pub use label::{label_icon, label_sort_order, Label, LabelId};
pub use message::{EmailAddress, Message, MessageId};
pub use sync_state::SyncState;
pub use thread::{Thread, ThreadId};
