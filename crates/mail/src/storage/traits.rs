//! Storage trait definitions

use crate::models::{EmailAddress, Message, MessageId, SyncState, Thread, ThreadId};
use anyhow::Result;
use chrono::{DateTime, Utc};

/// A raw message pending processing
///
/// Stores the raw Gmail API response for deferred processing.
/// This allows fetching at max API speed, then processing separately.
#[derive(Debug, Clone)]
pub struct PendingMessage {
    /// Message ID
    pub id: MessageId,
    /// Raw Gmail API JSON response (serialized GmailMessage)
    pub data: Vec<u8>,
    /// Label IDs from the message (for prioritization - e.g., INBOX first)
    pub label_ids: Vec<String>,
}

/// Message metadata without body content (for list views and fast queries)
///
/// This is a lightweight representation of a message that excludes the
/// potentially large body_text and body_html fields. Use this for listing
/// messages and only load full bodies when needed.
#[derive(Debug, Clone)]
pub struct MessageMetadata {
    /// Gmail message ID
    pub id: MessageId,
    /// ID of the thread this message belongs to
    pub thread_id: ThreadId,
    /// Sender's email address
    pub from: EmailAddress,
    /// Recipients (To field)
    pub to: Vec<EmailAddress>,
    /// CC recipients
    pub cc: Vec<EmailAddress>,
    /// Subject line
    pub subject: String,
    /// Plain text preview of the body (snippet)
    pub body_preview: String,
    /// When the message was received
    pub received_at: DateTime<Utc>,
    /// Gmail's internal timestamp (milliseconds since epoch)
    pub internal_date: i64,
    /// Gmail label IDs (e.g., "INBOX", "SENT", "UNREAD")
    pub label_ids: Vec<String>,
    /// Whether plain text body exists in blob storage
    pub has_body_text: bool,
    /// Whether HTML body exists in blob storage
    pub has_body_html: bool,
}

impl MessageMetadata {
    /// Convert to a full Message by adding body content
    pub fn with_body(self, body: MessageBody) -> Message {
        Message {
            id: self.id,
            thread_id: self.thread_id,
            from: self.from,
            to: self.to,
            cc: self.cc,
            subject: self.subject,
            body_preview: self.body_preview,
            body_text: body.text,
            body_html: body.html,
            received_at: self.received_at,
            internal_date: self.internal_date,
            label_ids: self.label_ids,
        }
    }
}

impl From<&Message> for MessageMetadata {
    fn from(msg: &Message) -> Self {
        Self {
            id: msg.id.clone(),
            thread_id: msg.thread_id.clone(),
            from: msg.from.clone(),
            to: msg.to.clone(),
            cc: msg.cc.clone(),
            subject: msg.subject.clone(),
            body_preview: msg.body_preview.clone(),
            received_at: msg.received_at,
            internal_date: msg.internal_date,
            label_ids: msg.label_ids.clone(),
            has_body_text: msg.body_text.is_some(),
            has_body_html: msg.body_html.is_some(),
        }
    }
}

/// Message body content (loaded separately from metadata)
#[derive(Debug, Clone, Default)]
pub struct MessageBody {
    /// Full plain text body content
    pub text: Option<String>,
    /// Full HTML body content
    pub html: Option<String>,
}

impl MessageBody {
    /// Create an empty body
    pub fn empty() -> Self {
        Self::default()
    }

    /// Create a body with just text
    pub fn text(text: String) -> Self {
        Self {
            text: Some(text),
            html: None,
        }
    }

    /// Create a body with just HTML
    pub fn html(html: String) -> Self {
        Self {
            text: None,
            html: Some(html),
        }
    }

    /// Create a body with both text and HTML
    pub fn both(text: String, html: String) -> Self {
        Self {
            text: Some(text),
            html: Some(html),
        }
    }
}

/// Trait for mail storage operations
///
/// This trait abstracts over different storage backends (in-memory, database, etc.)
/// and provides the core CRUD operations needed for mail entities.
pub trait MailStore: Send + Sync {
    /// Insert or update a thread
    fn upsert_thread(&self, thread: Thread) -> Result<()>;

    /// Insert or update a message
    fn upsert_message(&self, message: Message) -> Result<()>;

    /// Link a message to its thread
    fn link_message_to_thread(&self, msg_id: &MessageId, thread_id: &ThreadId) -> Result<()>;

    /// Get a thread by ID
    fn get_thread(&self, id: &ThreadId) -> Result<Option<Thread>>;

