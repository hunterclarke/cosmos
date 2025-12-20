//! Inbox sync implementation

use anyhow::Result;
use chrono::Utc;
use std::collections::HashMap;

use crate::gmail::{normalize_message, GmailClient};
use crate::models::{Message, MessageId, Thread, ThreadId};
use crate::storage::MailStore;

/// Statistics from a sync operation
#[derive(Debug, Default, Clone)]
pub struct SyncStats {
    /// Number of messages fetched from Gmail
    pub messages_fetched: usize,
    /// Number of new messages stored
    pub messages_stored: usize,
    /// Number of messages skipped (already synced)
    pub messages_skipped: usize,
    /// Number of threads created or updated
    pub threads_updated: usize,
    /// Number of errors encountered
    pub errors: usize,
    /// Duration of the sync operation
    pub duration_ms: u64,
}

/// Sync inbox messages from Gmail to local storage
///
/// This operation is idempotent - running it multiple times will not
/// create duplicate messages.
///
/// # Arguments
/// * `gmail` - Gmail API client
/// * `store` - Storage backend
/// * `max_messages` - Maximum number of messages to sync
pub fn sync_inbox(
    gmail: &GmailClient,
    store: &dyn MailStore,
    max_messages: usize,
) -> Result<SyncStats> {
    let start = std::time::Instant::now();
    let mut stats = SyncStats::default();

    // 1. Fetch message IDs from Gmail
    let list_response = gmail.list_messages(max_messages, None)?;
    let message_refs = list_response.messages.unwrap_or_default();
    stats.messages_fetched = message_refs.len();

    if message_refs.is_empty() {
        stats.duration_ms = start.elapsed().as_millis() as u64;
        return Ok(stats);
    }

    // 2. Filter out already-synced messages
    let mut to_fetch: Vec<MessageId> = Vec::new();
    for msg_ref in &message_refs {
        let msg_id = MessageId::new(&msg_ref.id);
        if !store.has_message(&msg_id)? {
            to_fetch.push(msg_id);
        } else {
            stats.messages_skipped += 1;
        }
    }

    if to_fetch.is_empty() {
        stats.duration_ms = start.elapsed().as_millis() as u64;
        return Ok(stats);
    }

    // 3. Fetch full message details
    let results = gmail.get_messages_batch(&to_fetch);

    // 4. Normalize to Orion models and group by thread
    let mut thread_messages: HashMap<ThreadId, Vec<Message>> = HashMap::new();

    for result in results {
        match result {
            Ok(gmail_msg) => match normalize_message(gmail_msg) {
                Ok(message) => {
                    let thread_id = message.thread_id.clone();
                    thread_messages.entry(thread_id).or_default().push(message);
                }
                Err(e) => {
                    eprintln!("Failed to normalize message: {}", e);
                    stats.errors += 1;
                }
            },
            Err(e) => {
                eprintln!("Failed to fetch message: {}", e);
                stats.errors += 1;
            }
        }
    }

    // 5. Upsert threads and messages
    for (thread_id, messages) in thread_messages {
        // Compute thread properties from messages
        let thread = compute_thread(&thread_id, &messages, store)?;

        // Store thread
        store.upsert_thread(thread)?;
        stats.threads_updated += 1;

        // Store messages
        for message in messages {
            store.upsert_message(message)?;
            stats.messages_stored += 1;
        }
    }

    stats.duration_ms = start.elapsed().as_millis() as u64;
    Ok(stats)
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
}
