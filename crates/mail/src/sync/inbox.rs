//! Gmail sync implementation
//!
//! Provides both initial full sync and incremental sync via Gmail History API.
//! Syncs the user's entire email library (all labels/folders).

use anyhow::{Context, Result};
use chrono::Utc;
use log::{debug, info, warn};
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;

use crate::gmail::{api::GmailMessage, normalize_message, GmailClient, HistoryExpiredError};
use crate::models::{LabelId, Message, MessageId, SyncState, Thread, ThreadId};
use crate::search::SearchIndex;
use crate::storage::{MailStore, MessageMetadata};

/// Options for sync operation
#[derive(Debug, Clone, Default)]
pub struct SyncOptions {
    /// Maximum messages to fetch in initial sync
    pub max_messages: Option<usize>,
    /// Force full resync even if history_id exists
    pub full_resync: bool,
    /// Filter by label ID (e.g., "INBOX") - for debugging
    pub label_filter: Option<String>,
    /// Optional search index for incremental indexing during sync
    pub search_index: Option<Arc<SearchIndex>>,
}

/// Statistics from a sync operation
#[derive(Debug, Default, Clone)]
pub struct SyncStats {
    /// Number of messages fetched from Gmail
    pub messages_fetched: usize,
    /// Number of new messages created
    pub messages_created: usize,
    /// Number of messages updated
    pub messages_updated: usize,
    /// Number of messages skipped (already synced)
    pub messages_skipped: usize,
    /// Number of label changes applied (from incremental sync)
    pub labels_updated: usize,
    /// Number of threads created
    pub threads_created: usize,
    /// Number of threads updated
    pub threads_updated: usize,
    /// Whether this was an incremental sync
    pub was_incremental: bool,
    /// Number of errors encountered
    pub errors: usize,
    /// Duration of the sync operation in milliseconds
    pub duration_ms: u64,
    /// Timing breakdown for performance analysis
    pub timing: SyncTiming,
}

/// Detailed timing breakdown for sync operations
#[derive(Debug, Default, Clone)]
pub struct SyncTiming {
    /// Total wall-clock time for initial sync phase (ms)
    pub initial_sync_ms: u64,
    /// Total wall-clock time for incremental/catch-up sync phase (ms)
    pub incremental_sync_ms: u64,
    /// Time spent getting Gmail profile/history ID (ms)
    pub profile_ms: u64,
    /// Time spent listing message IDs from Gmail (ms)
    pub list_messages_ms: u64,
    /// Time spent fetching full message content (ms)
    pub fetch_messages_ms: u64,
    /// Time spent normalizing Gmail messages to domain models (ms)
    pub normalize_ms: u64,
    /// Time spent on storage operations (upsert message/thread) (ms)
    pub storage_ms: u64,
    /// Time spent computing thread aggregates (ms)
    pub compute_thread_ms: u64,
    /// Time spent checking if messages exist (ms)
    pub has_message_ms: u64,
    /// Time spent indexing messages for search (ms)
    pub search_index_ms: u64,
    /// Time spent fetching history for incremental sync (ms)
    pub history_ms: u64,
}

// Keep backward compatibility with Phase 1 API
impl SyncStats {
    /// Total messages stored (created + updated) for backward compatibility
    pub fn messages_stored(&self) -> usize {
        self.messages_created + self.messages_updated
    }
}

/// Sync Gmail inbox with incremental support
///
/// Automatically uses incremental sync if SyncState exists,
/// otherwise performs full initial sync.
///
/// # Arguments
/// * `gmail` - Gmail API client
/// * `store` - Storage backend
/// * `account_id` - Account identifier for tracking sync state
/// * `options` - Sync options
pub fn sync_gmail(
    gmail: &GmailClient,
    store: &dyn MailStore,
    account_id: &str,
    options: SyncOptions,
) -> Result<SyncStats> {
    let start = std::time::Instant::now();

    // Check for existing sync state
    let existing_state = store.get_sync_state(account_id)?;

    let mut stats = match existing_state {
        // Full resync requested - start fresh
        _ if options.full_resync => {
            store.clear_mail_data()?;
            store.delete_sync_state(account_id)?;
            initial_sync(gmail, store, account_id, &options)?
        }
        // Incomplete initial sync - resume it
        Some(state) if !state.initial_sync_complete => {
            info!("Resuming incomplete initial sync...");
            initial_sync(gmail, store, account_id, &options)?
        }
        // Complete sync state - try incremental
        Some(state) => {
            match incremental_sync(gmail, store, account_id, &state, &options) {
                Ok(stats) => stats,
                Err(e) if e.downcast_ref::<HistoryExpiredError>().is_some() => {
                    // History ID expired, fall back to full resync
                    warn!("History ID expired, performing full resync");
                    store.clear_mail_data()?;
                    store.delete_sync_state(account_id)?;
                    initial_sync(gmail, store, account_id, &options)?
                }
                Err(e) => return Err(e),
            }
        }
        // No sync state - start initial sync
        None => initial_sync(gmail, store, account_id, &options)?,
    };

    stats.duration_ms = start.elapsed().as_millis() as u64;
    Ok(stats)
}

