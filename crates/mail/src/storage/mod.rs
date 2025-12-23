//! Storage traits and implementations
//!
//! This module defines the storage abstraction layer for mail entities.
//! The trait-based design allows swapping between in-memory and persistent
//! storage implementations.
//!
//! ## Architecture
//!
//! - **SQLite** stores queryable metadata (threads, messages, labels, sync state)
//! - **Blob storage** stores large content (message bodies, attachments) with compression
//! - **InMemoryMailStore** provides a testing/development implementation

mod blob;
mod blob_file;
mod memory;
mod sqlite;
mod traits;

pub use blob::{BlobKey, BlobStore, ContentType};
pub use blob_file::FileBlobStore;
pub use memory::InMemoryMailStore;
pub use sqlite::SqliteMailStore;
pub use traits::{MailStore, MessageBody, MessageMetadata, PendingMessage};
