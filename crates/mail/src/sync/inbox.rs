//! Gmail sync implementation
//!
//! Provides both initial full sync and incremental sync via Gmail History API.
//! Syncs the user's entire email library (all labels/folders).

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use log::{info, warn};
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;

use crate::gmail::{api::GmailMessage, normalize_message, GmailClient, HistoryExpiredError};
use crate::models::{LabelId, Message, MessageId, SyncState, Thread, ThreadId};
use crate::search::SearchIndex;
use crate::storage::{MailStore, MessageMetadata};

/// The action that should be taken when syncing
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncAction {
    /// Start a fresh initial sync (no existing state or force resync)
    InitialSync,
    /// Resume an incomplete initial sync from saved checkpoint
    ResumeInitialSync {
        /// Page token to resume listing from (None = start from beginning)
        page_token: Option<String>,
        /// Number of messages already listed
        messages_listed: usize,
        /// Message IDs that failed previously and need retry
        failed_message_ids: Vec<String>,
    },
    /// Perform incremental sync using History API
    IncrementalSync {
        /// History ID to sync from
        history_id: String,
    },
    /// Full resync needed due to stale history ID (> 5 days old)
    StaleResync {
        /// How many days since last sync
        days_since_sync: i64,
    },
}

/// Determines what sync action should be taken based on current state
///
/// This is a pure function that examines the sync state and returns
/// the appropriate action. It does not perform any I/O.
///
/// # Arguments
/// * `sync_state` - Current sync state from storage (None if never synced)
/// * `force_resync` - Whether to force a full resync regardless of state
///
/// # Returns
/// The sync action that should be taken
pub fn determine_sync_action(sync_state: Option<&SyncState>, force_resync: bool) -> SyncAction {
    // Force resync requested
    if force_resync {
        return SyncAction::InitialSync;
    }

    match sync_state {
        // No sync state - start fresh
        None => SyncAction::InitialSync,

        // Incomplete initial sync - resume it
        Some(state) if !state.initial_sync_complete => SyncAction::ResumeInitialSync {
            page_token: state.fetch_page_token.clone(),
            messages_listed: state.messages_listed,
            failed_message_ids: state.failed_message_ids.clone(),
        },

        // Complete sync state - check staleness
        Some(state) => {
            let age = Utc::now() - state.last_sync_at;
            let days = age.num_days();

            if days >= 5 {
                // History ID likely expired or about to expire
                SyncAction::StaleResync {
                    days_since_sync: days,
                }
            } else {
                // Recent enough for incremental sync
                SyncAction::IncrementalSync {
                    history_id: state.history_id.clone(),
                }
            }
        }
    }
}

/// Checks if an app should auto-start sync on startup
///
/// Returns true if:
/// - There's an incomplete initial sync that needs to be resumed
/// - OR there's no sync state at all (first time)
///
/// # Arguments
/// * `sync_state` - Current sync state from storage
pub fn should_auto_sync_on_startup(sync_state: Option<&SyncState>) -> bool {
    match sync_state {
        None => true, // Never synced
        Some(state) => !state.initial_sync_complete, // Incomplete sync
    }
}

/// Returns details about the sync state for logging/display
#[derive(Debug, Clone)]
pub struct SyncStateInfo {
    /// Whether a sync has ever completed
    pub has_completed_sync: bool,
    /// Whether initial sync is incomplete and needs resume
    pub needs_resume: bool,
    /// Last sync timestamp (if any)
    pub last_sync_at: Option<DateTime<Utc>>,
    /// Resume progress (if resuming)
    pub resume_progress: Option<ResumeProgress>,
}

/// Progress information for resuming a sync
#[derive(Debug, Clone)]
pub struct ResumeProgress {
    /// Whether we have a page token to resume from
    pub has_page_token: bool,
    /// Number of messages already listed
    pub messages_listed: usize,
    /// Number of failed message IDs to retry
    pub failed_message_count: usize,
}

/// Get information about current sync state
pub fn get_sync_state_info(sync_state: Option<&SyncState>) -> SyncStateInfo {
    match sync_state {
        None => SyncStateInfo {
            has_completed_sync: false,
            needs_resume: false,
            last_sync_at: None,
            resume_progress: None,
        },
        Some(state) => {
            let needs_resume = !state.initial_sync_complete;
            SyncStateInfo {
                has_completed_sync: state.initial_sync_complete,
                needs_resume,
                last_sync_at: Some(state.last_sync_at),
                resume_progress: if needs_resume {
                    Some(ResumeProgress {
                        has_page_token: state.fetch_page_token.is_some(),
                        messages_listed: state.messages_listed,
                        failed_message_count: state.failed_message_ids.len(),
                    })
                } else {
                    None
                },
            }
        }
    }
}

