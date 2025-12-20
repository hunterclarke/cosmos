//! Domain models for mail entities

mod message;
mod thread;

pub use message::{EmailAddress, Message, MessageId};
pub use thread::{Thread, ThreadId};
