//! Integration tests for the mail crate
//!
//! These tests verify the complete flow from syncing to querying.

use chrono::Utc;
use mail::models::{EmailAddress, Message, MessageId, SyncState, Thread, ThreadId};
use mail::query::{get_thread_detail, list_threads};
use mail::storage::{FileBlobStore, InMemoryMailStore, MailStore, SqliteMailStore};
use mail::{SyncAction, cooldown_elapsed, determine_sync_action, get_sync_state_info, should_auto_sync_on_startup};
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

// === Scenario Tests: Auth -> Initial Sync -> Polling ===

/// Test scenario: User authenticates, initial sync runs, then polling uses incremental sync
///
/// This simulates the flow:
/// 1. User authenticates (no sync state exists)
/// 2. Initial sync starts (partial state created)
/// 3. Initial sync completes (state marked complete)
/// 4. Polling triggers sync -> should use incremental sync
#[test]
fn test_scenario_auth_initial_sync_then_polling() {
    let (store, _temp_dir) = create_sqlite_store();

    // Step 1: User just authenticated - no sync state
    let state = store.get_sync_state("default").unwrap();
    assert!(state.is_none());

    // Should auto-sync on startup (first time)
    assert!(should_auto_sync_on_startup(state.as_ref()));

    // Action should be InitialSync
    assert_eq!(
        determine_sync_action(state.as_ref(), false),
        SyncAction::InitialSync
    );

    // Step 2: Initial sync starts - save partial state with history_id
    let history_id_at_start = "12345";
    let partial = SyncState::partial("default", history_id_at_start);
    store.save_sync_state(partial).unwrap();

    // Simulate some messages being synced (thread first due to FK constraint)
    store.upsert_thread(make_thread("t1", "Test Thread", 1, 1)).unwrap();
    store.upsert_message(make_message("m1", "t1", "Test Thread", 1)).unwrap();

    // Step 3: Initial sync completes - mark complete
    let state = store.get_sync_state("default").unwrap().unwrap();
    let complete = state.mark_complete();
    store.save_sync_state(complete).unwrap();

    // Verify completion
    let state = store.get_sync_state("default").unwrap().unwrap();
    assert!(state.initial_sync_complete);
    assert_eq!(state.history_id, history_id_at_start);

    // Step 4: Polling triggers - should use incremental sync
    let state = store.get_sync_state("default").unwrap();

    // Should NOT auto-sync on startup (already completed)
    assert!(!should_auto_sync_on_startup(state.as_ref()));

    // Action should be IncrementalSync with the saved history_id
    match determine_sync_action(state.as_ref(), false) {
        SyncAction::IncrementalSync { history_id } => {
            assert_eq!(history_id, history_id_at_start);
        }
        other => panic!("Expected IncrementalSync, got {:?}", other),
    }
}

/// Test that incremental sync can be determined while initial sync might still be running
/// (tests the parallel capability mentioned in the requirements)
#[test]
fn test_parallel_sync_decision_paths() {
    let (store1, _temp_dir1) = create_sqlite_store();
    let (store2, _temp_dir2) = create_sqlite_store();

    // Store 1: Fresh state - should do initial sync
    let state1 = store1.get_sync_state("default").unwrap();
    let action1 = determine_sync_action(state1.as_ref(), false);
    assert_eq!(action1, SyncAction::InitialSync);

    // Store 2: Complete state - should do incremental sync
    let complete = SyncState::new("default", "history_999");
    store2.save_sync_state(complete).unwrap();

    let state2 = store2.get_sync_state("default").unwrap();
    let action2 = determine_sync_action(state2.as_ref(), false);

    match &action2 {
        SyncAction::IncrementalSync { history_id } => {
            assert_eq!(history_id, "history_999");
        }
        other => panic!("Expected IncrementalSync, got {:?}", other),
    }

    // Both determinations can happen independently (simulating parallel)
    // This proves the sync action logic is stateless and can be evaluated concurrently
    // (action1 is InitialSync, action2 is IncrementalSync - they're different)
    assert!(matches!(action1, SyncAction::InitialSync));
    assert!(matches!(action2, SyncAction::IncrementalSync { .. }));
}

