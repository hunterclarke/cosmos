//! FFI-friendly type wrappers for UniFFI export
//!
//! These types convert internal Rust types to FFI-compatible versions:
//! - `DateTime<Utc>` → `i64` (Unix timestamp)
//! - `ThreadId`/`MessageId` → `String`
//! - Complex enums → simpler representations

use crate::models::{Account, EmailAddress, Label, Message, SyncState, Thread};
use crate::query::{ThreadDetail, ThreadSummary};
use crate::search::{FieldHighlight, HighlightSpan, SearchResult};
use crate::sync::SyncStats;

// ============================================================================
// Error Types
// ============================================================================

/// FFI-friendly error type
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum MailError {
    #[error("Database error: {message}")]
    Database { message: String },

    #[error("Network error: {message}")]
    Network { message: String },

    #[error("Authentication required")]
    AuthRequired,

    #[error("Not found: {resource}")]
    NotFound { resource: String },

    #[error("Invalid argument: {message}")]
    InvalidArgument { message: String },

    #[error("Sync error: {message}")]
    Sync { message: String },
}

impl From<anyhow::Error> for MailError {
    fn from(e: anyhow::Error) -> Self {
        // Check for specific error types
        let msg = e.to_string();
        if msg.contains("database") || msg.contains("sqlite") || msg.contains("SQL") {
            MailError::Database { message: msg }
        } else if msg.contains("network") || msg.contains("connection") || msg.contains("HTTP") {
            MailError::Network { message: msg }
        } else {
            MailError::Database { message: msg }
        }
    }
}

// ============================================================================
// Account Types
// ============================================================================

/// FFI-friendly account representation
#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiAccount {
    pub id: i64,
    pub email: String,
    pub display_name: Option<String>,
    pub avatar_color: String,
    pub is_primary: bool,
    /// Unix timestamp (seconds since epoch)
    pub added_at: i64,
}

impl From<Account> for FfiAccount {
    fn from(a: Account) -> Self {
        Self {
            id: a.id,
            email: a.email,
            display_name: a.display_name,
            avatar_color: a.avatar_color,
            is_primary: a.is_primary,
            added_at: a.added_at.timestamp(),
        }
    }
}

// ============================================================================
// Email Address
// ============================================================================

/// FFI-friendly email address
#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiEmailAddress {
    pub name: Option<String>,
    pub email: String,
}

impl From<EmailAddress> for FfiEmailAddress {
    fn from(e: EmailAddress) -> Self {
        Self {
            name: e.name,
            email: e.email,
        }
    }
}

impl From<FfiEmailAddress> for EmailAddress {
    fn from(e: FfiEmailAddress) -> Self {
        match e.name {
            Some(name) => EmailAddress::with_name(name, e.email),
            None => EmailAddress::new(e.email),
        }
    }
}

// ============================================================================
// Label Types
// ============================================================================

/// FFI-friendly label representation
#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiLabel {
    pub id: String,
    pub name: String,
    pub is_system: bool,
    pub message_count: u32,
    pub unread_count: u32,
}

impl From<Label> for FfiLabel {
    fn from(l: Label) -> Self {
        Self {
            id: l.id.0,
            name: l.name,
            is_system: l.is_system,
            message_count: l.message_count,
            unread_count: l.unread_count,
        }
    }
}

// ============================================================================
// Thread Types
// ============================================================================

/// FFI-friendly thread representation
#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiThread {
    pub id: String,
    pub account_id: i64,
    pub subject: String,
    pub snippet: String,
    /// Unix timestamp (seconds since epoch)
    pub last_message_at: i64,
    pub message_count: u32,
    pub sender_name: Option<String>,
    pub sender_email: String,
    pub is_unread: bool,
}

impl From<Thread> for FfiThread {
    fn from(t: Thread) -> Self {
        Self {
            id: t.id.0,
            account_id: t.account_id,
            subject: t.subject,
            snippet: t.snippet,
            last_message_at: t.last_message_at.timestamp(),
            message_count: t.message_count as u32,
            sender_name: t.sender_name,
            sender_email: t.sender_email,
            is_unread: t.is_unread,
        }
    }
}