/// Perform initial full sync using decoupled fetch/process phases
///
/// Phase 1 (Fetch): Downloads messages at max Gmail API speed, stores as pending
/// Phase 2 (Process): Processes pending messages, INBOX first, then the rest
///
/// After completing, runs an incremental catch-up sync for messages that arrived during sync.
fn initial_sync(
    gmail: &GmailClient,
    store: &dyn MailStore,
    account_id: &str,
    options: &SyncOptions,
) -> Result<SyncStats> {
    let sync_start = Instant::now();
    let mut stats = SyncStats {
        was_incremental: false,
        ..Default::default()
    };

    // Check if we're resuming an incomplete sync (already have history_id)
    // or starting fresh (need to capture history_id now)
    let existing_state = store.get_sync_state(account_id)?;
    let start_history_id = match existing_state {
        Some(state) if !state.initial_sync_complete && !state.history_id.is_empty() => {
            info!("Resuming initial sync from history_id: {}", state.history_id);
            state.history_id
        }
        _ => {
            // Get history_id at START of sync so we can catch up on any
            // messages that arrive during the sync
            let profile_start = Instant::now();
            let history_id = get_current_history_id(gmail)?;
            stats.timing.profile_ms += profile_start.elapsed().as_millis() as u64;
            debug!("[SYNC] get_profile: {:?}", profile_start.elapsed());
            info!("Starting initial sync from history_id: {}", history_id);

            // Save partial sync state to indicate initial sync is in progress
            let partial_state = SyncState::partial(account_id, &history_id);
            store.save_sync_state(partial_state)?;
            history_id
        }
    };

    // === PHASE 1: FETCH ===
    // Download messages as fast as Gmail allows, store as pending
    info!("[SYNC] Phase 1: Fetching messages from Gmail...");
    let fetch_stats = fetch_phase(gmail, store, options, &mut stats)?;
    info!("[SYNC] Fetch phase complete: {} fetched, {} pending, {} skipped",
        fetch_stats.fetched, fetch_stats.pending, fetch_stats.skipped);

    // === PHASE 2: PROCESS ===
    // Process pending messages: INBOX first, then the rest
    let pending_count = store.count_pending_messages(None)?;
    info!("[SYNC] Phase 2: Processing {} pending messages (INBOX first)...", pending_count);

    if pending_count == 0 {
        info!("[SYNC] No pending messages to process, skipping process phase");
    } else {
        process_phase(store, options, &mut stats)?;
        info!("[SYNC] Process phase complete: {} messages created, {} threads",
            stats.messages_created, stats.threads_created + stats.threads_updated);
    }

    // Mark initial sync as complete with the history_id we captured at the start
    let partial_state = SyncState::partial(account_id, &start_history_id);
    let complete_state = partial_state.mark_complete();
    store.save_sync_state(complete_state.clone())?;

    // Record total initial sync time
    stats.timing.initial_sync_ms = sync_start.elapsed().as_millis() as u64;

    debug!("[SYNC] initial_sync complete: {:?}", sync_start.elapsed());
    debug!(
        "[SYNC] timing breakdown - profile: {}ms, list: {}ms, fetch: {}ms, normalize: {}ms, storage: {}ms, compute_thread: {}ms, has_message: {}ms, search_index: {}ms, total: {}ms",
        stats.timing.profile_ms,
        stats.timing.list_messages_ms,
        stats.timing.fetch_messages_ms,
        stats.timing.normalize_ms,
        stats.timing.storage_ms,
        stats.timing.compute_thread_ms,
        stats.timing.has_message_ms,
        stats.timing.search_index_ms,
        stats.timing.initial_sync_ms
    );
    info!(
        "Initial sync complete: {} messages fetched, {} created, {} skipped in {}ms",
        stats.messages_fetched,
        stats.messages_created,
        stats.messages_skipped,
        stats.timing.initial_sync_ms
    );

    // Run incremental catch-up sync to get any messages that arrived during initial sync
    info!("Running catch-up sync for messages received during initial sync...");
    match incremental_sync(gmail, store, account_id, &complete_state, options) {
        Ok(catchup_stats) => {
            info!(
                "Catch-up sync complete: {} new messages, {} label updates",
                catchup_stats.messages_created,
                catchup_stats.labels_updated
            );
            // Merge catch-up stats into main stats (including timing)
            stats.messages_fetched += catchup_stats.messages_fetched;
            stats.messages_created += catchup_stats.messages_created;
            stats.messages_updated += catchup_stats.messages_updated;
            stats.labels_updated += catchup_stats.labels_updated;
            stats.threads_created += catchup_stats.threads_created;
            stats.threads_updated += catchup_stats.threads_updated;
            stats.errors += catchup_stats.errors;
            // Merge timing (catch-up is incremental sync)
            stats.timing.incremental_sync_ms += catchup_stats.timing.incremental_sync_ms;
            stats.timing.history_ms += catchup_stats.timing.history_ms;
            stats.timing.fetch_messages_ms += catchup_stats.timing.fetch_messages_ms;
            stats.timing.normalize_ms += catchup_stats.timing.normalize_ms;
            stats.timing.storage_ms += catchup_stats.timing.storage_ms;
            stats.timing.compute_thread_ms += catchup_stats.timing.compute_thread_ms;
            stats.timing.search_index_ms += catchup_stats.timing.search_index_ms;
        }
        Err(e) => {
            // Catch-up sync is best-effort; log but don't fail the whole sync
            warn!("Catch-up sync failed (non-fatal): {}", e);
        }
    }

    Ok(stats)
}