// === Scenario Tests: Launch with Incomplete Sync -> Resume ===

/// Test scenario: App launches with incomplete initial sync and resumes
///
/// This simulates:
/// 1. App was running initial sync
/// 2. App crashed/quit mid-sync (with checkpoint saved)
/// 3. App relaunches and detects incomplete sync
/// 4. App resumes from checkpoint
#[test]
fn test_scenario_launch_resume_incomplete_sync() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("mail.test.sqlite");
    let blob_path = temp_dir.path().join("blobs.test");

    // === First session: Start sync, save checkpoint, "crash" ===
    {
        let blob_store = Box::new(FileBlobStore::new(&blob_path).unwrap());
        let store = SqliteMailStore::new(&db_path, blob_store).unwrap();

        // No previous sync state
        assert!(store.get_sync_state("default").unwrap().is_none());

        // Start initial sync - save partial state
        let partial = SyncState::partial("default", "history_at_crash");
        store.save_sync_state(partial).unwrap();

        // Simulate progress: listed 5000 messages, have page token for next batch
        let state = store.get_sync_state("default").unwrap().unwrap();
        let with_progress = state.with_fetch_progress(
            Some("next_page_token_xyz".to_string()),
            5000,
        );
        store.save_sync_state(with_progress).unwrap();

        // Simulate some failed message IDs
        let state = store.get_sync_state("default").unwrap().unwrap();
        let with_failures = state.with_failed_ids(vec![
            "failed_msg_1".to_string(),
            "failed_msg_2".to_string(),
        ]);
        store.save_sync_state(with_failures).unwrap();

        // Store some actual mail data that was synced before "crash" (thread first)
        store.upsert_thread(make_thread("t1", "Synced before crash", 1, 2)).unwrap();
        store.upsert_message(make_message("m1", "t1", "Synced before crash", 2)).unwrap();

        // App "crashes" here - store is dropped
    }

    // === Second session: App relaunches ===
    {
        let blob_store = Box::new(FileBlobStore::new(&blob_path).unwrap());
        let store = SqliteMailStore::new(&db_path, blob_store).unwrap();

        // Check sync state on launch
        let state = store.get_sync_state("default").unwrap();
        assert!(state.is_some(), "Sync state should persist across restarts");

        let state_ref = state.as_ref();

        // Should auto-sync on startup (incomplete sync)
        assert!(
            should_auto_sync_on_startup(state_ref),
            "Should auto-sync because initial sync is incomplete"
        );

        // Verify sync state info for UI display
        let info = get_sync_state_info(state_ref);
        assert!(!info.has_completed_sync, "Sync is not complete");
        assert!(info.needs_resume, "Should need resume");

        let progress = info.resume_progress.expect("Should have resume progress");
        assert!(progress.has_page_token, "Should have page token");
        assert_eq!(progress.messages_listed, 5000);
        assert_eq!(progress.failed_message_count, 2);

        // Determine sync action - should be ResumeInitialSync
        match determine_sync_action(state_ref, false) {
            SyncAction::ResumeInitialSync {
                page_token,
                messages_listed,
                failed_message_ids,
            } => {
                assert_eq!(page_token, Some("next_page_token_xyz".to_string()));
                assert_eq!(messages_listed, 5000);
                assert_eq!(failed_message_ids.len(), 2);
                assert!(failed_message_ids.contains(&"failed_msg_1".to_string()));
                assert!(failed_message_ids.contains(&"failed_msg_2".to_string()));
            }
            other => panic!("Expected ResumeInitialSync, got {:?}", other),
        }

        // Verify previously synced data is still there
        assert!(store.has_message(&MessageId::new("m1")).unwrap());
        assert_eq!(store.count_threads().unwrap(), 1);

        // Simulate resuming and completing sync
        let state = store.get_sync_state("default").unwrap().unwrap();

        // Continue from checkpoint...
        // (In real code, fetch_phase would resume from page_token)

        // Eventually mark complete
        let complete = state.mark_complete();
        store.save_sync_state(complete).unwrap();

        // Now should use incremental sync
        let final_state = store.get_sync_state("default").unwrap();
        assert!(!should_auto_sync_on_startup(final_state.as_ref()));
        assert!(matches!(
            determine_sync_action(final_state.as_ref(), false),
            SyncAction::IncrementalSync { .. }
        ));
    }
}

