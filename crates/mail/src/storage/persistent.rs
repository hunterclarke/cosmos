//! Persistent storage implementation using redb
//!
//! This implementation provides durable storage that persists across
//! application restarts.

use anyhow::{Context, Result};
use redb::{Database, ReadableDatabase, ReadableTable, ReadableTableMetadata, TableDefinition};
use std::path::Path;
use std::sync::Arc;

use super::MailStore;
use crate::models::{Message, MessageId, SyncState, Thread, ThreadId};

// Table definitions
const THREADS: TableDefinition<&str, &[u8]> = TableDefinition::new("threads");
const MESSAGES: TableDefinition<&str, &[u8]> = TableDefinition::new("messages");
const SYNC_STATE: TableDefinition<&str, &[u8]> = TableDefinition::new("sync_state");
// Index: thread_id -> list of message_ids (JSON array)
const THREAD_MESSAGES: TableDefinition<&str, &[u8]> = TableDefinition::new("thread_messages");
// Sorted label index: "{label}\0{inverted_timestamp}\0{thread_id}" -> ()
// Using inverted timestamp (i64::MAX - ts) for descending order
const LABEL_THREAD_INDEX: TableDefinition<&str, ()> = TableDefinition::new("label_thread_index");
// Reverse index: "{thread_id}\0{label}" -> timestamp_millis (as i64 bytes)
const THREAD_LABEL_TS: TableDefinition<&str, &[u8]> = TableDefinition::new("thread_label_ts");

/// Persistent storage implementation using redb
pub struct RedbMailStore {
    db: Arc<Database>,
}

impl RedbMailStore {
    /// Create a new persistent store at the given path
    ///
    /// If the database already exists, opens it without re-initializing tables (faster).
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();

        // Fast path: if database already exists, just open it (skips table init)
        if path.exists() {
            return Self::open(path);
        }

        // Slow path: create new database and initialize tables
        let db = Database::create(path)
            .with_context(|| format!("Failed to create database at {:?}", path))?;

        let store = Self { db: Arc::new(db) };

        // Initialize tables (only needed for new databases)
        store.init_tables()?;

        Ok(store)
    }

    /// Open an existing store (fails if doesn't exist)
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let db = Database::open(path.as_ref())
            .with_context(|| format!("Failed to open database at {:?}", path.as_ref()))?;

        Ok(Self { db: Arc::new(db) })
    }

    /// Initialize tables if they don't exist
    fn init_tables(&self) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            // Create tables by opening them
            let _ = write_txn.open_table(THREADS)?;
            let _ = write_txn.open_table(MESSAGES)?;
            let _ = write_txn.open_table(SYNC_STATE)?;
            let _ = write_txn.open_table(THREAD_MESSAGES)?;
            let _ = write_txn.open_table(LABEL_THREAD_INDEX)?;
            let _ = write_txn.open_table(THREAD_LABEL_TS)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    /// Get the list of message IDs for a thread
    fn get_thread_message_ids(&self, thread_id: &str) -> Result<Vec<String>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(THREAD_MESSAGES)?;

        match table.get(thread_id)? {
            Some(data) => {
                let ids: Vec<String> = serde_json::from_slice(data.value())?;
                Ok(ids)
            }
            None => Ok(Vec::new()),
        }
    }

    /// Add a message ID to a thread's message list
    fn add_message_to_thread(&self, thread_id: &str, message_id: &str) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(THREAD_MESSAGES)?;

            // Get existing IDs
            let mut ids: Vec<String> = match table.get(thread_id)? {
                Some(data) => serde_json::from_slice(data.value())?,
                None => Vec::new(),
            };

            // Add new ID if not already present
            if !ids.contains(&message_id.to_string()) {
                ids.push(message_id.to_string());
                let data = serde_json::to_vec(&ids)?;
                table.insert(thread_id, data.as_slice())?;
            }
        }
        write_txn.commit()?;
        Ok(())
    }

    /// Build sorted index key: "{label}\0{inverted_timestamp}\0{thread_id}"
    fn build_label_index_key(label: &str, timestamp_millis: i64, thread_id: &str) -> String {
        // Invert timestamp for descending order
        let inverted = i64::MAX - timestamp_millis;
        format!("{}\0{:020}\0{}", label, inverted, thread_id)
    }

    /// Build reverse index key: "{thread_id}\0{label}"
    fn build_reverse_index_key(thread_id: &str, label: &str) -> String {
        format!("{}\0{}", thread_id, label)
    }

    /// Update label index for a thread's labels
    fn update_label_index(&self, thread_id: &str, labels: &[String], timestamp_millis: i64) -> Result<()> {
        if labels.is_empty() {
            return Ok(());
        }

        let write_txn = self.db.begin_write()?;
        {
            let mut index_table = write_txn.open_table(LABEL_THREAD_INDEX)?;
            let mut reverse_table = write_txn.open_table(THREAD_LABEL_TS)?;

            for label in labels {
                let reverse_key = Self::build_reverse_index_key(thread_id, label);

                // Check for existing timestamp
                if let Some(old_ts_data) = reverse_table.get(reverse_key.as_str())? {
                    let old_ts = i64::from_be_bytes(old_ts_data.value().try_into().unwrap_or([0; 8]));
                    if old_ts != timestamp_millis {
                        // Remove old index entry
                        let old_key = Self::build_label_index_key(label, old_ts, thread_id);
                        index_table.remove(old_key.as_str())?;
                    }
                }

                // Insert new index entry
                let new_key = Self::build_label_index_key(label, timestamp_millis, thread_id);
                index_table.insert(new_key.as_str(), ())?;

                // Update reverse index
                reverse_table.insert(reverse_key.as_str(), &timestamp_millis.to_be_bytes()[..])?;
            }
        }
        write_txn.commit()?;
        Ok(())
    }
}

