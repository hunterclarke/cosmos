//! Sync state tracking for incremental Gmail sync

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Tracks sync progress for a Gmail account
///
/// Persisted separately from messages to enable incremental sync.
/// Only one SyncState per Gmail account.
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
        }
    }

    /// Mark initial sync as complete
    ///
    /// Uses the history_id already stored in the partial state.
    pub fn mark_complete(mut self) -> Self {
        self.last_sync_at = Utc::now();
        self.initial_sync_complete = true;
        self
    }

    /// Update with new history_id after successful sync
    pub fn updated(mut self, history_id: impl Into<String>) -> Self {
        self.history_id = history_id.into();
        self.last_sync_at = Utc::now();
        self
    }

    /// Check if this state is recent enough to be useful
    /// Gmail history IDs typically expire after about a week
    pub fn is_recent(&self) -> bool {
        let age = Utc::now() - self.last_sync_at;
        age.num_days() < 7
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
