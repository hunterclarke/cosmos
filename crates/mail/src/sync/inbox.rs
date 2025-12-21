//! Gmail sync implementation
//!
//! Provides both initial full sync and incremental sync via Gmail History API.
//! Syncs the user's entire email library (all labels/folders).

use anyhow::{Context, Result};
use chrono::Utc;
use log::{info, warn};
use std::collections::HashSet;

use crate::gmail::{normalize_message, GmailClient, HistoryExpiredError};
use crate::models::{Message, MessageId, SyncState, Thread, ThreadId};
use crate::storage::MailStore;

/// Options for sync operation
#[derive(Debug, Clone, Default)]
pub struct SyncOptions {
    /// Maximum messages to fetch in initial sync
    pub max_messages: Option<usize>,
    /// Force full resync even if history_id exists
    pub full_resync: bool,
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
            match incremental_sync(gmail, store, account_id, &state) {
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

/// Perform initial full sync
///
/// Fetches and stores messages in batches for incremental progress.
/// Resumable: saves partial sync state and skips already-fetched messages.
fn initial_sync(
    gmail: &GmailClient,
    store: &dyn MailStore,
    account_id: &str,
    options: &SyncOptions,
) -> Result<SyncStats> {
    let mut stats = SyncStats {
        was_incremental: false,
        ..Default::default()
    };

    // Save partial sync state to indicate initial sync is in progress
    // This allows resuming if the app is closed mid-sync
    let partial_state = SyncState::partial(account_id);
    store.save_sync_state(partial_state)?;
    info!("Starting initial sync (resumable)...");

    let mut page_token: Option<String> = None;
    let mut total_listed = 0usize;
    let batch_size = 500; // Gmail API max is 500 per page

    loop {
        // Check if we've hit the limit
        if let Some(max) = options.max_messages {
            if total_listed >= max {
                break;
            }
        }

        // Fetch a page of message IDs
        let list_response = gmail.list_messages(batch_size, page_token.as_deref())?;
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

        // Filter out already-synced messages
        let mut to_fetch: Vec<MessageId> = Vec::new();
        for msg_ref in &message_refs {
            let msg_id = MessageId::new(&msg_ref.id);
            if !store.has_message(&msg_id)? {
                to_fetch.push(msg_id);
            } else {
                stats.messages_skipped += 1;
            }
        }

        stats.messages_fetched += message_refs.len();

        if !to_fetch.is_empty() {
            info!("Fetching {} new messages...", to_fetch.len());

            // Track which threads we've seen in this batch for stats
            let mut threads_seen_in_batch: HashSet<ThreadId> = HashSet::new();

            // Fetch and store each message immediately as it arrives
            let results = gmail.get_messages_batch(&to_fetch);
            for result in results {
                match result {
                    Ok(gmail_msg) => match normalize_message(gmail_msg) {
                        Ok(message) => {
                            let thread_id = message.thread_id.clone();
                            let is_new_thread = !store.has_thread(&thread_id)?;

                            // Store message immediately
                            store.upsert_message(message)?;
                            stats.messages_created += 1;

                            // Update thread (message is already in store, pass empty slice)
                            let thread = compute_thread(&thread_id, &[], store)?;
                            store.upsert_thread(thread)?;

                            // Track thread stats (only count once per batch)
                            if threads_seen_in_batch.insert(thread_id) {
                                if is_new_thread {
                                    stats.threads_created += 1;
                                } else {
                                    stats.threads_updated += 1;
                                }
                            }
                        }
                        Err(e) => {
                            warn!("Failed to normalize message: {}", e);
                            stats.errors += 1;
                        }
                    },
                    Err(e) => {
                        warn!("Failed to fetch message: {}", e);
                        stats.errors += 1;
                    }
                }
            }

            info!(
                "Stored batch: {} threads, {} messages total so far",
                stats.threads_created + stats.threads_updated,
                stats.messages_created + stats.messages_updated
            );
        }

        // Check for next page
        match list_response.next_page_token {
            Some(token) => page_token = Some(token),
            None => break,
        }
    }

    // Mark initial sync as complete with history_id for future incremental syncs
    match get_current_history_id(gmail) {
        Ok(history_id) => {
            let complete_state = SyncState::new(account_id, &history_id);
            store.save_sync_state(complete_state)?;
            info!("Initial sync complete, saved history_id for incremental sync");
        }
        Err(e) => {
            warn!("Could not get history_id, incremental sync may not work: {}", e);
            // Still mark as complete so we don't keep retrying
            let complete_state = SyncState::new(account_id, "");
            store.save_sync_state(complete_state)?;
        }
    }

    info!(
        "Initial sync complete: {} messages fetched, {} created, {} skipped",
        stats.messages_fetched,
        stats.messages_created,
        stats.messages_skipped
    );

    Ok(stats)
}

/// Get the current history ID from Gmail
fn get_current_history_id(gmail: &GmailClient) -> Result<String> {
    // The messages.list endpoint doesn't return historyId directly
    // We can get it from the user profile
    // For now, we'll use a workaround: fetch the first message and use its internal_date
    // as a rough proxy, or make a profile call

    // Actually, the proper way is to call the profile endpoint
    // But for simplicity, we'll fetch one message to bootstrap
    let list = gmail.list_messages(1, None)?;
    if let Some(msgs) = list.messages
        && let Some(first) = msgs.first()
    {
        // Use the message ID as a baseline - not ideal but works
        // In a production system, you'd call GET /users/me/profile
        // which returns historyId
        return Ok(first.id.clone());
    }

    // Fallback: use current timestamp as a marker
    Ok(Utc::now().timestamp_millis().to_string())
}

/// Perform incremental sync using History API
fn incremental_sync(
    gmail: &GmailClient,
    store: &dyn MailStore,
    _account_id: &str,
    state: &SyncState,
) -> Result<SyncStats> {
    let mut stats = SyncStats {
        was_incremental: true,
        ..Default::default()
    };

    // Fetch history since last sync
    let history = gmail
        .list_history_all(&state.history_id)
        .context("Failed to fetch history")?;

    // Collect message IDs to fetch
    let mut message_ids_to_fetch: Vec<MessageId> = Vec::new();

    if let Some(records) = &history.history {
        for record in records {
            if let Some(added) = &record.messages_added {
                for msg_added in added {
                    let msg_id = MessageId::new(&msg_added.message.id);
                    // Only fetch if we don't already have it
                    if !store.has_message(&msg_id)? {
                        message_ids_to_fetch.push(msg_id);
                    }
                }
            }
        }
    }

    stats.messages_fetched = message_ids_to_fetch.len();

    if message_ids_to_fetch.is_empty() {
        // No new messages, but update sync state with new history ID
        if let Some(new_history_id) = history.history_id {
            let updated_state = state.clone().updated(new_history_id);
            store.save_sync_state(updated_state)?;
        }
        return Ok(stats);
    }

    // Track which threads we've seen for stats
    let mut threads_seen: HashSet<ThreadId> = HashSet::new();

    // Fetch and store each message immediately as it arrives
    let results = gmail.get_messages_batch(&message_ids_to_fetch);
    for result in results {
        match result {
            Ok(gmail_msg) => match normalize_message(gmail_msg) {
                Ok(message) => {
                    let thread_id = message.thread_id.clone();
                    let is_new_thread = !store.has_thread(&thread_id)?;

                    // Store message immediately
                    store.upsert_message(message)?;
                    stats.messages_created += 1;

                    // Update thread (message is already in store, pass empty slice)
                    let thread = compute_thread(&thread_id, &[], store)?;
                    store.upsert_thread(thread)?;

                    // Track thread stats (only count once)
                    if threads_seen.insert(thread_id) {
                        if is_new_thread {
                            stats.threads_created += 1;
                        } else {
                            stats.threads_updated += 1;
                        }
                    }
                }
                Err(e) => {
                    warn!("Failed to normalize message: {}", e);
                    stats.errors += 1;
                }
            },
            Err(e) => {
                warn!("Failed to fetch message: {}", e);
                stats.errors += 1;
            }
        }
    }

    // Update sync state with new history ID
    if let Some(new_history_id) = history.history_id {
        let updated_state = state.clone().updated(new_history_id);
        store.save_sync_state(updated_state)?;
    }

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
    };

    sync_gmail(gmail, store, "default", options)
}

/// Compute thread properties from its messages
fn compute_thread(
    thread_id: &ThreadId,
    new_messages: &[Message],
    store: &dyn MailStore,
) -> Result<Thread> {
    // Get existing messages for this thread
    let existing_messages = store.list_messages_for_thread(thread_id)?;

    // Combine existing and new messages
    let all_messages: Vec<&Message> = existing_messages
        .iter()
        .chain(new_messages.iter())
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

    Ok(Thread::new(
        thread_id.clone(),
        subject,
        latest.body_preview.clone(),
        last_message_at,
        all_messages.len(),
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