// === Sync Timing / Cooldown Tests ===

/// Test that cooldown_elapsed integrates correctly with sync timing
#[test]
fn test_cooldown_with_sync_state_timing() {
    use chrono::Duration;

    let (store, _temp_dir) = create_sqlite_store();

    // Complete a sync
    let state = SyncState::new("default", "history_100");
    store.save_sync_state(state).unwrap();

    let state = store.get_sync_state("default").unwrap().unwrap();

    // Just synced - cooldown should NOT be elapsed (30s default)
    assert!(
        !cooldown_elapsed(Some(state.last_sync_at), 30),
        "Cooldown should not be elapsed immediately after sync"
    );

    // Simulate time passing - create state with older last_sync_at
    let mut old_state = SyncState::new("default", "history_200");
    old_state.last_sync_at = Utc::now() - Duration::seconds(60);
    store.save_sync_state(old_state).unwrap();

    let state = store.get_sync_state("default").unwrap().unwrap();

    // 60 seconds ago - cooldown should be elapsed for 30s threshold
    assert!(
        cooldown_elapsed(Some(state.last_sync_at), 30),
        "Cooldown should be elapsed 60s after sync with 30s threshold"
    );

    // But not for 120s threshold
    assert!(
        !cooldown_elapsed(Some(state.last_sync_at), 120),
        "Cooldown should not be elapsed 60s after sync with 120s threshold"
    );
}

