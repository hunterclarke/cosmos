//! Mail crate - Business logic for email operations
//!
//! This crate provides platform-independent mail functionality including:
//! - Domain models (Thread, Message, EmailAddress)
//! - Gmail API client and OAuth authentication
//! - Storage trait abstractions
//! - Idempotent sync engine
//! - Query API for UI consumption
//! - Action handlers for mutations (archive, star, read/unread)
//!
//! This crate has zero UI dependencies and is designed to be UniFFI-ready
//! for future mobile support.

pub mod actions;
pub mod config;
pub mod gmail;
pub mod models;
pub mod query;
pub mod search;
pub mod storage;
pub mod sync;

pub use actions::ActionHandler;
pub use config::GmailCredentials;
pub use gmail::{GmailAuth, GmailClient, HistoryExpiredError, api::ProfileResponse};
pub use models::{label_icon, label_sort_order, Account, EmailAddress, Label, LabelId, Message, MessageId, SyncState, Thread, ThreadId};
pub use query::{ThreadDetail, ThreadSummary, get_thread_detail, list_threads, list_threads_by_label};
pub use search::{FieldHighlight, HighlightSpan, ParsedQuery, SearchIndex, SearchResult, parse_query, search_threads};
pub use storage::{
    BlobKey, BlobStore, ContentType, FileBlobStore, InMemoryMailStore, MailStore,
    MessageBody, MessageMetadata, PendingMessage, SqliteMailStore,
};
pub use sync::{
    // Sync execution
    FetchPhaseStats, ProcessBatchResult, SyncOptions, SyncStats, SyncTiming,
    fetch_phase, process_pending_batch, sync_gmail, incremental_sync,
    // Sync decision (for app startup logic)
    SyncAction, SyncStateInfo, ResumeProgress,
    determine_sync_action, should_auto_sync_on_startup, get_sync_state_info,
    // Sync timing (for UI cooldown management)
    cooldown_elapsed,
};
