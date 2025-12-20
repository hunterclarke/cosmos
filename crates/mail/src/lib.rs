//! Mail crate - Business logic for email operations
//!
//! This crate provides platform-independent mail functionality including:
//! - Domain models (Thread, Message, EmailAddress)
//! - Gmail API client and OAuth authentication
//! - Storage trait abstractions
//! - Idempotent sync engine
//! - Query API for UI consumption
//!
//! This crate has zero UI dependencies and is designed to be UniFFI-ready
//! for future mobile support.

pub mod config;
pub mod gmail;
pub mod models;
pub mod query;
pub mod storage;
pub mod sync;

pub use config::GmailCredentials;
pub use gmail::{GmailAuth, GmailClient};
pub use models::{EmailAddress, Message, MessageId, Thread, ThreadId};
pub use query::{ThreadDetail, ThreadSummary, get_thread_detail, list_threads};
pub use storage::MailStore;
pub use sync::sync_inbox;
