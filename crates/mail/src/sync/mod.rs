//! Sync engine for fetching and storing mail
//!
//! Provides idempotent sync operations that can be safely retried.
//! Supports both initial full sync and incremental sync via Gmail History API.

mod inbox;

pub use inbox::{
    FetchPhaseStats, ProcessBatchResult, SyncOptions, SyncStats, SyncTiming,
    fetch_phase, process_pending_batch, sync_gmail, sync_inbox,
};
