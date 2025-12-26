//! Sync timing utilities for cooldown management
//!
//! Pure functions that can be tested without UI dependencies.

use chrono::{DateTime, Utc};

/// Check if enough time has elapsed since the last sync to allow a new sync.
///
/// # Arguments
/// * `last_sync_at` - When the last successful sync completed (None if never synced)
/// * `cooldown_secs` - Minimum seconds that must elapse between syncs
///
/// # Returns
/// `true` if enough time has passed (or never synced), `false` if still in cooldown
pub fn cooldown_elapsed(last_sync_at: Option<DateTime<Utc>>, cooldown_secs: u64) -> bool {
    match last_sync_at {
        Some(last) => {
            let elapsed = Utc::now() - last;
            elapsed.num_seconds() >= cooldown_secs as i64
        }
        None => true, // Never synced, so cooldown has "elapsed"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn test_cooldown_elapsed_never_synced() {
        // If we've never synced, cooldown should be considered elapsed
        assert!(cooldown_elapsed(None, 30));
        assert!(cooldown_elapsed(None, 0));
        assert!(cooldown_elapsed(None, 3600));
    }

    #[test]
    fn test_cooldown_elapsed_recent_sync() {
        // Sync happened 10 seconds ago, cooldown is 30 seconds
        let last_sync = Utc::now() - Duration::seconds(10);
        assert!(!cooldown_elapsed(Some(last_sync), 30));

        // Sync happened 1 second ago, cooldown is 30 seconds
        let last_sync = Utc::now() - Duration::seconds(1);
        assert!(!cooldown_elapsed(Some(last_sync), 30));
    }

    #[test]
    fn test_cooldown_elapsed_old_sync() {
        // Sync happened 60 seconds ago, cooldown is 30 seconds
        let last_sync = Utc::now() - Duration::seconds(60);
        assert!(cooldown_elapsed(Some(last_sync), 30));

        // Sync happened exactly at the cooldown boundary
        let last_sync = Utc::now() - Duration::seconds(30);
        assert!(cooldown_elapsed(Some(last_sync), 30));
    }

    #[test]
    fn test_cooldown_elapsed_zero_cooldown() {
        // Zero cooldown means always elapsed
        let last_sync = Utc::now();
        assert!(cooldown_elapsed(Some(last_sync), 0));
    }

    #[test]
    fn test_cooldown_elapsed_very_old_sync() {
        // Sync happened a long time ago
        let last_sync = Utc::now() - Duration::hours(24);
        assert!(cooldown_elapsed(Some(last_sync), 60));
    }
}