    /// Get a message by ID (includes body content)
    ///
    /// This loads the full message including body content from blob storage.
    /// For list views, use `get_message_metadata` instead.
    fn get_message(&self, id: &MessageId) -> Result<Option<Message>>;

    /// Get message metadata only (without body content)
    ///
    /// Fast operation that only reads from the database, not blob storage.
    /// Use this for list views and when you don't need the full body.
    fn get_message_metadata(&self, id: &MessageId) -> Result<Option<MessageMetadata>>;

    /// Get just the body content for a message
    ///
    /// Use this when you already have metadata and just need the body.
    fn get_message_body(&self, id: &MessageId) -> Result<Option<MessageBody>>;

    /// List threads, ordered by last_message_at descending
    fn list_threads(&self, limit: usize, offset: usize) -> Result<Vec<Thread>>;

    /// List threads that have at least one message with the given label
    /// Returns threads ordered by last_message_at descending
    fn list_threads_by_label(
        &self,
        label: &str,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<Thread>>;

    /// List message metadata for a thread, ordered by received_at ascending
    ///
    /// Returns lightweight metadata without body content. Use this for
    /// displaying message lists in a thread view.
    fn list_messages_for_thread(&self, thread_id: &ThreadId) -> Result<Vec<MessageMetadata>>;

    /// List full messages for a thread (with bodies), ordered by received_at ascending
    ///
    /// More expensive - loads body content from blob storage for each message.
    /// Use this when you need to render full message content.
    fn list_messages_for_thread_with_bodies(
        &self,
        thread_id: &ThreadId,
    ) -> Result<Vec<Message>>;

    /// Check if a message exists
    fn has_message(&self, id: &MessageId) -> Result<bool>;

    /// Count total threads
    fn count_threads(&self) -> Result<usize>;

    /// Count messages in a thread
    fn count_messages_in_thread(&self, thread_id: &ThreadId) -> Result<usize>;

    /// Clear all data (for testing)
    fn clear(&self) -> Result<()>;

    // === Phase 2: Sync State Methods ===

    /// Get sync state for an account
    fn get_sync_state(&self, account_id: &str) -> Result<Option<SyncState>>;

    /// Save sync state (upsert)
    fn save_sync_state(&self, state: SyncState) -> Result<()>;

    /// Delete sync state for an account
    fn delete_sync_state(&self, account_id: &str) -> Result<()>;

    /// Check if thread exists by external ID
    fn has_thread(&self, id: &ThreadId) -> Result<bool>;

    /// Clear all mail data (messages and threads) but preserve sync state
    fn clear_mail_data(&self) -> Result<()>;

    // === Phase 3: Mutation Support Methods ===

    /// Get all message IDs for a thread
    ///
    /// Used for batch operations like archiving all messages in a thread.
    fn get_message_ids_for_thread(&self, thread_id: &ThreadId) -> Result<Vec<MessageId>>;

    /// Update labels on a message
    ///
    /// Replaces the entire label_ids array on the message.
    /// Also updates thread-level is_unread flag if UNREAD label changes.
    fn update_message_labels(&self, message_id: &MessageId, label_ids: Vec<String>) -> Result<()>;

    /// Delete a message by ID
    ///
    /// Also updates the thread's message_count. If this was the last message
    /// in the thread, the thread is also deleted.
    fn delete_message(&self, message_id: &MessageId) -> Result<()>;

    // === Phase 4: Pending Message Queue (Decoupled Fetch/Process) ===

    /// Store a raw message for deferred processing
    ///
    /// This allows fetching at max Gmail API speed without blocking on processing.
    /// Messages are stored with their label_ids for prioritization (INBOX first).
    fn store_pending_message(&self, id: &MessageId, data: &[u8], label_ids: Vec<String>)
        -> Result<()>;

    /// Check if a pending message exists
    fn has_pending_message(&self, id: &MessageId) -> Result<bool>;

    /// Get pending messages for processing, prioritized by label
    ///
    /// Returns messages with the given label first. If label is None, returns
    /// messages in arbitrary order. Limit controls batch size.
    fn get_pending_messages(&self, label: Option<&str>, limit: usize) -> Result<Vec<PendingMessage>>;

    /// Delete a pending message after successful processing
    ///
    /// Call this after the message has been normalized and stored to free storage.
    fn delete_pending_message(&self, id: &MessageId) -> Result<()>;

    /// Count pending messages (optionally filtered by label)
    fn count_pending_messages(&self, label: Option<&str>) -> Result<usize>;

    /// Clear all pending messages
    fn clear_pending_messages(&self) -> Result<()>;
}
