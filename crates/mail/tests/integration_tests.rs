//! Integration tests for the mail crate
//!
//! These tests verify the complete flow from syncing to querying.

use chrono::Utc;
use mail::models::{EmailAddress, Message, MessageId, SyncState, Thread, ThreadId};
use mail::query::{get_thread_detail, list_threads};
use mail::storage::{FileBlobStore, InMemoryMailStore, MailStore, SqliteMailStore};
use mail::{SyncAction, determine_sync_action, get_sync_state_info, should_auto_sync_on_startup};
use tempfile::TempDir;

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

// === SQLite Sync State Persistence Tests ===

fn create_sqlite_store() -> (SqliteMailStore, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    // Use .test.sqlite extension to clearly distinguish from production databases
    let db_path = temp_dir.path().join("mail.test.sqlite");
    let blob_path = temp_dir.path().join("blobs.test");
    let blob_store = Box::new(FileBlobStore::new(&blob_path).unwrap());
    let store = SqliteMailStore::new(&db_path, blob_store).unwrap();
    (store, temp_dir)
}

#[test]
fn test_sqlite_sync_state_persistence() {
    let (store, _temp_dir) = create_sqlite_store();

    // Initially no sync state
    assert!(store.get_sync_state("default").unwrap().is_none());

    // Save partial sync state (simulating start of initial sync)
    let partial = SyncState::partial("default", "history_12345");
    store.save_sync_state(partial.clone()).unwrap();

    // Verify it persists
    let retrieved = store.get_sync_state("default").unwrap().unwrap();
    assert_eq!(retrieved.account_id, "default");
    assert_eq!(retrieved.history_id, "history_12345");
    assert!(!retrieved.initial_sync_complete);
}

#[test]
fn test_sqlite_sync_checkpoint_persistence() {
    let (store, _temp_dir) = create_sqlite_store();

    // Save partial state with checkpoint
    let mut state = SyncState::partial("default", "history_100");
    state = state.with_fetch_progress(Some("page_token_xyz".to_string()), 5000);
    state = state.with_failed_ids(vec!["msg1".to_string(), "msg2".to_string(), "msg3".to_string()]);
    store.save_sync_state(state).unwrap();

    // Retrieve and verify checkpoint data
    let retrieved = store.get_sync_state("default").unwrap().unwrap();
    assert_eq!(retrieved.fetch_page_token, Some("page_token_xyz".to_string()));
    assert_eq!(retrieved.messages_listed, 5000);
    assert_eq!(retrieved.failed_message_ids.len(), 3);
    assert!(retrieved.failed_message_ids.contains(&"msg1".to_string()));
    assert!(retrieved.failed_message_ids.contains(&"msg2".to_string()));
    assert!(retrieved.failed_message_ids.contains(&"msg3".to_string()));
}

#[test]
fn test_sqlite_sync_state_across_reopens() {
    let temp_dir = TempDir::new().unwrap();
    // Use .test.sqlite extension to clearly distinguish from production databases
    let db_path = temp_dir.path().join("mail.test.sqlite");
    let blob_path = temp_dir.path().join("blobs.test");

    // Open store, save state, close
    {
        let blob_store = Box::new(FileBlobStore::new(&blob_path).unwrap());
        let store = SqliteMailStore::new(&db_path, blob_store).unwrap();
        let mut state = SyncState::partial("default", "history_200");
        state = state.with_fetch_progress(Some("page_abc".to_string()), 10000);
        state.failed_message_ids = vec!["fail1".to_string()];
        store.save_sync_state(state).unwrap();
    } // store dropped here, connection closed

    // Reopen store and verify state persists
    {
        let blob_store = Box::new(FileBlobStore::new(&blob_path).unwrap());
        let store = SqliteMailStore::new(&db_path, blob_store).unwrap();
        let retrieved = store.get_sync_state("default").unwrap().unwrap();

        assert_eq!(retrieved.history_id, "history_200");
        assert!(!retrieved.initial_sync_complete);
        assert_eq!(retrieved.fetch_page_token, Some("page_abc".to_string()));
        assert_eq!(retrieved.messages_listed, 10000);
        assert_eq!(retrieved.failed_message_ids, vec!["fail1".to_string()]);

        // Verify should_auto_sync detects incomplete sync
        assert!(should_auto_sync_on_startup(Some(&retrieved)));

        // Verify determine_sync_action returns correct resume info
        match determine_sync_action(Some(&retrieved), false) {
            SyncAction::ResumeInitialSync { page_token, messages_listed, failed_message_ids } => {
                assert_eq!(page_token, Some("page_abc".to_string()));
                assert_eq!(messages_listed, 10000);
                assert_eq!(failed_message_ids.len(), 1);
            }
            other => panic!("Expected ResumeInitialSync, got {:?}", other),
        }
    }
}

