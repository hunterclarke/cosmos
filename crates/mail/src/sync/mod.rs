//! Sync engine for fetching and storing mail
//!
//! Provides idempotent sync operations that can be safely retried.

mod inbox;

pub use inbox::{SyncStats, sync_inbox};
