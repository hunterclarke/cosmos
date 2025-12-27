//! Sync engine for fetching and storing mail
//!
//! Provides idempotent sync operations that can be safely retried.
//! Supports both initial full sync and incremental sync via Gmail History API.

mod inbox;
mod timing;

pub use inbox::{
    // Sync execution
    FetchPhaseStats, ProcessBatchResult, SyncOptions, SyncStats, SyncTiming,
    fetch_phase, process_pending_batch, sync_gmail, sync_gmail_with_progress, incremental_sync,
    // Sync decision (testable)
    SyncAction, SyncStateInfo, ResumeProgress,
    determine_sync_action, should_auto_sync_on_startup, get_sync_state_info,
};
pub use timing::cooldown_elapsed;
