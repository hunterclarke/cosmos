//! Storage trait definitions

use crate::models::{Message, MessageId, SyncState, Thread, ThreadId};
use anyhow::Result;

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

    /// Get a message by ID
    fn get_message(&self, id: &MessageId) -> Result<Option<Message>>;

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

    /// List messages for a thread, ordered by received_at ascending
    fn list_messages_for_thread(&self, thread_id: &ThreadId) -> Result<Vec<Message>>;

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
