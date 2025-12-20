//! Storage traits and implementations
//!
//! This module defines the storage abstraction layer for mail entities.
//! The trait-based design allows swapping between in-memory and persistent
//! storage implementations.

mod memory;
mod traits;

pub use memory::InMemoryMailStore;
pub use traits::MailStore;