#[test]
fn test_sqlite_complete_sync_lifecycle() {
    let (store, _temp_dir) = create_sqlite_store();

    // Step 1: Initial state - no sync
    let state = store.get_sync_state("default").unwrap();
    assert!(state.is_none());
    assert!(should_auto_sync_on_startup(state.as_ref()));
    assert_eq!(determine_sync_action(state.as_ref(), false), SyncAction::InitialSync);

    // Step 2: Start initial sync (partial state)
    let partial = SyncState::partial("default", "history_100");
    store.save_sync_state(partial).unwrap();

    let state = store.get_sync_state("default").unwrap();
    assert!(should_auto_sync_on_startup(state.as_ref()));
    assert!(matches!(
        determine_sync_action(state.as_ref(), false),
        SyncAction::ResumeInitialSync { .. }
    ));

    // Step 3: Update checkpoint during sync
    let mut checkpointed = state.unwrap();
    checkpointed = checkpointed.with_fetch_progress(Some("page_1".to_string()), 500);
    store.save_sync_state(checkpointed).unwrap();

    // Step 4: More progress
    let state = store.get_sync_state("default").unwrap().unwrap();
    let more_progress = state.with_fetch_progress(Some("page_2".to_string()), 1000);
    store.save_sync_state(more_progress).unwrap();

    // Step 5: Complete sync
    let state = store.get_sync_state("default").unwrap().unwrap();
    let complete = state.mark_complete();
    store.save_sync_state(complete).unwrap();

    let final_state = store.get_sync_state("default").unwrap();
    assert!(!should_auto_sync_on_startup(final_state.as_ref()));
    assert!(matches!(
        determine_sync_action(final_state.as_ref(), false),
        SyncAction::IncrementalSync { .. }
    ));

    // Verify checkpoints are cleared after completion
    let complete_state = final_state.unwrap();
    assert!(complete_state.initial_sync_complete);
    assert!(complete_state.fetch_page_token.is_none());
    assert!(complete_state.failed_message_ids.is_empty());
    assert_eq!(complete_state.messages_listed, 0);
}

#[test]
fn test_sqlite_sync_state_with_mail_data() {
    let (store, _temp_dir) = create_sqlite_store();

    // Save some mail data
    store
        .upsert_thread(make_thread("t1", "Test Thread", 1, 1))
        .unwrap();
    store
        .upsert_message(make_message("m1", "t1", "Test", 1))
        .unwrap();

    // Save sync state
    let state = SyncState::new("default", "history_500");
    store.save_sync_state(state).unwrap();

    // Clear mail data only (not sync state)
    store.clear_mail_data().unwrap();

    // Mail data should be gone
    assert_eq!(store.count_threads().unwrap(), 0);
    assert!(!store.has_message(&MessageId::new("m1")).unwrap());

    // But sync state should still exist
    let state = store.get_sync_state("default").unwrap();
    assert!(state.is_some());
    assert_eq!(state.unwrap().history_id, "history_500");
}

#[test]
fn test_sqlite_stale_sync_detection() {
    let (store, _temp_dir) = create_sqlite_store();

    // Create a sync state from 6 days ago
    let mut stale_state = SyncState::new("default", "old_history");
    stale_state.last_sync_at = Utc::now() - chrono::Duration::days(6);
    store.save_sync_state(stale_state).unwrap();

    let state = store.get_sync_state("default").unwrap();

    // Should NOT auto-sync (completed sync)
    assert!(!should_auto_sync_on_startup(state.as_ref()));

    // But should recommend stale resync
    match determine_sync_action(state.as_ref(), false) {
        SyncAction::StaleResync { days_since_sync } => {
            assert_eq!(days_since_sync, 6);
        }
        other => panic!("Expected StaleResync, got {:?}", other),
    }
}

#[test]
fn test_sqlite_force_resync() {
    let (store, _temp_dir) = create_sqlite_store();

    // Create complete sync state
    let state = SyncState::new("default", "history_999");
    store.save_sync_state(state).unwrap();

    let state = store.get_sync_state("default").unwrap();

    // Force resync should override everything
    assert_eq!(
        determine_sync_action(state.as_ref(), true),
        SyncAction::InitialSync
    );
}

#[test]
fn test_sync_state_info_with_sqlite() {
    let (store, _temp_dir) = create_sqlite_store();

    // No state
    let info = get_sync_state_info(None);
    assert!(!info.has_completed_sync);
    assert!(!info.needs_resume);

    // Partial state with progress
    let mut partial = SyncState::partial("default", "h1");
    partial = partial.with_fetch_progress(Some("token".to_string()), 2500);
    partial.failed_message_ids = vec!["f1".to_string(), "f2".to_string()];
    store.save_sync_state(partial).unwrap();

    let state = store.get_sync_state("default").unwrap();
    let info = get_sync_state_info(state.as_ref());

    assert!(!info.has_completed_sync);
    assert!(info.needs_resume);
    assert!(info.last_sync_at.is_some());

    let progress = info.resume_progress.unwrap();
    assert!(progress.has_page_token);
    assert_eq!(progress.messages_listed, 2500);
    assert_eq!(progress.failed_message_count, 2);
}