impl MailStore for RedbMailStore {
    fn upsert_thread(&self, thread: Thread) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(THREADS)?;
            let data = serde_json::to_vec(&thread)?;
            table.insert(thread.id.as_str(), data.as_slice())?;
        }
        write_txn.commit()?;
        Ok(())
    }

    fn upsert_message(&self, message: Message) -> Result<()> {
        let thread_id = message.thread_id.as_str().to_string();
        let msg_id = message.id.as_str().to_string();
        let labels = message.label_ids.clone();

        // Get thread's last_message_at for index timestamp
        let timestamp_millis = {
            let read_txn = self.db.begin_read()?;
            let threads_table = read_txn.open_table(THREADS)?;
            if let Some(data) = threads_table.get(thread_id.as_str())? {
                let thread: Thread = serde_json::from_slice(data.value())?;
                thread.last_message_at.timestamp_millis()
            } else {
                message.received_at.timestamp_millis()
            }
        };

        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(MESSAGES)?;
            let data = serde_json::to_vec(&message)?;
            table.insert(message.id.as_str(), data.as_slice())?;
        }
        write_txn.commit()?;

        // Link message to thread
        self.add_message_to_thread(&thread_id, &msg_id)?;

        // Update label index
        self.update_label_index(&thread_id, &labels, timestamp_millis)?;

        Ok(())
    }

    fn link_message_to_thread(&self, msg_id: &MessageId, thread_id: &ThreadId) -> Result<()> {
        self.add_message_to_thread(thread_id.as_str(), msg_id.as_str())
    }

    fn get_thread(&self, id: &ThreadId) -> Result<Option<Thread>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(THREADS)?;

        match table.get(id.as_str())? {
            Some(data) => {
                let thread: Thread = serde_json::from_slice(data.value())?;
                Ok(Some(thread))
            }
            None => Ok(None),
        }
    }

    fn get_message(&self, id: &MessageId) -> Result<Option<Message>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(MESSAGES)?;

        match table.get(id.as_str())? {
            Some(data) => {
                let message: Message = serde_json::from_slice(data.value())?;
                Ok(Some(message))
            }
            None => Ok(None),
        }
    }

    fn list_threads(&self, limit: usize, offset: usize) -> Result<Vec<Thread>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(THREADS)?;

        // Collect all threads
        let mut threads: Vec<Thread> = Vec::new();
        for result in table.iter()? {
            let (_, data) = result?;
            let thread: Thread = serde_json::from_slice(data.value())?;
            threads.push(thread);
        }

        // Sort by last_message_at descending
        threads.sort_by(|a, b| b.last_message_at.cmp(&a.last_message_at));

        // Apply pagination
        let result = threads.into_iter().skip(offset).take(limit).collect();

        Ok(result)
    }

    fn list_threads_by_label(
        &self,
        label: &str,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<Thread>> {
        let read_txn = self.db.begin_read()?;
        let index_table = read_txn.open_table(LABEL_THREAD_INDEX)?;
        let threads_table = read_txn.open_table(THREADS)?;

        // Build prefix to scan for this label
        let prefix = format!("{}\0", label);

        // Range scan from prefix, already sorted by inverted timestamp (descending order)
        let mut threads = Vec::new();
        let mut skipped = 0;

        for result in index_table.range(prefix.as_str()..)? {
            let (key, _) = result?;
            let key_str = key.value();

            // Stop if we've passed this label's entries
            if !key_str.starts_with(&prefix) {
                break;
            }

            // Skip offset entries
            if skipped < offset {
                skipped += 1;
                continue;
            }

            // Extract thread_id from key: "{label}\0{inverted_ts}\0{thread_id}"
            let parts: Vec<&str> = key_str.splitn(3, '\0').collect();
            if parts.len() == 3 {
                let thread_id = parts[2];
                if let Some(data) = threads_table.get(thread_id)? {
                    let thread: Thread = serde_json::from_slice(data.value())?;
                    threads.push(thread);
                }
            }

            // Stop once we have enough
            if threads.len() >= limit {
                break;
            }
        }

        Ok(threads)
    }

    fn list_messages_for_thread(&self, thread_id: &ThreadId) -> Result<Vec<Message>> {
        let msg_ids = self.get_thread_message_ids(thread_id.as_str())?;

        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(MESSAGES)?;

        let mut messages: Vec<Message> = Vec::new();
        for msg_id in msg_ids {
            if let Some(data) = table.get(msg_id.as_str())? {
                let message: Message = serde_json::from_slice(data.value())?;
                messages.push(message);
            }
        }

        // Sort by received_at ascending
        messages.sort_by(|a, b| a.received_at.cmp(&b.received_at));

        Ok(messages)
    }

    fn has_message(&self, id: &MessageId) -> Result<bool> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(MESSAGES)?;
        Ok(table.get(id.as_str())?.is_some())
    }

    fn count_threads(&self) -> Result<usize> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(THREADS)?;
        Ok(table.len()? as usize)
    }

    fn count_messages_in_thread(&self, thread_id: &ThreadId) -> Result<usize> {
        let msg_ids = self.get_thread_message_ids(thread_id.as_str())?;
        Ok(msg_ids.len())
    }

    fn clear(&self) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            // Clear all tables by recreating them
            let mut threads = write_txn.open_table(THREADS)?;
            let mut messages = write_txn.open_table(MESSAGES)?;
            let mut sync_state = write_txn.open_table(SYNC_STATE)?;
            let mut thread_messages = write_txn.open_table(THREAD_MESSAGES)?;
            let mut label_index = write_txn.open_table(LABEL_THREAD_INDEX)?;
            let mut thread_label_ts = write_txn.open_table(THREAD_LABEL_TS)?;

            // Drain all entries
            while threads.pop_first()?.is_some() {}
            while messages.pop_first()?.is_some() {}
            while sync_state.pop_first()?.is_some() {}
            while thread_messages.pop_first()?.is_some() {}
            while label_index.pop_first()?.is_some() {}
            while thread_label_ts.pop_first()?.is_some() {}
        }
        write_txn.commit()?;
        Ok(())
    }

    // === Phase 2: Sync State Methods ===

    fn get_sync_state(&self, account_id: &str) -> Result<Option<SyncState>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(SYNC_STATE)?;

        match table.get(account_id)? {
            Some(data) => {
                let state: SyncState = serde_json::from_slice(data.value())?;
                Ok(Some(state))
            }
            None => Ok(None),
        }
    }

    fn save_sync_state(&self, state: SyncState) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(SYNC_STATE)?;
            let data = serde_json::to_vec(&state)?;
            table.insert(state.account_id.as_str(), data.as_slice())?;
        }
        write_txn.commit()?;
        Ok(())
    }

    fn delete_sync_state(&self, account_id: &str) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(SYNC_STATE)?;
            table.remove(account_id)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    fn has_thread(&self, id: &ThreadId) -> Result<bool> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(THREADS)?;
        Ok(table.get(id.as_str())?.is_some())
    }

    fn clear_mail_data(&self) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            // Clear mail data but NOT sync state
            let mut threads = write_txn.open_table(THREADS)?;
            let mut messages = write_txn.open_table(MESSAGES)?;
            let mut thread_messages = write_txn.open_table(THREAD_MESSAGES)?;
            let mut label_index = write_txn.open_table(LABEL_THREAD_INDEX)?;
            let mut thread_label_ts = write_txn.open_table(THREAD_LABEL_TS)?;

            while threads.pop_first()?.is_some() {}
            while messages.pop_first()?.is_some() {}
            while thread_messages.pop_first()?.is_some() {}
            while label_index.pop_first()?.is_some() {}
            while thread_label_ts.pop_first()?.is_some() {}
        }
        write_txn.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::EmailAddress;
    use chrono::Utc;
    use tempfile::tempdir;

    fn make_test_thread(id: &str, subject: &str) -> Thread {
        Thread::new(
            ThreadId::new(id),
            subject.to_string(),
            "Test snippet".to_string(),
            Utc::now(),
            1,
            Some("Test User".to_string()),
            "test@example.com".to_string(),
            false,
        )
    }

    fn make_test_message(id: &str, thread_id: &str) -> Message {
        Message::builder(MessageId::new(id), ThreadId::new(thread_id))
            .from(EmailAddress::new("test@example.com"))
            .subject("Test")
            .body_preview("Test body")
            .build()
    }

    #[test]
    fn test_create_and_open() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.redb");

        // Create new database
        let store = RedbMailStore::new(&path).unwrap();
        drop(store);

        // Open existing database
        let store = RedbMailStore::open(&path).unwrap();
        drop(store);
    }

    #[test]
    fn test_thread_crud() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.redb");
        let store = RedbMailStore::new(&path).unwrap();

        let thread = make_test_thread("t1", "Test Thread");

        // Create
        store.upsert_thread(thread.clone()).unwrap();

        // Read
        let retrieved = store.get_thread(&ThreadId::new("t1")).unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().subject, "Test Thread");

        // Update
        let mut updated = thread.clone();
        updated.subject = "Updated Subject".to_string();
        store.upsert_thread(updated).unwrap();

        let retrieved = store.get_thread(&ThreadId::new("t1")).unwrap().unwrap();
        assert_eq!(retrieved.subject, "Updated Subject");
    }

    #[test]
    fn test_message_crud() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.redb");
        let store = RedbMailStore::new(&path).unwrap();

        let message = make_test_message("m1", "t1");

        // Create
        store.upsert_message(message.clone()).unwrap();

        // Read
        let retrieved = store.get_message(&MessageId::new("m1")).unwrap();
        assert!(retrieved.is_some());

        // Check has_message
        assert!(store.has_message(&MessageId::new("m1")).unwrap());
        assert!(!store.has_message(&MessageId::new("m2")).unwrap());
    }

    #[test]
    fn test_list_threads() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.redb");
        let store = RedbMailStore::new(&path).unwrap();

        for i in 0..5 {
            let mut thread = make_test_thread(&format!("t{}", i), &format!("Thread {}", i));
            thread.last_message_at = Utc::now() - chrono::Duration::hours(5 - i as i64);
            store.upsert_thread(thread).unwrap();
        }

        // List all
        let threads = store.list_threads(10, 0).unwrap();
        assert_eq!(threads.len(), 5);

        // Check order (most recent first)
        assert_eq!(threads[0].id.as_str(), "t4");

        // Test pagination
        let page = store.list_threads(2, 2).unwrap();
        assert_eq!(page.len(), 2);
    }

    #[test]
    fn test_messages_for_thread() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.redb");
        let store = RedbMailStore::new(&path).unwrap();

        // Add messages to different threads
        store.upsert_message(make_test_message("m1", "t1")).unwrap();
        store.upsert_message(make_test_message("m2", "t1")).unwrap();
        store.upsert_message(make_test_message("m3", "t2")).unwrap();

        let messages = store.list_messages_for_thread(&ThreadId::new("t1")).unwrap();
        assert_eq!(messages.len(), 2);
    }

    #[test]
    fn test_sync_state() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.redb");
        let store = RedbMailStore::new(&path).unwrap();

        // Initially no state
        assert!(store.get_sync_state("user@gmail.com").unwrap().is_none());

        // Save state
        let state = SyncState::new("user@gmail.com", "12345");
        store.save_sync_state(state).unwrap();

        // Retrieve
        let retrieved = store.get_sync_state("user@gmail.com").unwrap().unwrap();
        assert_eq!(retrieved.history_id, "12345");

        // Delete
        store.delete_sync_state("user@gmail.com").unwrap();
        assert!(store.get_sync_state("user@gmail.com").unwrap().is_none());
    }

    #[test]
    fn test_clear_mail_data() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.redb");
        let store = RedbMailStore::new(&path).unwrap();

        // Add data
        store.upsert_thread(make_test_thread("t1", "Test")).unwrap();
        store.upsert_message(make_test_message("m1", "t1")).unwrap();
        store
            .save_sync_state(SyncState::new("user@gmail.com", "12345"))
            .unwrap();

        // Clear mail data
        store.clear_mail_data().unwrap();

        // Mail data is gone
        assert_eq!(store.count_threads().unwrap(), 0);
        assert!(!store.has_message(&MessageId::new("m1")).unwrap());

        // Sync state is preserved
        assert!(store.get_sync_state("user@gmail.com").unwrap().is_some());
    }

    #[test]
    fn test_persistence() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.redb");

        // Create and populate
        {
            let store = RedbMailStore::new(&path).unwrap();
            store.upsert_thread(make_test_thread("t1", "Persistent Thread")).unwrap();
            store.upsert_message(make_test_message("m1", "t1")).unwrap();
            store
                .save_sync_state(SyncState::new("user@gmail.com", "12345"))
                .unwrap();
        }

        // Reopen and verify data persisted
        {
            let store = RedbMailStore::open(&path).unwrap();

            let thread = store.get_thread(&ThreadId::new("t1")).unwrap().unwrap();
            assert_eq!(thread.subject, "Persistent Thread");

            let message = store.get_message(&MessageId::new("m1")).unwrap();
            assert!(message.is_some());

            let state = store.get_sync_state("user@gmail.com").unwrap().unwrap();
            assert_eq!(state.history_id, "12345");
        }
    }
}
