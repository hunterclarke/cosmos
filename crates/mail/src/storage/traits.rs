//! Storage trait definitions

use crate::models::{Message, MessageId, SyncState, Thread, ThreadId};
use anyhow::Result;

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
}
