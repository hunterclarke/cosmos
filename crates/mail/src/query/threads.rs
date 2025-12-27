//! Thread query functions

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::models::{Message, Thread, ThreadId};
use crate::storage::MailStore;

/// Summary information for displaying a thread in a list
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadSummary {
    /// Thread ID
    pub id: ThreadId,
    /// Account ID this thread belongs to
    pub account_id: i64,
    /// Subject line
    pub subject: String,
    /// Preview snippet
    pub snippet: String,
    /// Timestamp of the most recent message
    pub last_message_at: DateTime<Utc>,
    /// Number of messages in the thread
    pub message_count: usize,
    /// Display name of the thread sender
    pub sender_name: Option<String>,
    /// Email address of the thread sender
    pub sender_email: String,
    /// Whether the thread has unread messages
    pub is_unread: bool,
}

impl From<Thread> for ThreadSummary {
    fn from(thread: Thread) -> Self {
        Self {
            id: thread.id,
            account_id: thread.account_id,
            subject: thread.subject,
            snippet: thread.snippet,
            last_message_at: thread.last_message_at,
            message_count: thread.message_count,
            sender_name: thread.sender_name,
            sender_email: thread.sender_email,
            is_unread: thread.is_unread,
        }
    }
}

/// Detailed thread information including all messages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadDetail {
    /// The thread metadata
    pub thread: Thread,
    /// All messages in the thread, ordered chronologically
    pub messages: Vec<Message>,
}

/// List threads with pagination
///
/// Returns threads sorted by last_message_at descending (newest first).
///
/// # Arguments
/// * `store` - The storage backend
/// * `limit` - Maximum number of threads to return
/// * `offset` - Number of threads to skip
pub fn list_threads(
    store: &dyn MailStore,
    limit: usize,
    offset: usize,
) -> Result<Vec<ThreadSummary>> {
    let threads = store.list_threads(limit, offset)?;
    Ok(threads.into_iter().map(ThreadSummary::from).collect())
}

/// List threads by label with pagination
///
/// Returns threads that have at least one message with the given label,
/// sorted by last_message_at descending (newest first).
///
/// # Arguments
/// * `store` - The storage backend
/// * `label` - The label ID to filter by (e.g., "INBOX", "SENT")
/// * `limit` - Maximum number of threads to return
/// * `offset` - Number of threads to skip
pub fn list_threads_by_label(
    store: &dyn MailStore,
    label: &str,
    limit: usize,
    offset: usize,
) -> Result<Vec<ThreadSummary>> {
    let threads = store.list_threads_by_label(label, limit, offset)?;
    Ok(threads.into_iter().map(ThreadSummary::from).collect())
}

/// Get detailed thread information including all messages with bodies
///
/// This loads full message content including bodies from blob storage.
/// For a lightweight view without bodies, use `list_messages_for_thread` directly.
///
/// # Arguments
/// * `store` - The storage backend
/// * `thread_id` - The thread to fetch
pub fn get_thread_detail(
    store: &dyn MailStore,
    thread_id: &ThreadId,
) -> Result<Option<ThreadDetail>> {
    let thread = match store.get_thread(thread_id)? {
        Some(t) => t,
        None => return Ok(None),
    };

    // Load full messages with bodies for rendering
    let messages = store.list_messages_for_thread_with_bodies(thread_id)?;

    Ok(Some(ThreadDetail { thread, messages }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{EmailAddress, MessageId};
    use crate::storage::InMemoryMailStore;

    fn setup_test_store() -> InMemoryMailStore {
        let store = InMemoryMailStore::new();

        // Create some test threads
        for i in 0..5 {
            let thread = Thread::new(
                ThreadId::new(format!("t{}", i)),
                1, // account_id
                format!("Thread {}", i),
                format!("Snippet {}", i),
                Utc::now() - chrono::Duration::hours(i as i64),
                2,
                Some(format!("Test User {}", i)),
                format!("test{}@example.com", i),
                i % 2 == 0, // alternate unread
            );
            store.upsert_thread(thread).unwrap();

            // Add messages
            for j in 0..2 {
                let msg = crate::models::Message::builder(
                    MessageId::new(format!("m{}_{}", i, j)),
                    ThreadId::new(format!("t{}", i)),
                )
                .from(EmailAddress::new("test@example.com"))
                .subject(format!("Thread {}", i))
                .body_preview(format!("Message {} body", j))
                .received_at(Utc::now() - chrono::Duration::hours(i as i64 * 2 + j as i64))
                .build();
                store.upsert_message(msg).unwrap();
            }
        }

        store
    }

    #[test]
    fn test_list_threads() {
        let store = setup_test_store();

        let threads = list_threads(&store, 3, 0).unwrap();
        assert_eq!(threads.len(), 3);
        // Should be sorted by last_message_at descending
        assert_eq!(threads[0].id.0, "t0");
        assert_eq!(threads[1].id.0, "t1");
        assert_eq!(threads[2].id.0, "t2");
    }

    #[test]
    fn test_list_threads_pagination() {
        let store = setup_test_store();

        let page1 = list_threads(&store, 2, 0).unwrap();
        let page2 = list_threads(&store, 2, 2).unwrap();

        assert_eq!(page1.len(), 2);
        assert_eq!(page2.len(), 2);
        assert_ne!(page1[0].id, page2[0].id);
    }

    #[test]
    fn test_get_thread_detail() {
        let store = setup_test_store();

        let detail = get_thread_detail(&store, &ThreadId::new("t0")).unwrap();
        assert!(detail.is_some());

        let detail = detail.unwrap();
        assert_eq!(detail.thread.id.0, "t0");
        assert_eq!(detail.messages.len(), 2);
    }

    #[test]
    fn test_get_thread_detail_not_found() {
        let store = setup_test_store();

        let detail = get_thread_detail(&store, &ThreadId::new("nonexistent")).unwrap();
        assert!(detail.is_none());
    }
}