/// Options for sync operation
#[derive(Debug, Clone, Default)]
pub struct SyncOptions {
    /// Maximum messages to fetch in initial sync
    pub max_messages: Option<usize>,
    /// Force full resync even if history_id exists
    pub full_resync: bool,
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
/// ## Resilience Features
///
/// - **Automatic resume**: Incomplete initial syncs are resumed from the last checkpoint.
/// - **Page token checkpointing**: Message listing can resume from where it left off.
/// - **Failed ID retry**: Messages that failed to fetch are retried on next sync.
/// - **Stale state detection**: History IDs older than 5 days trigger a proactive full resync.
/// - **History expired handling**: 404/400 from Gmail History API triggers full resync.
/// - **Catch-up sync retry**: After initial sync, catch-up is retried up to 3 times.
///
/// # Arguments
/// * `gmail` - Gmail API client
/// * `store` - Storage backend
/// * `account_id` - Account ID (FK to accounts table)
/// * `options` - Sync options
pub fn sync_gmail(
    gmail: &GmailClient,
    store: &dyn MailStore,
    account_id: i64,
    options: SyncOptions,
) -> Result<SyncStats> {
    sync_gmail_with_progress(gmail, store, account_id, options, |_, _| {})
}

/// Sync Gmail inbox with progress callback
///
/// Same as `sync_gmail` but with a progress callback for UI updates.
/// The callback receives (messages_fetched, phase_description).
pub fn sync_gmail_with_progress<F>(
    gmail: &GmailClient,
    store: &dyn MailStore,
    account_id: i64,
    options: SyncOptions,
    on_progress: F,
) -> Result<SyncStats>
where
    F: Fn(usize, &str),
{
    let start = std::time::Instant::now();

    // Check for existing sync state
    let existing_state = store.get_sync_state(account_id)?;

    on_progress(0, "Checking sync state...");

    let mut stats = match existing_state {
        // Full resync requested - start fresh
        _ if options.full_resync => {
            on_progress(0, "Starting full resync...");
            info!("Full resync requested, clearing existing data...");
            store.clear_mail_data()?;
            store.delete_sync_state(account_id)?;
            initial_sync_with_progress(gmail, store, account_id, &options, &on_progress)?
        }
        // Incomplete initial sync - resume it
        Some(state) if !state.initial_sync_complete => {
            on_progress(state.messages_listed, &format!("Resuming sync ({} listed)...", state.messages_listed));
            info!(
                "Resuming incomplete initial sync (page_token={}, messages_listed={}, failed_ids={})",
                state.fetch_page_token.is_some(),
                state.messages_listed,
                state.failed_message_ids.len()
            );
            initial_sync_with_progress(gmail, store, account_id, &options, &on_progress)?
        }
        // Complete sync state - check for staleness first
        Some(state) => {
            // Proactively detect stale history IDs (older than 5 days)
            // Gmail history IDs expire after ~7 days, so we trigger a resync early
            // to avoid losing messages during the gap
            let age = chrono::Utc::now() - state.last_sync_at;
            if age.num_days() >= 5 {
                on_progress(0, "Sync state stale, resyncing...");
                warn!(
                    "Sync state is {} days old (history_id may expire), performing full resync",
                    age.num_days()
                );
                store.clear_mail_data()?;
                store.delete_sync_state(account_id)?;
                initial_sync_with_progress(gmail, store, account_id, &options, &on_progress)?
            } else {
                on_progress(0, "Checking for new messages...");
                // Try incremental sync
                match incremental_sync(gmail, store, &state, &options) {
                    Ok(stats) => stats,
                    Err(e) if e.downcast_ref::<HistoryExpiredError>().is_some() => {
                        // History ID expired, fall back to full resync
                        on_progress(0, "History expired, resyncing...");
                        warn!("History ID expired (404/400 from Gmail), performing full resync");
                        store.clear_mail_data()?;
                        store.delete_sync_state(account_id)?;
                        initial_sync_with_progress(gmail, store, account_id, &options, &on_progress)?
                    }
                    Err(e) => return Err(e),
                }
            }
        }
        // No sync state - start initial sync
        None => {
            on_progress(0, "Starting initial sync...");
            info!("No existing sync state, starting initial sync...");
            initial_sync_with_progress(gmail, store, account_id, &options, &on_progress)?
        }
    };

    stats.duration_ms = start.elapsed().as_millis() as u64;
    Ok(stats)
}

/// Perform initial full sync using decoupled fetch/process phases (no progress callback)
fn initial_sync(
    gmail: &GmailClient,
    store: &dyn MailStore,
    account_id: i64,
    options: &SyncOptions,
) -> Result<SyncStats> {
    initial_sync_with_progress(gmail, store, account_id, options, &|_, _| {})
}

/// Perform initial full sync using decoupled fetch/process phases
///
/// Phase 1 (Fetch): Downloads messages at max Gmail API speed, stores as pending
/// Phase 2 (Process): Processes pending messages, INBOX first, then the rest
///
/// After completing, runs an incremental catch-up sync for messages that arrived during sync.
fn initial_sync_with_progress<F>(
    gmail: &GmailClient,
    store: &dyn MailStore,
    account_id: i64,
    options: &SyncOptions,
    on_progress: &F,
) -> Result<SyncStats>
where
    F: Fn(usize, &str),
{
    log::debug!("initial_sync_with_progress called for account_id={}", account_id);
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
            log::debug!("Resuming initial sync from history_id: {}", state.history_id);
            info!("Resuming initial sync from history_id: {}", state.history_id);
            state.history_id
        }
        _ => {
            // Get history_id at START of sync so we can catch up on any
            // messages that arrive during the sync
            log::debug!("Getting current history ID from Gmail profile...");
            let profile_start = Instant::now();
            let history_id = get_current_history_id(gmail)?;
            stats.timing.profile_ms += profile_start.elapsed().as_millis() as u64;
            log::debug!("Got history_id: {} in {}ms", history_id, profile_start.elapsed().as_millis());
            info!("Starting initial sync from history_id: {}", history_id);

            // Save partial sync state to indicate initial sync is in progress
            let partial_state = SyncState::partial(account_id, &history_id);
            store.save_sync_state(partial_state)?;
            history_id
        }
    };

    // === PHASE 1: FETCH ===
    // Download messages as fast as Gmail allows, store as pending
    log::debug!("Phase 1: Fetching messages from Gmail...");
    info!("[SYNC] Phase 1: Fetching messages from Gmail...");
    on_progress(0, "Fetching messages...");
    let fetch_stats = fetch_phase_with_progress(gmail, store, account_id, options, &mut stats, on_progress)?;
    log::debug!("Phase 1 complete: {} fetched, {} pending", fetch_stats.fetched, fetch_stats.pending);
    info!("[SYNC] Fetch phase complete: {} fetched, {} pending, {} skipped, {} failed",
        fetch_stats.fetched, fetch_stats.pending, fetch_stats.skipped, fetch_stats.failed_ids.len());

    // === PHASE 2: PROCESS ===
    // Process pending messages: INBOX first, then the rest
    let pending_count = store.count_pending_messages(account_id, None)?;
    info!("[SYNC] Phase 2: Processing {} pending messages (INBOX first)...", pending_count);
    on_progress(stats.messages_fetched, &format!("Processing {} messages...", pending_count));

    if pending_count == 0 {
        info!("[SYNC] No pending messages to process, skipping process phase");
    } else {
        process_phase_with_progress(store, account_id, options, &mut stats, on_progress)?;
        info!("[SYNC] Process phase complete: {} messages created, {} threads",
            stats.messages_created, stats.threads_created + stats.threads_updated);
    }

    // Mark initial sync as complete with the history_id we captured at the start
    // IMPORTANT: Load the existing state to preserve failed_message_ids from fetch_phase
    let existing_state = store.get_sync_state(account_id)?;
    let complete_state = match existing_state {
        Some(state) => {
            // Preserve failed_message_ids for retry on next sync
            let failed_ids = state.failed_message_ids.clone();
            let mut complete = SyncState::partial(account_id, &start_history_id).mark_complete();
            complete.failed_message_ids = failed_ids;
            complete
        }
        None => SyncState::partial(account_id, &start_history_id).mark_complete(),
    };
    store.save_sync_state(complete_state.clone())?;

    // Record total initial sync time
    stats.timing.initial_sync_ms = sync_start.elapsed().as_millis() as u64;

    info!(
        "Initial sync complete: {} messages fetched, {} created, {} skipped in {}ms",
        stats.messages_fetched,
        stats.messages_created,
        stats.messages_skipped,
        stats.timing.initial_sync_ms
    );

    // Run incremental catch-up sync to get any messages that arrived during initial sync
    // Retry up to 3 times to ensure we don't miss messages
    info!("Running catch-up sync for messages received during initial sync...");
    let max_catchup_retries = 3;
    let mut catchup_attempt = 0;
    let mut catchup_success = false;

    while catchup_attempt < max_catchup_retries && !catchup_success {
        catchup_attempt += 1;

        match incremental_sync(gmail, store, &complete_state, options) {
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
                catchup_success = true;
            }
            Err(e) => {
                if catchup_attempt < max_catchup_retries {
                    warn!(
                        "Catch-up sync failed (attempt {}/{}), retrying: {}",
                        catchup_attempt, max_catchup_retries, e
                    );
                    // Brief delay before retry
                    std::thread::sleep(std::time::Duration::from_millis(500));
                } else {
                    // Final attempt failed - log but don't fail the whole sync
                    // The failed_message_ids mechanism will help recover any missed messages
                    // on the next sync
                    warn!(
                        "Catch-up sync failed after {} attempts (non-fatal): {}",
                        max_catchup_retries, e
                    );
                }
            }
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
    /// Message IDs that failed to fetch (will be retried next sync)
    pub failed_ids: Vec<String>,
}