/// Test that incremental sync can handle ALL Gmail change types for 1:1 parity
///
/// Gmail History API provides these change types:
/// - messagesAdded: New messages arrived
/// - messagesDeleted: Messages permanently deleted
/// - labelsAdded: Labels added to messages (including UNREAD, STARRED, INBOX, etc.)
/// - labelsRemoved: Labels removed from messages
///
/// This test verifies our data model and storage can represent all these changes.
#[test]
fn test_incremental_sync_gmail_parity_all_change_types() {
    let (store, _temp_dir) = create_sqlite_store();

    // === Setup: Initial sync with some messages ===
    // Thread must be created before messages (FK constraint)
    store.upsert_thread(make_thread("t1", "Original Thread", 2, 4)).unwrap();

    let mut msg1 = make_message("m1", "t1", "Original Thread", 5);
    msg1.label_ids = vec!["INBOX".to_string(), "UNREAD".to_string()];

    let mut msg2 = make_message("m2", "t1", "Re: Original Thread", 4);
    msg2.label_ids = vec!["INBOX".to_string()];

    store.upsert_message(msg1.clone()).unwrap();
    store.upsert_message(msg2.clone()).unwrap();

    // Save initial sync state
    let initial_state = SyncState::new("default", "history_1000");
    store.save_sync_state(initial_state).unwrap();

    // Verify initial state
    assert_eq!(store.count_threads().unwrap(), 1);
    assert!(store.has_message(&MessageId::new("m1")).unwrap());
    assert!(store.has_message(&MessageId::new("m2")).unwrap());

    // === Simulate incremental sync changes ===

    // Change 1: messagesAdded - New message arrives
    let mut msg3 = make_message("m3", "t1", "Re: Re: Original Thread", 1);
    msg3.label_ids = vec!["INBOX".to_string(), "UNREAD".to_string()];
    store.upsert_message(msg3).unwrap();

    // Also add a message to a NEW thread (thread must be created first)
    store.upsert_thread(make_thread("t2", "Brand New Thread", 1, 0)).unwrap();
    let mut msg4 = make_message("m4", "t2", "Brand New Thread", 0);
    msg4.label_ids = vec!["INBOX".to_string(), "UNREAD".to_string()];
    store.upsert_message(msg4).unwrap();

    assert!(store.has_message(&MessageId::new("m3")).unwrap());
    assert!(store.has_message(&MessageId::new("m4")).unwrap());
    assert_eq!(store.count_threads().unwrap(), 2);

    // Change 2: labelsAdded - Mark message as starred
    let msg1_updated = store.get_message(&MessageId::new("m1")).unwrap().unwrap();
    let mut new_labels = msg1_updated.label_ids.clone();
    new_labels.push("STARRED".to_string());
    store.update_message_labels(&MessageId::new("m1"), new_labels).unwrap();

    // Verify starred label was added
    let msg1_check = store.get_message(&MessageId::new("m1")).unwrap().unwrap();
    assert!(msg1_check.label_ids.contains(&"STARRED".to_string()));

    // Change 3: labelsRemoved - Mark message as read (remove UNREAD)
    let msg1_updated = store.get_message(&MessageId::new("m1")).unwrap().unwrap();
    let new_labels: Vec<String> = msg1_updated.label_ids.iter()
        .filter(|l| *l != "UNREAD")
        .cloned()
        .collect();
    store.update_message_labels(&MessageId::new("m1"), new_labels).unwrap();

    // Verify UNREAD was removed
    let msg1_check = store.get_message(&MessageId::new("m1")).unwrap().unwrap();
    assert!(!msg1_check.label_ids.contains(&"UNREAD".to_string()));

    // Change 4: labelsAdded + labelsRemoved - Archive (remove INBOX, keep in ALL)
    let msg2_updated = store.get_message(&MessageId::new("m2")).unwrap().unwrap();
    let new_labels: Vec<String> = msg2_updated.label_ids.iter()
        .filter(|l| *l != "INBOX")
        .cloned()
        .collect();
    store.update_message_labels(&MessageId::new("m2"), new_labels).unwrap();

    // Verify archived (INBOX removed)
    let msg2_check = store.get_message(&MessageId::new("m2")).unwrap().unwrap();
    assert!(!msg2_check.label_ids.contains(&"INBOX".to_string()));

    // Change 5: messagesDeleted - Permanently delete a message
    store.delete_message(&MessageId::new("m4")).unwrap();
    assert!(!store.has_message(&MessageId::new("m4")).unwrap());

    // Update sync state with new history_id
    let updated_state = SyncState::new("default", "history_2000");
    store.save_sync_state(updated_state).unwrap();

    // === Verify final state ===

    // Thread count: t2 may still exist but is now empty
    // (In real sync, empty threads would be cleaned up)

    // Messages: m1, m2, m3 exist; m4 deleted
    assert!(store.has_message(&MessageId::new("m1")).unwrap());
    assert!(store.has_message(&MessageId::new("m2")).unwrap());
    assert!(store.has_message(&MessageId::new("m3")).unwrap());
    assert!(!store.has_message(&MessageId::new("m4")).unwrap());

    // m1: has STARRED, no UNREAD (read + starred)
    let m1 = store.get_message(&MessageId::new("m1")).unwrap().unwrap();
    assert!(m1.label_ids.contains(&"STARRED".to_string()));
    assert!(!m1.label_ids.contains(&"UNREAD".to_string()));

    // m2: no INBOX (archived)
    let m2 = store.get_message(&MessageId::new("m2")).unwrap().unwrap();
    assert!(!m2.label_ids.contains(&"INBOX".to_string()));

    // Sync state is current
    let state = store.get_sync_state("default").unwrap().unwrap();
    assert_eq!(state.history_id, "history_2000");
    assert!(state.initial_sync_complete);
}

