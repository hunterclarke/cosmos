//! Sync state tracking for incremental Gmail sync

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Tracks sync progress for a Gmail account
///
/// Persisted separately from messages to enable incremental sync.
/// Only one SyncState per Gmail account.
///
/// ## Resilience Design
///
/// The sync state is designed to survive crashes, restarts, and interruptions:
///
/// - `initial_sync_complete`: Distinguishes between "never synced" vs "partial sync"
/// - `fetch_page_token`: Allows resuming message listing from where we left off
/// - `failed_message_ids`: Tracks messages that failed to fetch for later retry
/// - `messages_listed`: Count of message IDs discovered (for progress tracking)
///
/// After any interruption, `sync_gmail` will:
/// 1. Resume listing from `fetch_page_token` (if set)
/// 2. Retry fetching `failed_message_ids`
/// 3. Process any pending messages from previous fetch
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SyncState {
    /// Gmail user or account identifier
    pub account_id: String,
    /// Gmail historyId for incremental sync
    pub history_id: String,
    /// When we last successfully synced
    pub last_sync_at: DateTime<Utc>,
    /// Schema version for migrations
    pub sync_version: u32,
    /// Whether initial sync has completed (false = still in progress)
    #[serde(default = "default_true")]
    pub initial_sync_complete: bool,
    /// Page token to resume message listing (None = start from beginning or listing complete)
    #[serde(default)]
    pub fetch_page_token: Option<String>,
    /// Message IDs that failed to fetch (non-retriable errors like 404)
    /// These will be retried on next sync attempt
    #[serde(default)]
    pub failed_message_ids: Vec<String>,
    /// Count of message IDs listed so far (for progress tracking)
    #[serde(default)]
    pub messages_listed: usize,
}

fn default_true() -> bool {
    true
}

impl SyncState {
    /// Create a new SyncState after completed initial sync
    pub fn new(account_id: impl Into<String>, history_id: impl Into<String>) -> Self {
        Self {
            account_id: account_id.into(),
            history_id: history_id.into(),
            last_sync_at: Utc::now(),
            sync_version: 1,
            initial_sync_complete: true,
            fetch_page_token: None,
            failed_message_ids: Vec::new(),
            messages_listed: 0,
        }
    }

    /// Create a partial SyncState during initial sync (for resumability)
    ///
    /// The history_id should be captured at the START of initial sync,
    /// so we can run incremental sync after to catch up on any messages
    /// that arrived during the sync.
    pub fn partial(account_id: impl Into<String>, history_id: impl Into<String>) -> Self {
        Self {
            account_id: account_id.into(),
            history_id: history_id.into(),
            last_sync_at: Utc::now(),
            sync_version: 1,
            initial_sync_complete: false,
            fetch_page_token: None,
            failed_message_ids: Vec::new(),
            messages_listed: 0,
        }
    }

    /// Mark initial sync as complete
    ///
    /// Uses the history_id already stored in the partial state.
    /// Clears fetch progress fields since listing is complete.
    pub fn mark_complete(mut self) -> Self {
        self.last_sync_at = Utc::now();
        self.initial_sync_complete = true;
        self.fetch_page_token = None;
        self.failed_message_ids.clear();
        self.messages_listed = 0;
        self
    }

    /// Update with new history_id after successful sync
    pub fn updated(mut self, history_id: impl Into<String>) -> Self {
        self.history_id = history_id.into();
        self.last_sync_at = Utc::now();
        self
    }

    /// Update fetch progress (call after each page of message listing)
    ///
    /// Persisting this allows resuming listing from where we left off after a crash.
    pub fn with_fetch_progress(
        mut self,
        page_token: Option<String>,
        messages_listed: usize,
    ) -> Self {
        self.fetch_page_token = page_token;
        self.messages_listed = messages_listed;
        self.last_sync_at = Utc::now();
        self
    }

    /// Add failed message IDs for later retry
    pub fn with_failed_ids(mut self, failed_ids: Vec<String>) -> Self {
        self.failed_message_ids = failed_ids;
        self
    }

    /// Check if this state is recent enough to be useful
    /// Gmail history IDs typically expire after about a week
    pub fn is_recent(&self) -> bool {
        let age = Utc::now() - self.last_sync_at;
        age.num_days() < 7
    }

    /// Check if there are failed message IDs that need retry
    pub fn has_failed_messages(&self) -> bool {
        !self.failed_message_ids.is_empty()
    }

    /// Check if there's a page token to resume from
    pub fn has_fetch_progress(&self) -> bool {
        self.fetch_page_token.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_sync_state() {
        let state = SyncState::new("user@gmail.com", "12345");
        assert_eq!(state.account_id, "user@gmail.com");
        assert_eq!(state.history_id, "12345");
        assert_eq!(state.sync_version, 1);
    }

    #[test]
    fn test_updated_sync_state() {
        let state = SyncState::new("user@gmail.com", "12345");
        let updated = state.updated("67890");
        assert_eq!(updated.account_id, "user@gmail.com");
        assert_eq!(updated.history_id, "67890");
    }

    #[test]
    fn test_is_recent() {
        let state = SyncState::new("user@gmail.com", "12345");
        assert!(state.is_recent());
    }

    #[test]
    fn test_serialization() {
        let state = SyncState::new("user@gmail.com", "12345");
        let json = serde_json::to_string(&state).unwrap();
        let deserialized: SyncState = serde_json::from_str(&json).unwrap();
        assert_eq!(state.account_id, deserialized.account_id);
        assert_eq!(state.history_id, deserialized.history_id);
    }
}