/// Stats from fetch phase
#[derive(Debug, Default, Clone)]
pub struct FetchPhaseStats {
    /// Messages successfully fetched and stored as pending
    pub fetched: usize,
    /// Messages currently pending processing
    pub pending: usize,
    /// Messages skipped (already synced)
    pub skipped: usize,
}

/// Phase 1: Fetch messages from Gmail as fast as possible
///
/// Lists all message IDs, fetches full content in parallel, stores raw JSON as pending.
/// This phase is optimized for maximum Gmail API throughput.
///
/// Call this from a background thread, then call `process_pending_batch` repeatedly
/// to process messages with UI updates between batches.
pub fn fetch_phase(
    gmail: &GmailClient,
    store: &dyn MailStore,
    options: &SyncOptions,
    stats: &mut SyncStats,
) -> Result<FetchPhaseStats> {
    debug!("[SYNC] >>> ENTERED fetch_phase <<<");

    // Log current store state for debugging
    let message_count = store.count_threads().unwrap_or(0);
    let pending_count = store.count_pending_messages(None).unwrap_or(0);
    info!(
        "[SYNC] fetch_phase starting: {} threads in store, {} pending messages",
        message_count, pending_count
    );

    let mut fetch_stats = FetchPhaseStats {
        fetched: 0,
        pending: 0,
        skipped: 0,
    };

    let mut page_token: Option<String> = None;
    let mut total_listed = 0usize;
    let batch_size = 500; // Gmail API max is 500 per page
    let mut batch_num = 0usize;

    debug!("[SYNC] fetch_phase starting, batch_size={}", batch_size);

    loop {
        batch_num += 1;
        let batch_start = Instant::now();
        debug!("[SYNC] Starting fetch batch {}", batch_num);

        // Check if we've hit the limit
        if let Some(max) = options.max_messages {
            if total_listed >= max {
                debug!("[SYNC] Hit max_messages limit: {}", max);
                break;
            }
        }

        // Limit batch size if we have a max_messages constraint
        let effective_batch_size = if let Some(max) = options.max_messages {
            batch_size.min(max - total_listed)
        } else {
            batch_size
        };

        // Fetch a page of message IDs
        debug!("[SYNC] Calling list_messages_with_label...");
        let list_start = Instant::now();
        let list_response = gmail.list_messages_with_label(
            effective_batch_size,
            page_token.as_deref(),
            options.label_filter.as_deref(),
        )?;
        stats.timing.list_messages_ms += list_start.elapsed().as_millis() as u64;
        debug!("[SYNC] list_messages batch {}: {:?}", batch_num, list_start.elapsed());

        let message_refs = list_response.messages.unwrap_or_default();

        if message_refs.is_empty() {
            break;
        }

        total_listed += message_refs.len();
        info!(
            "Listed {} messages (batch of {})",
            total_listed,
            message_refs.len()
        );

        // Filter out already-synced messages (check both processed and pending)
        let has_msg_start = Instant::now();
        let mut to_fetch: Vec<MessageId> = Vec::new();
        for msg_ref in &message_refs {
            let msg_id = MessageId::new(&msg_ref.id);
            // Skip if already processed OR already pending
            if store.has_message(&msg_id)? || store.has_pending_message(&msg_id)? {
                fetch_stats.skipped += 1;
                stats.messages_skipped += 1;
            } else {
                to_fetch.push(msg_id);
            }
        }
        stats.timing.has_message_ms += has_msg_start.elapsed().as_millis() as u64;
        debug!("[SYNC] has_message checks ({} msgs): {:?}", message_refs.len(), has_msg_start.elapsed());

        stats.messages_fetched += message_refs.len();

        info!(
            "[SYNC] Batch {}: {} listed, {} to fetch, {} skipped (total skipped: {})",
            batch_num,
            message_refs.len(),
            to_fetch.len(),
            message_refs.len() - to_fetch.len(),
            fetch_stats.skipped
        );

        if !to_fetch.is_empty() {
            info!("Fetching {} new messages...", to_fetch.len());

            // Fetch in small chunks and store immediately for greedy ingestion
            // This allows the process phase to start working while we're still fetching
            // No artificial delays - let Gmail's rate limiting control the speed
            let chunk_size = 50; // Larger chunks = fewer HTTP requests
            for chunk in to_fetch.chunks(chunk_size) {
                let fetch_start = Instant::now();
                let results = gmail.get_messages_batch(chunk);
                stats.timing.fetch_messages_ms += fetch_start.elapsed().as_millis() as u64;

                // Store immediately after each chunk
                let store_start = Instant::now();
                let mut chunk_stored = 0;
                let mut chunk_errors = 0;
                for result in results {
                    match result {
                        Ok(gmail_msg) => {
                            let msg_id = MessageId::new(&gmail_msg.id);
                            let label_ids = gmail_msg.label_ids.clone().unwrap_or_default();

                            match serde_json::to_vec(&gmail_msg) {
                                Ok(data) => {
                                    if let Err(e) = store.store_pending_message(&msg_id, &data, label_ids) {
                                        warn!("Failed to store pending message {}: {}", msg_id.as_str(), e);
                                        chunk_errors += 1;
                                        stats.errors += 1;
                                    } else {
                                        fetch_stats.fetched += 1;
                                        fetch_stats.pending += 1;
                                        chunk_stored += 1;
                                    }
                                }
                                Err(e) => {
                                    warn!("Failed to serialize message {}: {}", msg_id.as_str(), e);
                                    chunk_errors += 1;
                                    stats.errors += 1;
                                }
                            }
                        }
                        Err(e) => {
                            warn!("Failed to fetch message: {}", e);
                            chunk_errors += 1;
                            stats.errors += 1;
                        }
                    }
                }
                stats.timing.storage_ms += store_start.elapsed().as_millis() as u64;

                if chunk_stored > 0 {
                    debug!("[SYNC] Stored {} pending ({} errors), total pending: {}",
                        chunk_stored, chunk_errors, fetch_stats.pending);
                }
            }

            debug!(
                "[SYNC] fetch batch {} complete: {:?} ({} pending)",
                batch_num, batch_start.elapsed(), fetch_stats.pending
            );
        }

        // Check for next page
        match list_response.next_page_token {
            Some(token) => page_token = Some(token),
            None => break,
        }
    }

    Ok(fetch_stats)
}

