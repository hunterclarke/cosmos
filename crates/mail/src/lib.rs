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
pub mod search;
pub mod storage;
pub mod sync;

pub use config::GmailCredentials;
pub use gmail::{GmailAuth, GmailClient, HistoryExpiredError};
pub use models::{label_icon, label_sort_order, EmailAddress, Label, LabelId, Message, MessageId, SyncState, Thread, ThreadId};
pub use query::{ThreadDetail, ThreadSummary, get_thread_detail, list_threads, list_threads_by_label};
pub use search::{FieldHighlight, HighlightSpan, ParsedQuery, SearchIndex, SearchResult, parse_query, search_threads};
pub use storage::{HeedMailStore, InMemoryMailStore, MailStore, RedbMailStore};
pub use sync::{SyncOptions, SyncStats, sync_gmail, sync_inbox};