/// FFI-friendly thread summary for list views
#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiThreadSummary {
    pub id: String,
    pub account_id: i64,
    pub subject: String,
    pub snippet: String,
    /// Unix timestamp (seconds since epoch)
    pub last_message_at: i64,
    pub message_count: u32,
    pub sender_name: Option<String>,
    pub sender_email: String,
    pub is_unread: bool,
}

impl From<ThreadSummary> for FfiThreadSummary {
    fn from(t: ThreadSummary) -> Self {
        Self {
            id: t.id.0,
            account_id: t.account_id,
            subject: t.subject,
            snippet: t.snippet,
            last_message_at: t.last_message_at.timestamp(),
            message_count: t.message_count as u32,
            sender_name: t.sender_name,
            sender_email: t.sender_email,
            is_unread: t.is_unread,
        }
    }
}

/// FFI-friendly thread detail with messages
#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiThreadDetail {
    pub thread: FfiThread,
    pub messages: Vec<FfiMessage>,
}

impl From<ThreadDetail> for FfiThreadDetail {
    fn from(d: ThreadDetail) -> Self {
        Self {
            thread: d.thread.into(),
            messages: d.messages.into_iter().map(FfiMessage::from).collect(),
        }
    }
}

// ============================================================================
// Message Types
// ============================================================================

/// FFI-friendly message representation
#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiMessage {
    pub id: String,
    pub thread_id: String,
    pub account_id: i64,
    pub from: FfiEmailAddress,
    pub to: Vec<FfiEmailAddress>,
    pub cc: Vec<FfiEmailAddress>,
    pub subject: String,
    pub body_preview: String,
    pub body_text: Option<String>,
    pub body_html: Option<String>,
    /// Unix timestamp (seconds since epoch)
    pub received_at: i64,
    pub internal_date: i64,
    pub label_ids: Vec<String>,
}

impl From<Message> for FfiMessage {
    fn from(m: Message) -> Self {
        Self {
            id: m.id.0,
            thread_id: m.thread_id.0,
            account_id: m.account_id,
            from: m.from.into(),
            to: m.to.into_iter().map(FfiEmailAddress::from).collect(),
            cc: m.cc.into_iter().map(FfiEmailAddress::from).collect(),
            subject: m.subject,
            body_preview: m.body_preview,
            body_text: m.body_text,
            body_html: m.body_html,
            received_at: m.received_at.timestamp(),
            internal_date: m.internal_date,
            label_ids: m.label_ids,
        }
    }
}

// ============================================================================
// Sync Types
// ============================================================================

/// FFI-friendly sync state
#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiSyncState {
    pub account_id: i64,
    pub history_id: String,
    /// Unix timestamp (seconds since epoch)
    pub last_sync_at: i64,
    pub sync_version: u32,
    pub initial_sync_complete: bool,
}

impl From<SyncState> for FfiSyncState {
    fn from(s: SyncState) -> Self {
        Self {
            account_id: s.account_id,
            history_id: s.history_id,
            last_sync_at: s.last_sync_at.timestamp(),
            sync_version: s.sync_version,
            initial_sync_complete: s.initial_sync_complete,
        }
    }
}

/// FFI-friendly sync statistics
#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiSyncStats {
    pub messages_fetched: u32,
    pub messages_created: u32,
    pub messages_updated: u32,
    pub messages_skipped: u32,
    pub labels_updated: u32,
    pub threads_created: u32,
    pub threads_updated: u32,
    pub was_incremental: bool,
    pub errors: u32,
    pub duration_ms: u64,
}

impl From<SyncStats> for FfiSyncStats {
    fn from(s: SyncStats) -> Self {
        Self {
            messages_fetched: s.messages_fetched as u32,
            messages_created: s.messages_created as u32,
            messages_updated: s.messages_updated as u32,
            messages_skipped: s.messages_skipped as u32,
            labels_updated: s.labels_updated as u32,
            threads_created: s.threads_created as u32,
            threads_updated: s.threads_updated as u32,
            was_incremental: s.was_incremental,
            errors: s.errors as u32,
            duration_ms: s.duration_ms,
        }
    }
}

