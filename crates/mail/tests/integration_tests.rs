//! Integration tests for the mail crate
//!
//! These tests verify the complete flow from syncing to querying.

use chrono::Utc;
use mail::models::{EmailAddress, Message, MessageId, Thread, ThreadId};
use mail::query::{get_thread_detail, list_threads};
use mail::storage::{InMemoryMailStore, MailStore};

/// Helper to create test messages
fn make_message(id: &str, thread_id: &str, subject: &str, age_hours: i64) -> Message {
    let received_at = Utc::now() - chrono::Duration::hours(age_hours);
    Message::builder(MessageId::new(id), ThreadId::new(thread_id))
        .from(EmailAddress::with_name("Test User", "test@example.com"))
        .to(vec![EmailAddress::new("recipient@example.com")])
        .subject(subject)
        .body_preview(format!("This is the preview for message {}", id))
        .received_at(received_at)
        .internal_date(received_at.timestamp_millis())
        .build()
}

/// Helper to create test threads
fn make_thread(id: &str, subject: &str, message_count: usize, age_hours: i64) -> Thread {
    Thread::new(
        ThreadId::new(id),
        subject.to_string(),
        format!("Snippet for thread {}", id),
        Utc::now() - chrono::Duration::hours(age_hours),
        message_count,
        Some("Test User".to_string()),
        "test@example.com".to_string(),
        false,
    )
}

#[test]
fn test_full_sync_simulation() {
    let store = InMemoryMailStore::new();

    // Simulate syncing messages
    let messages = vec![
        make_message("m1", "t1", "First Thread", 3),
        make_message("m2", "t1", "Re: First Thread", 2),
        make_message("m3", "t2", "Second Thread", 1),
    ];

    // Store messages
    for msg in &messages {
        store.upsert_message(msg.clone()).unwrap();
    }

    // Create corresponding threads
    store
        .upsert_thread(make_thread("t1", "First Thread", 2, 2))
        .unwrap();
    store
        .upsert_thread(make_thread("t2", "Second Thread", 1, 1))
        .unwrap();

    // Verify threads are stored
    let threads = list_threads(&store, 10, 0).unwrap();
    assert_eq!(threads.len(), 2);

    // Verify t2 comes first (more recent)
    assert_eq!(threads[0].id.as_str(), "t2");
    assert_eq!(threads[1].id.as_str(), "t1");

    // Verify thread details
    let detail = get_thread_detail(&store, &ThreadId::new("t1"))
        .unwrap()
        .unwrap();
    assert_eq!(detail.messages.len(), 2);
    assert_eq!(detail.thread.subject, "First Thread");
}

#[test]
fn test_idempotent_sync() {
    let store = InMemoryMailStore::new();

    let message = make_message("m1", "t1", "Test", 1);

    // First sync
    store.upsert_message(message.clone()).unwrap();

    // Simulate second sync - same message
    store.upsert_message(message.clone()).unwrap();

    // Should still have only one message
    let messages = store
        .list_messages_for_thread(&ThreadId::new("t1"))
        .unwrap();
    assert_eq!(messages.len(), 1);
}

#[test]
fn test_incremental_sync() {
    let store = InMemoryMailStore::new();

    // Initial sync with 2 messages
    store
        .upsert_message(make_message("m1", "t1", "Thread", 3))
        .unwrap();
    store
        .upsert_message(make_message("m2", "t1", "Re: Thread", 2))
        .unwrap();
    store
        .upsert_thread(make_thread("t1", "Thread", 2, 2))
        .unwrap();

    // Check message exists
    assert!(store.has_message(&MessageId::new("m1")).unwrap());
    assert!(store.has_message(&MessageId::new("m2")).unwrap());
    assert!(!store.has_message(&MessageId::new("m3")).unwrap());

    // Second sync with new message
    store
        .upsert_message(make_message("m3", "t1", "Re: Re: Thread", 1))
        .unwrap();

    // Update thread
    store
        .upsert_thread(make_thread("t1", "Thread", 3, 1))
        .unwrap();

    // Verify incremental update
    let detail = get_thread_detail(&store, &ThreadId::new("t1"))
        .unwrap()
        .unwrap();
    assert_eq!(detail.messages.len(), 3);
    assert_eq!(detail.thread.message_count, 3);
}

#[test]
fn test_thread_reconstruction() {
    let store = InMemoryMailStore::new();

    // Messages arrive out of order
    store
        .upsert_message(make_message("m2", "t1", "Re: Topic", 2))
        .unwrap();
    store
        .upsert_message(make_message("m1", "t1", "Topic", 3))
        .unwrap();
    store
        .upsert_message(make_message("m3", "t1", "Re: Re: Topic", 1))
        .unwrap();

    store
        .upsert_thread(make_thread("t1", "Topic", 3, 1))
        .unwrap();

    // Messages should be sorted chronologically
    let detail = get_thread_detail(&store, &ThreadId::new("t1"))
        .unwrap()
        .unwrap();
    assert_eq!(detail.messages.len(), 3);
    assert_eq!(detail.messages[0].id.as_str(), "m1"); // Oldest
    assert_eq!(detail.messages[1].id.as_str(), "m2");
    assert_eq!(detail.messages[2].id.as_str(), "m3"); // Newest
}

#[test]
fn test_multiple_threads() {
    let store = InMemoryMailStore::new();

    // Create 10 threads with varying recency
    for i in 0..10 {
        let thread_id = format!("t{}", i);
        store
            .upsert_message(make_message(
                &format!("m{}", i),
                &thread_id,
                &format!("Thread {}", i),
                (10 - i) as i64,
            ))
            .unwrap();
        store
            .upsert_thread(make_thread(
                &thread_id,
                &format!("Thread {}", i),
                1,
                (10 - i) as i64,
            ))
            .unwrap();
    }

    // List all threads
    let threads = list_threads(&store, 100, 0).unwrap();
    assert_eq!(threads.len(), 10);

    // Most recent should be first (t9 is newest)
    assert_eq!(threads[0].id.as_str(), "t9");
    assert_eq!(threads[9].id.as_str(), "t0");

    // Test pagination
    let page1 = list_threads(&store, 3, 0).unwrap();
    let page2 = list_threads(&store, 3, 3).unwrap();
    assert_eq!(page1.len(), 3);
    assert_eq!(page2.len(), 3);
    assert_eq!(page1[0].id.as_str(), "t9");
    assert_eq!(page2[0].id.as_str(), "t6");
}

#[test]
fn test_empty_store() {
    let store = InMemoryMailStore::new();

    let threads = list_threads(&store, 10, 0).unwrap();
    assert!(threads.is_empty());

    let detail = get_thread_detail(&store, &ThreadId::new("nonexistent")).unwrap();
    assert!(detail.is_none());
}

#[test]
fn test_clear_store() {
    let store = InMemoryMailStore::new();

    // Add some data
    store
        .upsert_message(make_message("m1", "t1", "Test", 1))
        .unwrap();
    store
        .upsert_thread(make_thread("t1", "Test", 1, 1))
        .unwrap();

    assert_eq!(store.count_threads().unwrap(), 1);

    // Clear
    store.clear().unwrap();

    assert_eq!(store.count_threads().unwrap(), 0);
    assert!(!store.has_message(&MessageId::new("m1")).unwrap());
}