/// Result from processing a batch of pending messages
#[derive(Debug, Default, Clone)]
pub struct ProcessBatchResult {
    /// Number of messages processed in this batch
    pub processed: usize,
    /// Number of messages remaining to process
    pub remaining: usize,
    /// Number of errors in this batch
    pub errors: usize,
    /// Whether there are more messages to process
    pub has_more: bool,
}

/// Process a single batch of pending messages (INBOX first)
///
/// Returns after processing up to `batch_size` messages, allowing the caller
/// to update the UI between batches. Call repeatedly until `has_more` is false.
pub fn process_pending_batch(
    store: &dyn MailStore,
    options: &SyncOptions,
    stats: &mut SyncStats,
    batch_size: usize,
) -> Result<ProcessBatchResult> {
    let mut result = ProcessBatchResult::default();

    // Get next batch of pending messages (INBOX prioritized automatically)
    let pending = store.get_pending_messages(None, batch_size)?;

    if pending.is_empty() {
        result.remaining = 0;
        result.has_more = false;
        return Ok(result);
    }

    let mut threads_seen: HashSet<ThreadId> = HashSet::new();

    for pending_msg in pending {
        // Deserialize the raw Gmail message
        let gmail_msg: GmailMessage = match serde_json::from_slice(&pending_msg.data) {
            Ok(msg) => msg,
            Err(e) => {
                warn!("Failed to deserialize pending message: {}", e);
                store.delete_pending_message(&pending_msg.id)?;
                result.errors += 1;
                stats.errors += 1;
                continue;
            }
        };

        // Normalize
        let message = match normalize_message(gmail_msg) {
            Ok(msg) => msg,
            Err(e) => {
                warn!("Failed to normalize message: {}", e);
                store.delete_pending_message(&pending_msg.id)?;
                result.errors += 1;
                stats.errors += 1;
                continue;
            }
        };

        let thread_id = message.thread_id.clone();
        let is_new_thread = !store.has_thread(&thread_id)?;

        // Compute thread first (including this new message)
        // Must upsert thread BEFORE message due to FK constraint
        let thread = compute_thread(&thread_id, &[message.clone()], store)?;
        store.upsert_thread(thread.clone())?;

        // Now store message (thread exists, FK constraint satisfied)
        store.upsert_message(message.clone())?;
        stats.messages_created += 1;
        result.processed += 1;

        // Index for search if index is provided
        if let Some(ref index) = options.search_index {
            if let Err(e) = index.index_message(&message, &thread) {
                warn!("Failed to index message {}: {}", message.id.as_str(), e);
            }
        }

        // Track thread stats (only count once)
        if threads_seen.insert(thread_id) {
            if is_new_thread {
                stats.threads_created += 1;
            } else {
                stats.threads_updated += 1;
            }
        }

        // Delete pending message to free storage space
        store.delete_pending_message(&pending_msg.id)?;
    }

    // Commit search index after batch
    if let Some(ref index) = options.search_index {
        if let Err(e) = index.commit() {
            warn!("Failed to commit search index: {}", e);
        }
    }

    result.remaining = store.count_pending_messages(None)?;
    result.has_more = result.remaining > 0;

    Ok(result)
}

