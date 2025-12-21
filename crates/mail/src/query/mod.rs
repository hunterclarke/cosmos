//! Query API for UI consumption
//!
//! Provides high-level query functions that return data formatted
//! for display in the UI.

mod threads;

pub use threads::{ThreadDetail, ThreadSummary, get_thread_detail, list_threads, list_threads_by_label};