/// Phase 1: Fetch messages from Gmail as fast as possible (no progress callback)
pub fn fetch_phase(
    gmail: &GmailClient,
    store: &dyn MailStore,
    account_id: i64,
    options: &SyncOptions,
    stats: &mut SyncStats,
) -> Result<FetchPhaseStats> {
    fetch_phase_with_progress(gmail, store, account_id, options, stats, &|_, _| {})
}

/// Phase 1: Fetch messages from Gmail as fast as possible
///
/// Lists all message IDs, fetches full content in parallel, stores raw JSON as pending.
/// This phase is optimized for maximum Gmail API throughput.
///
/// ## Resilience Features
///
/// - **Page token checkpointing**: Progress is saved after each page of message listing.
///   If sync is interrupted, it will resume from the last saved page token.
/// - **Failed ID tracking**: Messages that fail to fetch (non-retriable errors) are
///   recorded and will be retried on the next sync attempt.
///
/// Call this from a background thread, then call `process_pending_batch` repeatedly
/// to process messages with UI updates between batches.
fn fetch_phase_with_progress<F>(
    gmail: &GmailClient,
    store: &dyn MailStore,
    account_id: i64,
    options: &SyncOptions,
    stats: &mut SyncStats,
    on_progress: &F,
) -> Result<FetchPhaseStats>
where
    F: Fn(usize, &str),
{
    log::debug!("fetch_phase_with_progress called");
    let mut fetch_stats = FetchPhaseStats {
        fetched: 0,
        pending: 0,
        skipped: 0,
        failed_ids: Vec::new(),
    };

    // Load existing sync state to get resume position and failed IDs
    let existing_state = store.get_sync_state(account_id)?;
    let (mut page_token, mut total_listed, previous_failed_ids) = match &existing_state {
        Some(state) if !state.initial_sync_complete => {
            log::debug!(
                "Resuming fetch from page_token={:?}, messages_listed={}, failed_ids={}",
                state.fetch_page_token.is_some(),
                state.messages_listed,
                state.failed_message_ids.len()
            );
            info!(
                "Resuming fetch from page_token={:?}, messages_listed={}, failed_ids={}",
                state.fetch_page_token.as_deref().map(|s| &s[..s.len().min(20)]),
                state.messages_listed,
                state.failed_message_ids.len()
            );
            (
                state.fetch_page_token.clone(),
                state.messages_listed,
                state.failed_message_ids.clone(),
            )
        }
        _ => (None, 0, Vec::new()),
    };

    let batch_size = 500; // Gmail API max is 500 per page

    // First, retry any previously failed message IDs
    if !previous_failed_ids.is_empty() {
        info!("Retrying {} previously failed message IDs", previous_failed_ids.len());
        let failed_ids_to_retry: Vec<MessageId> = previous_failed_ids
            .iter()
            .map(|id| MessageId::new(id))
            .collect();

        let retry_failed = fetch_message_batch(
            gmail,
            store,
            account_id,
            &failed_ids_to_retry,
            stats,
        );
        fetch_stats.fetched += retry_failed.fetched;
        fetch_stats.pending += retry_failed.pending;
        // Any still-failing IDs will be tracked
        fetch_stats.failed_ids.extend(retry_failed.failed_ids);
    }

    loop {
        // Check if we've hit the limit
        if let Some(max) = options.max_messages {
            if total_listed >= max {
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
        log::debug!("Listing messages (page_token={:?})...", page_token.is_some());
        let list_start = Instant::now();
        let list_response = gmail.list_messages(
            effective_batch_size,
            page_token.as_deref(),
            None,
        )?;
        stats.timing.list_messages_ms += list_start.elapsed().as_millis() as u64;

        let message_refs = list_response.messages.unwrap_or_default();
        log::debug!("Listed {} messages in {}ms", message_refs.len(), list_start.elapsed().as_millis());

        if message_refs.is_empty() {
            log::debug!("No more messages to list");
            break;
        }

        total_listed += message_refs.len();
        log::debug!("Total listed so far: {}", total_listed);
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
        stats.messages_fetched += message_refs.len();

        if !to_fetch.is_empty() {
            let batch_result = fetch_message_batch(gmail, store, account_id, &to_fetch, stats);
            fetch_stats.fetched += batch_result.fetched;
            fetch_stats.pending += batch_result.pending;
            fetch_stats.failed_ids.extend(batch_result.failed_ids);
        }

        // Report progress after each page
        on_progress(
            fetch_stats.fetched,
            &format!("Fetched {} messages ({} listed)...", fetch_stats.fetched, total_listed)
        );

        // Determine next page token
        let next_page_token = list_response.next_page_token;

        // Checkpoint progress after each page (before moving to next)
        // This ensures we can resume from this point if interrupted
        if let Some(ref state) = existing_state {
            let updated_state = state.clone().with_fetch_progress(
                next_page_token.clone(),
                total_listed,
            ).with_failed_ids(fetch_stats.failed_ids.clone());
            store.save_sync_state(updated_state)?;
        }

        // Check for next page
        match next_page_token {
            Some(token) => page_token = Some(token),
            None => break,
        }
    }

    // Clear page token in final state (listing complete)
    if let Some(ref state) = existing_state {
        let final_state = state.clone().with_fetch_progress(None, total_listed)
            .with_failed_ids(fetch_stats.failed_ids.clone());
        store.save_sync_state(final_state)?;
    }

    if !fetch_stats.failed_ids.is_empty() {
        warn!(
            "Fetch phase complete with {} failed message IDs (will retry next sync)",
            fetch_stats.failed_ids.len()
        );
    }

    Ok(fetch_stats)
}

/// Helper struct for batch fetch results
struct BatchFetchResult {
    fetched: usize,
    pending: usize,
    failed_ids: Vec<String>,
}

/// Fetch a batch of messages and store them as pending
fn fetch_message_batch(
    gmail: &GmailClient,
    store: &dyn MailStore,
    account_id: i64,
    to_fetch: &[MessageId],
    stats: &mut SyncStats,
) -> BatchFetchResult {
    let mut result = BatchFetchResult {
        fetched: 0,
        pending: 0,
        failed_ids: Vec::new(),
    };

    // Gmail batch API has aggressive rate limiting independent of quota
    // 25 messages per batch with no delay works reliably
    let chunk_size = 25;
    for chunk in to_fetch.chunks(chunk_size) {
        let fetch_start = Instant::now();
        let results = gmail.get_messages_batch(chunk);
        stats.timing.fetch_messages_ms += fetch_start.elapsed().as_millis() as u64;

        // Store immediately after each chunk
        let store_start = Instant::now();
        for (msg_id, fetch_result) in chunk.iter().zip(results) {
            match fetch_result {
                Ok(gmail_msg) => {
                    let label_ids = gmail_msg.label_ids.clone().unwrap_or_default();

                    match serde_json::to_vec(&gmail_msg) {
                        Ok(data) => {
                            if let Err(e) = store.store_pending_message(msg_id, account_id, &data, label_ids) {
                                warn!("Failed to store pending message {}: {}", msg_id.as_str(), e);
                                stats.errors += 1;
                                result.failed_ids.push(msg_id.as_str().to_string());
                            } else {
                                result.fetched += 1;
                                result.pending += 1;
                            }
                        }
                        Err(e) => {
                            warn!("Failed to serialize message {}: {}", msg_id.as_str(), e);
                            stats.errors += 1;
                            // Don't add to failed_ids - serialization errors won't recover
                        }
                    }
                }
                Err(e) => {
                    let error_msg = e.to_string();
                    // Only track as failed if it's potentially recoverable
                    // 404 might be a permanently deleted message, but we'll retry once
                    // to be sure (could be a transient issue)
                    if error_msg.contains("404") {
                        warn!("Message {} not found (404), will retry once: {}", msg_id.as_str(), e);
                        result.failed_ids.push(msg_id.as_str().to_string());
                    } else {
                        warn!("Failed to fetch message {}: {}", msg_id.as_str(), e);
                        result.failed_ids.push(msg_id.as_str().to_string());
                    }
                    stats.errors += 1;
                }
            }
        }
        stats.timing.storage_ms += store_start.elapsed().as_millis() as u64;
    }

    result
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
    account_id: i64,
    options: &SyncOptions,
    stats: &mut SyncStats,
    batch_size: usize,
) -> Result<ProcessBatchResult> {
    let mut result = ProcessBatchResult::default();

    // Get next batch of pending messages (INBOX prioritized automatically)
    let pending = store.get_pending_messages(account_id, None, batch_size)?;

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
        let message = match normalize_message(gmail_msg, account_id) {
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
        let thread = compute_thread(&thread_id, account_id, &[message.clone()], store)?;
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

    result.remaining = store.count_pending_messages(account_id, None)?;
    result.has_more = result.remaining > 0;

    Ok(result)
}

/// Phase 2: Process pending messages (INBOX first) - no progress callback
fn process_phase(
    store: &dyn MailStore,
    account_id: i64,
    options: &SyncOptions,
    stats: &mut SyncStats,
) -> Result<()> {
    process_phase_with_progress(store, account_id, options, stats, &|_, _| {})
}

/// Phase 2: Process pending messages (INBOX first)
///
/// Reads pending messages from storage, normalizes them, stores as processed,
/// computes threads, and indexes for search. INBOX messages are processed first
/// to optimize time-to-inbox.
fn process_phase_with_progress<F>(
    store: &dyn MailStore,
    account_id: i64,
    options: &SyncOptions,
    stats: &mut SyncStats,
    on_progress: &F,
) -> Result<()>
where
    F: Fn(usize, &str),
{
    let process_batch_size = 100; // Process in batches for progress updates
    let mut threads_seen: HashSet<ThreadId> = HashSet::new();

    // Track timing in microseconds for per-message operations
    let mut normalize_us: u64 = 0;
    let mut storage_us: u64 = 0;
    let mut compute_thread_us: u64 = 0;
    let mut search_index_us: u64 = 0;

    loop {
        // Get next batch of pending messages (INBOX prioritized automatically)
        let pending = store.get_pending_messages(account_id, None, process_batch_size)?;

        if pending.is_empty() {
            break;
        }

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
            let message = match normalize_message(gmail_msg, account_id) {
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
            let thread = compute_thread(&thread_id, account_id, &[message.clone()], store)?;
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
            search_index_us += commit_start.elapsed().as_millis() as u64 * 1000;
        }

        // Report progress after each batch
        let remaining = store.count_pending_messages(account_id, None)?;
        on_progress(
            stats.messages_created,
            &format!("Processed {} messages ({} remaining)...", stats.messages_created, remaining)
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

/// Perform incremental sync using Gmail History API
///
/// Fetches changes since the last sync using the history_id from the sync state.
/// This is much faster than a full sync as it only fetches changed messages.
///
/// # Arguments
/// * `gmail` - Gmail client
/// * `store` - Mail store
/// * `state` - Current sync state (must have history_id)
/// * `options` - Sync options (for search indexing)
///
/// # Returns
/// Sync statistics or error (including HistoryExpiredError if history_id is too old)
pub fn incremental_sync(
    gmail: &GmailClient,
    store: &dyn MailStore,
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
        let fetch_start = Instant::now();
        let results = gmail.get_messages_batch(&message_ids_to_fetch);
        stats.timing.fetch_messages_ms += fetch_start.elapsed().as_millis() as u64;

        for result in results {
            match result {
                Ok(gmail_msg) => {
                    let normalize_start = Instant::now();
                    let normalize_result = normalize_message(gmail_msg, state.account_id);
                    stats.timing.normalize_ms += normalize_start.elapsed().as_micros() as u64;

                    match normalize_result {
                        Ok(message) => {
                            let thread_id = message.thread_id.clone();
                            let is_new_thread = !store.has_thread(&thread_id)?;

                            // Compute thread first (including this new message)
                            // Must upsert thread BEFORE message due to FK constraint
                            let compute_start = Instant::now();
                            let thread = compute_thread(&thread_id, state.account_id, &[message.clone()], store)?;
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
            let thread = compute_thread(&thread_id, state.account_id, &[], store)?;
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

    info!(
        "Incremental sync: {} messages, {} label updates in {}ms",
        stats.messages_created, stats.labels_updated, stats.timing.incremental_sync_ms
    );

    Ok(stats)
}

/// Compute thread properties from its messages
fn compute_thread(
    thread_id: &ThreadId,
    account_id: i64,
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
            account_id: m.account_id,
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
        account_id,
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

        let thread = compute_thread(&thread_id, 1, &messages, &store).unwrap();

        assert_eq!(thread.subject, "Original Subject");
        assert_eq!(thread.message_count, 3);
        assert_eq!(thread.snippet, "Body for m3"); // Latest message
        assert_eq!(thread.account_id, 1);
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

        let thread = compute_thread(&thread_id, 1, &new_messages, &store).unwrap();

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

    // === Sync Decision Tests ===

    #[test]
    fn test_determine_sync_action_no_state() {
        let action = determine_sync_action(None, false);
        assert_eq!(action, SyncAction::InitialSync);
    }

    #[test]
    fn test_determine_sync_action_force_resync() {
        // Even with existing complete state, force_resync should return InitialSync
        let state = SyncState::new(1, "12345");
        let action = determine_sync_action(Some(&state), true);
        assert_eq!(action, SyncAction::InitialSync);
    }

    #[test]
    fn test_determine_sync_action_incomplete_sync() {
        // Incomplete initial sync with checkpoint
        let mut state = SyncState::partial(1, "12345");
        state.fetch_page_token = Some("page_token_abc".to_string());
        state.messages_listed = 5000;
        state.failed_message_ids = vec!["msg1".to_string(), "msg2".to_string()];

        let action = determine_sync_action(Some(&state), false);

        match action {
            SyncAction::ResumeInitialSync {
                page_token,
                messages_listed,
                failed_message_ids,
            } => {
                assert_eq!(page_token, Some("page_token_abc".to_string()));
                assert_eq!(messages_listed, 5000);
                assert_eq!(failed_message_ids, vec!["msg1", "msg2"]);
            }
            _ => panic!("Expected ResumeInitialSync, got {:?}", action),
        }
    }

    #[test]
    fn test_determine_sync_action_incomplete_sync_no_checkpoint() {
        // Incomplete sync but no page token yet (just started)
        let state = SyncState::partial(1, "12345");

        let action = determine_sync_action(Some(&state), false);

        match action {
            SyncAction::ResumeInitialSync {
                page_token,
                messages_listed,
                failed_message_ids,
            } => {
                assert_eq!(page_token, None);
                assert_eq!(messages_listed, 0);
                assert!(failed_message_ids.is_empty());
            }
            _ => panic!("Expected ResumeInitialSync, got {:?}", action),
        }
    }

    #[test]
    fn test_determine_sync_action_recent_complete_sync() {
        // Recently completed sync should use incremental
        let state = SyncState::new(1, "12345");
        // state.last_sync_at is set to now() by default

        let action = determine_sync_action(Some(&state), false);

        match action {
            SyncAction::IncrementalSync { history_id } => {
                assert_eq!(history_id, "12345");
            }
            _ => panic!("Expected IncrementalSync, got {:?}", action),
        }
    }

    #[test]
    fn test_determine_sync_action_stale_sync() {
        // Sync from 6 days ago should trigger StaleResync
        let mut state = SyncState::new(1, "12345");
        state.last_sync_at = Utc::now() - chrono::Duration::days(6);

        let action = determine_sync_action(Some(&state), false);

        match action {
            SyncAction::StaleResync { days_since_sync } => {
                assert_eq!(days_since_sync, 6);
            }
            _ => panic!("Expected StaleResync, got {:?}", action),
        }
    }

    #[test]
    fn test_determine_sync_action_boundary_4_days() {
        // 4 days is still fresh enough for incremental
        let mut state = SyncState::new(1, "12345");
        state.last_sync_at = Utc::now() - chrono::Duration::days(4);

        let action = determine_sync_action(Some(&state), false);

        match action {
            SyncAction::IncrementalSync { .. } => {}
            _ => panic!("Expected IncrementalSync at 4 days, got {:?}", action),
        }
    }

    #[test]
    fn test_determine_sync_action_boundary_5_days() {
        // 5 days is the threshold for stale
        let mut state = SyncState::new(1, "12345");
        state.last_sync_at = Utc::now() - chrono::Duration::days(5);

        let action = determine_sync_action(Some(&state), false);

        match action {
            SyncAction::StaleResync { .. } => {}
            _ => panic!("Expected StaleResync at 5 days, got {:?}", action),
        }
    }

    // === Auto-Sync on Startup Tests ===

    #[test]
    fn test_should_auto_sync_no_state() {
        assert!(should_auto_sync_on_startup(None));
    }

    #[test]
    fn test_should_auto_sync_incomplete() {
        let state = SyncState::partial(1, "12345");
        assert!(should_auto_sync_on_startup(Some(&state)));
    }

    #[test]
    fn test_should_not_auto_sync_complete() {
        let state = SyncState::new(1, "12345");
        assert!(!should_auto_sync_on_startup(Some(&state)));
    }

    #[test]
    fn test_should_not_auto_sync_complete_stale() {
        // Even if stale, a completed sync shouldn't auto-start
        // (user can manually sync if they want)
        let mut state = SyncState::new(1, "12345");
        state.last_sync_at = Utc::now() - chrono::Duration::days(10);
        assert!(!should_auto_sync_on_startup(Some(&state)));
    }

    // === Sync State Info Tests ===

    #[test]
    fn test_sync_state_info_no_state() {
        let info = get_sync_state_info(None);
        assert!(!info.has_completed_sync);
        assert!(!info.needs_resume);
        assert!(info.last_sync_at.is_none());
        assert!(info.resume_progress.is_none());
    }

    #[test]
    fn test_sync_state_info_complete() {
        let state = SyncState::new(1, "12345");
        let info = get_sync_state_info(Some(&state));

        assert!(info.has_completed_sync);
        assert!(!info.needs_resume);
        assert!(info.last_sync_at.is_some());
        assert!(info.resume_progress.is_none());
    }

    #[test]
    fn test_sync_state_info_incomplete_with_progress() {
        let mut state = SyncState::partial(1, "12345");
        state.fetch_page_token = Some("token".to_string());
        state.messages_listed = 1000;
        state.failed_message_ids = vec!["m1".to_string()];

        let info = get_sync_state_info(Some(&state));

        assert!(!info.has_completed_sync);
        assert!(info.needs_resume);
        assert!(info.last_sync_at.is_some());

        let progress = info.resume_progress.unwrap();
        assert!(progress.has_page_token);
        assert_eq!(progress.messages_listed, 1000);
        assert_eq!(progress.failed_message_count, 1);
    }

    // === State Transitions Tests ===

    #[test]
    fn test_sync_state_lifecycle() {
        // Test the full lifecycle of sync state

        // 1. Start: no state
        let action1 = determine_sync_action(None, false);
        assert_eq!(action1, SyncAction::InitialSync);

        // 2. Initial sync started but not finished
        let partial = SyncState::partial(1, "history_100");
        let action2 = determine_sync_action(Some(&partial), false);
        assert!(matches!(action2, SyncAction::ResumeInitialSync { .. }));

        // 3. Initial sync completed
        let complete = partial.mark_complete();
        let action3 = determine_sync_action(Some(&complete), false);
        assert!(matches!(action3, SyncAction::IncrementalSync { .. }));

        // 4. After incremental sync updates history_id
        let updated = complete.updated("history_200");
        let action4 = determine_sync_action(Some(&updated), false);
        match action4 {
            SyncAction::IncrementalSync { history_id } => {
                assert_eq!(history_id, "history_200");
            }
            _ => panic!("Expected IncrementalSync with new history_id"),
        }
    }

    #[test]
    fn test_sync_state_checkpoint_preservation() {
        // Verify that checkpoints are preserved through state transitions
        let mut state = SyncState::partial(1, "history_100");
        state = state.with_fetch_progress(Some("page_xyz".to_string()), 5000);
        state = state.with_failed_ids(vec!["msg1".to_string(), "msg2".to_string()]);

        // Checkpoint should be preserved
        assert_eq!(state.fetch_page_token, Some("page_xyz".to_string()));
        assert_eq!(state.messages_listed, 5000);
        assert_eq!(state.failed_message_ids.len(), 2);

        // After mark_complete, checkpoints should be cleared
        let completed = state.mark_complete();
        assert!(completed.fetch_page_token.is_none());
        assert_eq!(completed.messages_listed, 0);
        assert!(completed.failed_message_ids.is_empty());
    }
}