/// Phase 2: Process pending messages (INBOX first)
///
/// Reads pending messages from storage, normalizes them, stores as processed,
/// computes threads, and indexes for search. INBOX messages are processed first
/// to optimize time-to-inbox.
fn process_phase(
    store: &dyn MailStore,
    options: &SyncOptions,
    stats: &mut SyncStats,
) -> Result<()> {
    let process_batch_size = 100; // Process in batches for progress updates
    let mut threads_seen: HashSet<ThreadId> = HashSet::new();

    // Track timing in microseconds for per-message operations
    let mut normalize_us: u64 = 0;
    let mut storage_us: u64 = 0;
    let mut compute_thread_us: u64 = 0;
    let mut search_index_us: u64 = 0;

    // Process INBOX messages first for fast time-to-inbox
    let inbox_count = store.count_pending_messages(Some("INBOX"))?;
    if inbox_count > 0 {
        info!("[SYNC] Processing {} INBOX messages first...", inbox_count);
    }

    loop {
        // Get next batch of pending messages (INBOX prioritized automatically)
        let pending = store.get_pending_messages(None, process_batch_size)?;

        if pending.is_empty() {
            break;
        }

        debug!("[SYNC] processing batch of {} pending messages", pending.len());

        for pending_msg in pending {
            // Deserialize the raw Gmail message
            let gmail_msg: GmailMessage = match serde_json::from_slice(&pending_msg.data) {
                Ok(msg) => msg,
                Err(e) => {
                    warn!("Failed to deserialize pending message: {}", e);
                    store.delete_pending_message(&pending_msg.id)?;
                    stats.errors += 1;
                    continue;
                }
            };

            // Normalize
            let normalize_start = Instant::now();
            let message = match normalize_message(gmail_msg) {
                Ok(msg) => msg,
                Err(e) => {
                    warn!("Failed to normalize message: {}", e);
                    store.delete_pending_message(&pending_msg.id)?;
                    stats.errors += 1;
                    continue;
                }
            };
            normalize_us += normalize_start.elapsed().as_micros() as u64;

            let thread_id = message.thread_id.clone();
            let is_new_thread = !store.has_thread(&thread_id)?;

            // Compute thread first (including this new message)
            // Must upsert thread BEFORE message due to FK constraint
            let compute_start = Instant::now();
            let thread = compute_thread(&thread_id, &[message.clone()], store)?;
            compute_thread_us += compute_start.elapsed().as_micros() as u64;

            let storage_start = Instant::now();
            store.upsert_thread(thread.clone())?;

            // Now store message (thread exists, FK constraint satisfied)
            store.upsert_message(message.clone())?;
            storage_us += storage_start.elapsed().as_micros() as u64;
            stats.messages_created += 1;

            // Index for search if index is provided
            if let Some(ref index) = options.search_index {
                let index_start = Instant::now();
                if let Err(e) = index.index_message(&message, &thread) {
                    warn!("Failed to index message {}: {}", message.id.as_str(), e);
                }
                search_index_us += index_start.elapsed().as_micros() as u64;
            }

            // Track thread stats (only count once)
            if threads_seen.insert(thread_id) {
                if is_new_thread {
                    stats.threads_created += 1;
                } else {
                    stats.threads_updated += 1;
                }
            }

            // Delete pending message to free storage space
            store.delete_pending_message(&pending_msg.id)?;
        }

        // Commit search index after each batch
        if let Some(ref index) = options.search_index {
            let commit_start = Instant::now();
            if let Err(e) = index.commit() {
                warn!("Failed to commit search index: {}", e);
            }
            search_index_us += commit_start.elapsed().as_millis() as u64 * 1000; // Convert to us
        }

        info!(
            "[SYNC] Processed: {} messages, {} threads ({} remaining)",
            stats.messages_created,
            stats.threads_created + stats.threads_updated,
            store.count_pending_messages(None)?
        );
    }

    // Convert microseconds to milliseconds
    stats.timing.normalize_ms += normalize_us / 1000;
    stats.timing.storage_ms += storage_us / 1000;
    stats.timing.compute_thread_ms += compute_thread_us / 1000;
    stats.timing.search_index_ms += search_index_us / 1000;

    Ok(())
}