// ============================================================================
// Search Types
// ============================================================================

/// FFI-friendly highlight span
#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiHighlightSpan {
    pub start: u32,
    pub end: u32,
}

impl From<HighlightSpan> for FfiHighlightSpan {
    fn from(h: HighlightSpan) -> Self {
        Self {
            start: h.start as u32,
            end: h.end as u32,
        }
    }
}

/// FFI-friendly field highlight
#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiFieldHighlight {
    pub field: String,
    pub text: String,
    pub highlights: Vec<FfiHighlightSpan>,
}

impl From<FieldHighlight> for FfiFieldHighlight {
    fn from(f: FieldHighlight) -> Self {
        Self {
            field: f.field,
            text: f.text,
            highlights: f.highlights.into_iter().map(FfiHighlightSpan::from).collect(),
        }
    }
}

/// FFI-friendly search result
#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiSearchResult {
    pub thread_id: String,
    pub subject: String,
    pub snippet: String,
    /// Unix timestamp (seconds since epoch)
    pub last_message_at: i64,
    pub message_count: u32,
    pub sender_name: Option<String>,
    pub sender_email: String,
    pub is_unread: bool,
    pub highlights: Vec<FfiFieldHighlight>,
    pub score: f32,
}

impl From<SearchResult> for FfiSearchResult {
    fn from(r: SearchResult) -> Self {
        Self {
            thread_id: r.thread_id.0,
            subject: r.subject,
            snippet: r.snippet,
            last_message_at: r.last_message_at.timestamp(),
            message_count: r.message_count as u32,
            sender_name: r.sender_name,
            sender_email: r.sender_email,
            is_unread: r.is_unread,
            highlights: r.highlights.into_iter().map(FfiFieldHighlight::from).collect(),
            score: r.score,
        }
    }
}

// ============================================================================
// Callback Traits
// ============================================================================

/// FFI-friendly fetch phase statistics (for concurrent sync)
#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiFetchStats {
    /// Messages successfully fetched and stored as pending
    pub messages_fetched: u32,
    /// Messages currently pending processing
    pub messages_pending: u32,
    /// Messages skipped (already synced)
    pub messages_skipped: u32,
}

/// FFI-friendly process batch result (for concurrent sync)
#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiProcessBatchResult {
    /// Number of messages processed in this batch
    pub processed: u32,
    /// Number of messages remaining to process
    pub remaining: u32,
    /// Number of errors in this batch
    pub errors: u32,
    /// Whether there are more messages to process
    pub has_more: bool,
}

/// Callback interface for sync progress updates
#[uniffi::export(callback_interface)]
pub trait SyncProgressCallback: Send + Sync {
    /// Called when sync progress updates
    fn on_progress(&self, fetched: u32, total: Option<u32>, phase: String);
    /// Called when an error occurs during sync
    fn on_error(&self, message: String);
}

// ============================================================================
// Log Callback
// ============================================================================

/// Log level for FFI callback
#[derive(Debug, Clone, Copy, uniffi::Enum)]
pub enum FfiLogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl From<log::Level> for FfiLogLevel {
    fn from(level: log::Level) -> Self {
        match level {
            log::Level::Error => FfiLogLevel::Error,
            log::Level::Warn => FfiLogLevel::Warn,
            log::Level::Info => FfiLogLevel::Info,
            log::Level::Debug => FfiLogLevel::Debug,
            log::Level::Trace => FfiLogLevel::Trace,
        }
    }
}

/// Callback interface for receiving log messages from Rust
///
/// Swift should implement this using os_log/Logger for unified logging.
#[uniffi::export(callback_interface)]
pub trait LogCallback: Send + Sync {
    /// Called when a log message is emitted
    ///
    /// # Arguments
    /// * `level` - The log level (error, warn, info, debug, trace)
    /// * `target` - The logging target (typically module path, e.g., "mail::sync")
    /// * `message` - The log message
    fn on_log(&self, level: FfiLogLevel, target: String, message: String);
}