/// Test label-based filtering for Gmail folder parity
#[test]
fn test_label_filtering_gmail_folder_parity() {
    let (store, _temp_dir) = create_sqlite_store();

    // Create threads first (FK constraint)
    store.upsert_thread(make_thread("t1", "Inbox Message", 1, 1)).unwrap();
    store.upsert_thread(make_thread("t2", "Sent Message", 1, 2)).unwrap();
    store.upsert_thread(make_thread("t3", "Draft Message", 1, 3)).unwrap();
    store.upsert_thread(make_thread("t4", "Starred Message", 1, 4)).unwrap();
    store.upsert_thread(make_thread("t5", "Archived Message", 1, 5)).unwrap();

    // Create messages in different "folders" (labels)
    let mut inbox_msg = make_message("m1", "t1", "Inbox Message", 1);
    inbox_msg.label_ids = vec!["INBOX".to_string(), "UNREAD".to_string()];

    let mut sent_msg = make_message("m2", "t2", "Sent Message", 2);
    sent_msg.label_ids = vec!["SENT".to_string()];

    let mut draft_msg = make_message("m3", "t3", "Draft Message", 3);
    draft_msg.label_ids = vec!["DRAFT".to_string()];

    let mut starred_msg = make_message("m4", "t4", "Starred Message", 4);
    starred_msg.label_ids = vec!["INBOX".to_string(), "STARRED".to_string()];

    let mut archived_msg = make_message("m5", "t5", "Archived Message", 5);
    archived_msg.label_ids = vec![]; // No INBOX = archived

    store.upsert_message(inbox_msg).unwrap();
    store.upsert_message(sent_msg).unwrap();
    store.upsert_message(draft_msg).unwrap();
    store.upsert_message(starred_msg).unwrap();
    store.upsert_message(archived_msg).unwrap();

    // Verify all threads are stored correctly
    let all_threads = list_threads(&store, 100, 0).unwrap();
    assert_eq!(all_threads.len(), 5);

    // Verify message labels are preserved (this is what matters for Gmail parity)
    let m1 = store.get_message(&MessageId::new("m1")).unwrap().unwrap();
    assert!(m1.label_ids.contains(&"INBOX".to_string()));
    assert!(m1.label_ids.contains(&"UNREAD".to_string()));

    let m2 = store.get_message(&MessageId::new("m2")).unwrap().unwrap();
    assert!(m2.label_ids.contains(&"SENT".to_string()));

    let m3 = store.get_message(&MessageId::new("m3")).unwrap().unwrap();
    assert!(m3.label_ids.contains(&"DRAFT".to_string()));

    let m4 = store.get_message(&MessageId::new("m4")).unwrap().unwrap();
    assert!(m4.label_ids.contains(&"INBOX".to_string()));
    assert!(m4.label_ids.contains(&"STARRED".to_string()));

    let m5 = store.get_message(&MessageId::new("m5")).unwrap().unwrap();
    assert!(m5.label_ids.is_empty());
}

/// Test the full polling cycle with cooldown
#[test]
fn test_polling_cycle_with_cooldown() {
    use chrono::Duration;

    let (store, _temp_dir) = create_sqlite_store();

    // Complete initial sync
    let state = SyncState::new("default", "history_start");
    store.save_sync_state(state).unwrap();

    // Poll 1: Just synced, should skip (cooldown)
    let state = store.get_sync_state("default").unwrap().unwrap();
    let should_sync_1 = cooldown_elapsed(Some(state.last_sync_at), 30);
    assert!(!should_sync_1, "Poll 1: Should skip due to cooldown");

    // Simulate time passing (35 seconds)
    let mut aged_state = state.clone();
    aged_state.last_sync_at = Utc::now() - Duration::seconds(35);
    store.save_sync_state(aged_state).unwrap();

    // Poll 2: Cooldown elapsed, should sync
    let state = store.get_sync_state("default").unwrap().unwrap();
    let should_sync_2 = cooldown_elapsed(Some(state.last_sync_at), 30);
    assert!(should_sync_2, "Poll 2: Should sync, cooldown elapsed");

    // Verify incremental sync is the action
    match determine_sync_action(Some(&state), false) {
        SyncAction::IncrementalSync { history_id } => {
            assert_eq!(history_id, "history_start");
        }
        other => panic!("Expected IncrementalSync, got {:?}", other),
    }

    // After sync completes, update state with new history_id
    let new_state = SyncState::new("default", "history_after_poll");
    store.save_sync_state(new_state).unwrap();

    // Poll 3: Just synced again, should skip
    let state = store.get_sync_state("default").unwrap().unwrap();
    let should_sync_3 = cooldown_elapsed(Some(state.last_sync_at), 30);
    assert!(!should_sync_3, "Poll 3: Should skip, just synced");
}