/// Get the current history ID from Gmail
fn get_current_history_id(gmail: &GmailClient) -> Result<String> {
    let profile = gmail.get_profile()?;
    Ok(profile.history_id)
}

/// Perform incremental sync using History API
fn incremental_sync(
    gmail: &GmailClient,
    store: &dyn MailStore,
    _account_id: &str,
    state: &SyncState,
    options: &SyncOptions,
) -> Result<SyncStats> {
    let sync_start = Instant::now();
    let mut stats = SyncStats {
        was_incremental: true,
        ..Default::default()
    };

    // Fetch history since last sync
    let history_start = Instant::now();
    let history = gmail
        .list_history_all(&state.history_id)
        .context("Failed to fetch history")?;
    stats.timing.history_ms = history_start.elapsed().as_millis() as u64;
    debug!("[SYNC] list_history_all: {:?}", history_start.elapsed());

    // Collect message IDs to fetch (new messages)
    let mut message_ids_to_fetch: Vec<MessageId> = Vec::new();
    // Track threads that need updating due to label changes
    let mut threads_to_update: HashSet<ThreadId> = HashSet::new();

    // Process history records
    // Track storage time in microseconds for per-message operations
    let mut storage_us: u64 = 0;

    if let Some(records) = &history.history {
        for record in records {
            // Handle new messages
            if let Some(added) = &record.messages_added {
                for msg_added in added {
                    let msg_id = MessageId::new(&msg_added.message.id);
                    // Only fetch if we don't already have it
                    if !store.has_message(&msg_id)? {
                        message_ids_to_fetch.push(msg_id);
                    }
                }
            }

            // Handle deleted messages
            if let Some(deleted) = &record.messages_deleted {
                for msg_deleted in deleted {
                    let msg_id = MessageId::new(&msg_deleted.message.id);
                    // Get thread ID before deletion for potential thread update
                    if let Some(msg) = store.get_message(&msg_id)? {
                        threads_to_update.insert(msg.thread_id.clone());
                    }
                    store.delete_message(&msg_id)?;
                    stats.messages_updated += 1; // Count deletions as updates
                }
            }

            // Handle labels added to messages
            if let Some(labels_added) = &record.labels_added {
                for change in labels_added {
                    let msg_id = MessageId::new(&change.message.id);
                    if let Some(mut msg) = store.get_message(&msg_id)? {
                        // Add labels that aren't already present
                        for label in &change.label_ids {
                            if !msg.label_ids.contains(label) {
                                msg.label_ids.push(label.clone());
                            }
                        }
                        store.update_message_labels(&msg_id, msg.label_ids)?;
                        stats.labels_updated += 1;
                        threads_to_update.insert(msg.thread_id);
                    }
                }
            }

            // Handle labels removed from messages
            if let Some(labels_removed) = &record.labels_removed {
                for change in labels_removed {
                    let msg_id = MessageId::new(&change.message.id);
                    if let Some(mut msg) = store.get_message(&msg_id)? {
                        // Remove the specified labels
                        msg.label_ids.retain(|l| !change.label_ids.contains(l));
                        store.update_message_labels(&msg_id, msg.label_ids)?;
                        stats.labels_updated += 1;
                        threads_to_update.insert(msg.thread_id);
                    }
                }
            }
        }
    }

    stats.messages_fetched = message_ids_to_fetch.len();

    // Track which threads we've seen for stats
    let mut threads_seen: HashSet<ThreadId> = HashSet::new();

    // Fetch and store new messages
    if !message_ids_to_fetch.is_empty() {
        debug!("[SYNC] fetching {} new messages from history", message_ids_to_fetch.len());

        let fetch_start = Instant::now();
        let results = gmail.get_messages_batch(&message_ids_to_fetch);
        stats.timing.fetch_messages_ms += fetch_start.elapsed().as_millis() as u64;
        debug!("[SYNC] fetch_messages ({} msgs): {:?}", message_ids_to_fetch.len(), fetch_start.elapsed());

        for result in results {
            match result {
                Ok(gmail_msg) => {
                    let normalize_start = Instant::now();
                    let normalize_result = normalize_message(gmail_msg);
                    stats.timing.normalize_ms += normalize_start.elapsed().as_micros() as u64;

                    match normalize_result {
                        Ok(message) => {
                            let thread_id = message.thread_id.clone();
                            let is_new_thread = !store.has_thread(&thread_id)?;

                            // Compute thread first (including this new message)
                            // Must upsert thread BEFORE message due to FK constraint
                            let compute_start = Instant::now();
                            let thread = compute_thread(&thread_id, &[message.clone()], store)?;
                            stats.timing.compute_thread_ms += compute_start.elapsed().as_micros() as u64;

                            let storage_start = Instant::now();
                            store.upsert_thread(thread.clone())?;

                            // Now store message (thread exists, FK constraint satisfied)
                            store.upsert_message(message.clone())?;
                            storage_us += storage_start.elapsed().as_micros() as u64;
                            stats.messages_created += 1;

                            // Index for search if index is provided
                            if let Some(ref index) = options.search_index {
                                let index_start = Instant::now();
                                if let Err(e) = index.index_message(&message, &thread) {
                                    warn!("Failed to index message {}: {}", message.id.as_str(), e);
                                }
                                stats.timing.search_index_ms += index_start.elapsed().as_micros() as u64;
                            }

                            // Track thread stats (only count once)
                            if threads_seen.insert(thread_id.clone()) {
                                if is_new_thread {
                                    stats.threads_created += 1;
                                } else {
                                    stats.threads_updated += 1;
                                }
                            }

                            // Remove from threads_to_update since we just updated it
                            threads_to_update.remove(&thread_id);
                        }
                        Err(e) => {
                            warn!("Failed to normalize message: {}", e);
                            stats.errors += 1;
                        }
                    }
                }
                Err(e) => {
                    warn!("Failed to fetch message: {}", e);
                    stats.errors += 1;
                }
            }
        }
    }

    // Update threads affected by label changes (that weren't already updated)
    for thread_id in threads_to_update {
        if store.has_thread(&thread_id)? {
            let compute_start = Instant::now();
            let thread = compute_thread(&thread_id, &[], store)?;
            stats.timing.compute_thread_ms += compute_start.elapsed().as_micros() as u64;

            let storage_start = Instant::now();
            store.upsert_thread(thread)?;
            storage_us += storage_start.elapsed().as_micros() as u64;

            if threads_seen.insert(thread_id) {
                stats.threads_updated += 1;
            }
        }
    }

    // Commit search index
    if let Some(ref index) = options.search_index {
        let commit_start = Instant::now();
        if let Err(e) = index.commit() {
            warn!("Failed to commit search index: {}", e);
        }
        stats.timing.search_index_ms += commit_start.elapsed().as_millis() as u64;
    }

    // Update sync state with new history ID
    if let Some(new_history_id) = history.history_id {
        let updated_state = state.clone().updated(new_history_id);
        store.save_sync_state(updated_state)?;
    }

    // Convert microseconds to milliseconds for sub-ms operations
    stats.timing.storage_ms = storage_us / 1000;
    stats.timing.normalize_ms /= 1000;
    stats.timing.compute_thread_ms /= 1000;
    stats.timing.search_index_ms /= 1000;

    // Record total incremental sync time
    stats.timing.incremental_sync_ms = sync_start.elapsed().as_millis() as u64;

    debug!("[SYNC] incremental_sync complete: {:?}", sync_start.elapsed());
    debug!(
        "[SYNC] timing breakdown - history: {}ms, fetch: {}ms, normalize: {}ms, storage: {}ms, compute_thread: {}ms, search_index: {}ms, total: {}ms",
        stats.timing.history_ms,
        stats.timing.fetch_messages_ms,
        stats.timing.normalize_ms,
        stats.timing.storage_ms,
        stats.timing.compute_thread_ms,
        stats.timing.search_index_ms,
        stats.timing.incremental_sync_ms
    );
    info!(
        "Incremental sync: {} messages, {} label updates in {}ms",
        stats.messages_created, stats.labels_updated, stats.timing.incremental_sync_ms
    );

    Ok(stats)
}

/// Legacy sync function for backward compatibility with Phase 1
///
/// This function syncs the entire mailbox (not just inbox).
/// For incremental sync support, use `sync_gmail` instead.
pub fn sync_inbox(
    gmail: &GmailClient,
    store: &dyn MailStore,
    max_messages: usize,
) -> Result<SyncStats> {
    // Use a default account ID for legacy API
    let options = SyncOptions {
        max_messages: Some(max_messages),
        full_resync: false,
        label_filter: None,
        search_index: None,
    };

    sync_gmail(gmail, store, "default", options)
}

/// Compute thread properties from its messages
fn compute_thread(
    thread_id: &ThreadId,
    new_messages: &[Message],
    store: &dyn MailStore,
) -> Result<Thread> {
    // Get existing messages for this thread (as metadata)
    let existing_messages = store.list_messages_for_thread(thread_id)?;

    // Convert new messages to metadata for uniform handling
    let new_metadata: Vec<MessageMetadata> = new_messages
        .iter()
        .map(|m| MessageMetadata {
            id: m.id.clone(),
            thread_id: m.thread_id.clone(),
            from: m.from.clone(),
            to: m.to.clone(),
            cc: m.cc.clone(),
            subject: m.subject.clone(),
            body_preview: m.body_preview.clone(),
            received_at: m.received_at,
            internal_date: m.internal_date,
            label_ids: m.label_ids.clone(),
            has_body_text: m.body_text.is_some(),
            has_body_html: m.body_html.is_some(),
        })
        .collect();

    // Combine existing and new messages
    let all_messages: Vec<&MessageMetadata> = existing_messages
        .iter()
        .chain(new_metadata.iter())
        .collect();

    // Find the latest message for subject and snippet
    let latest = all_messages
        .iter()
        .max_by_key(|m| m.received_at)
        .expect("Thread must have at least one message");

    // Find the overall latest timestamp
    let last_message_at = all_messages
        .iter()
        .map(|m| m.received_at)
        .max()
        .unwrap_or_else(Utc::now);

    // Use subject from first message (thread subject)
    let first = all_messages
        .iter()
        .min_by_key(|m| m.received_at)
        .expect("Thread must have at least one message");

    let subject = if first.subject.is_empty() {
        "(no subject)".to_string()
    } else {
        first.subject.clone()
    };

    // Extract sender from first message
    let sender_name = first.from.name.clone();
    let sender_email = first.from.email.clone();

    // Check if any message is unread
    let is_unread = all_messages
        .iter()
        .any(|m| m.label_ids.iter().any(|l| l == LabelId::UNREAD));

    Ok(Thread::new(
        thread_id.clone(),
        subject,
        latest.body_preview.clone(),
        last_message_at,
        all_messages.len(),
        sender_name,
        sender_email,
        is_unread,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::EmailAddress;
    use crate::storage::InMemoryMailStore;

    fn make_test_message(id: &str, thread_id: &str, subject: &str, age_hours: i64) -> Message {
        let received_at = Utc::now() - chrono::Duration::hours(age_hours);
        Message::builder(MessageId::new(id), ThreadId::new(thread_id))
            .from(EmailAddress::new("test@example.com"))
            .subject(subject)
            .body_preview(format!("Body for {}", id))
            .received_at(received_at)
            .build()
    }

    #[test]
    fn test_compute_thread() {
        let store = InMemoryMailStore::new();
        let thread_id = ThreadId::new("t1");

        let messages = vec![
            make_test_message("m1", "t1", "Original Subject", 3),
            make_test_message("m2", "t1", "Re: Original Subject", 2),
            make_test_message("m3", "t1", "Re: Original Subject", 1),
        ];

        let thread = compute_thread(&thread_id, &messages, &store).unwrap();

        assert_eq!(thread.subject, "Original Subject");
        assert_eq!(thread.message_count, 3);
        assert_eq!(thread.snippet, "Body for m3"); // Latest message
    }

    #[test]
    fn test_compute_thread_with_existing() {
        let store = InMemoryMailStore::new();
        let thread_id = ThreadId::new("t1");

        // Store an existing message
        let existing = make_test_message("m1", "t1", "Original Subject", 3);
        store.upsert_message(existing).unwrap();

        // New messages to add
        let new_messages = vec![
            make_test_message("m2", "t1", "Re: Original Subject", 2),
            make_test_message("m3", "t1", "Re: Original Subject", 1),
        ];

        let thread = compute_thread(&thread_id, &new_messages, &store).unwrap();

        assert_eq!(thread.message_count, 3);
        assert_eq!(thread.subject, "Original Subject");
    }

    #[test]
    fn test_sync_options_default() {
        let options = SyncOptions::default();
        assert!(options.max_messages.is_none());
        assert!(!options.full_resync);
    }

    #[test]
    fn test_sync_stats_messages_stored() {
        let stats = SyncStats {
            messages_created: 5,
            messages_updated: 3,
            ..Default::default()
        };
        assert_eq!(stats.messages_stored(), 8);
    }
}
