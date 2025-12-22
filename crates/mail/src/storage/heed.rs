//! Persistent storage implementation using heed3 (LMDB)
//!
//! This implementation provides durable storage using LMDB, which offers
//! instant startup times regardless of database size.

use anyhow::{Context, Result};
use heed3::byteorder::BE;
use heed3::types::{Bytes, Str, U64};
use heed3::{Database, Env, EnvOpenOptions};
use std::fs;
use std::path::Path;
use std::sync::Arc;

use super::MailStore;
use crate::models::{Message, MessageId, SyncState, Thread, ThreadId};

/// Default map size: 10 GB (LMDB requires pre-allocated map size)
const DEFAULT_MAP_SIZE: usize = 10 * 1024 * 1024 * 1024;

/// Persistent storage implementation using heed3/LMDB
pub struct HeedMailStore {
    env: Arc<Env>,
    /// threads table: thread_id -> Thread (JSON)
    threads: Database<Str, Bytes>,
    /// messages table: message_id -> Message (JSON)
    messages: Database<Str, Bytes>,
    /// sync_state table: account_id -> SyncState (JSON)
    sync_state: Database<Str, Bytes>,
    /// thread_messages index: thread_id -> Vec<message_id> (JSON)
    thread_messages: Database<Str, Bytes>,
    /// label_thread_index: "{label}\0{inverted_ts}\0{thread_id}" -> ()
    label_thread_index: Database<Str, Bytes>,
    /// thread_label_ts: "{thread_id}\0{label}" -> timestamp_millis
    thread_label_ts: Database<Str, U64<BE>>,
}

impl HeedMailStore {
    /// Create a new persistent store at the given path
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();

        // Ensure directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::create_dir_all(path)?;

        // Open LMDB environment
        let env = unsafe {
            EnvOpenOptions::new()
                .map_size(DEFAULT_MAP_SIZE)
                .max_dbs(10)
                .open(path)
                .with_context(|| format!("Failed to open LMDB environment at {:?}", path))?
        };

        // Create/open all databases
        let mut wtxn = env.write_txn()?;

        let threads = env
            .create_database(&mut wtxn, Some("threads"))?;
        let messages = env
            .create_database(&mut wtxn, Some("messages"))?;
        let sync_state = env
            .create_database(&mut wtxn, Some("sync_state"))?;
        let thread_messages = env
            .create_database(&mut wtxn, Some("thread_messages"))?;
        let label_thread_index = env
            .create_database(&mut wtxn, Some("label_thread_index"))?;
        let thread_label_ts = env
            .create_database(&mut wtxn, Some("thread_label_ts"))?;

        wtxn.commit()?;

        Ok(Self {
            env: Arc::new(env),
            threads,
            messages,
            sync_state,
            thread_messages,
            label_thread_index,
            thread_label_ts,
        })
    }

    /// Get the list of message IDs for a thread
    fn get_thread_message_ids(&self, thread_id: &str) -> Result<Vec<String>> {
        let rtxn = self.env.read_txn()?;
        match self.thread_messages.get(&rtxn, thread_id)? {
            Some(data) => {
                let ids: Vec<String> = serde_json::from_slice(data)?;
                Ok(ids)
            }
            None => Ok(Vec::new()),
        }
    }

    /// Add a message ID to a thread's message list
    fn add_message_to_thread(&self, thread_id: &str, message_id: &str) -> Result<()> {
        let mut wtxn = self.env.write_txn()?;

        // Get existing IDs
        let mut ids: Vec<String> = match self.thread_messages.get(&wtxn, thread_id)? {
            Some(data) => serde_json::from_slice(data)?,
            None => Vec::new(),
        };

        // Add new ID if not already present
        if !ids.contains(&message_id.to_string()) {
            ids.push(message_id.to_string());
            let data = serde_json::to_vec(&ids)?;
            self.thread_messages.put(&mut wtxn, thread_id, &data)?;
        }

        wtxn.commit()?;
        Ok(())
    }

    /// Build sorted index key: "{label}\0{inverted_timestamp}\0{thread_id}"
    fn build_label_index_key(label: &str, timestamp_millis: i64, thread_id: &str) -> String {
        let inverted = i64::MAX - timestamp_millis;
        format!("{}\0{:020}\0{}", label, inverted, thread_id)
    }

    /// Build reverse index key: "{thread_id}\0{label}"
    fn build_reverse_index_key(thread_id: &str, label: &str) -> String {
        format!("{}\0{}", thread_id, label)
    }

    /// Update label index for a thread's labels
    fn update_label_index(
        &self,
        thread_id: &str,
        labels: &[String],
        timestamp_millis: i64,
    ) -> Result<()> {
        if labels.is_empty() {
            return Ok(());
        }

        let mut wtxn = self.env.write_txn()?;

        for label in labels {
            let reverse_key = Self::build_reverse_index_key(thread_id, label);

            // Check for existing timestamp
            if let Some(old_ts) = self.thread_label_ts.get(&wtxn, &reverse_key)? {
                if old_ts != timestamp_millis as u64 {
                    // Remove old index entry
                    let old_key =
                        Self::build_label_index_key(label, old_ts as i64, thread_id);
                    self.label_thread_index.delete(&mut wtxn, &old_key)?;
                }
            }

            // Insert new index entry
            let new_key = Self::build_label_index_key(label, timestamp_millis, thread_id);
            self.label_thread_index.put(&mut wtxn, &new_key, &[])?;

            // Update reverse index
            self.thread_label_ts
                .put(&mut wtxn, &reverse_key, &(timestamp_millis as u64))?;
        }

        wtxn.commit()?;
        Ok(())
    }
}

impl MailStore for HeedMailStore {
    fn upsert_thread(&self, thread: Thread) -> Result<()> {
        let mut wtxn = self.env.write_txn()?;
        let data = serde_json::to_vec(&thread)?;
        self.threads.put(&mut wtxn, thread.id.as_str(), &data)?;
        wtxn.commit()?;
        Ok(())
    }

    fn upsert_message(&self, message: Message) -> Result<()> {
        let thread_id = message.thread_id.as_str().to_string();
        let msg_id = message.id.as_str().to_string();
        let labels = message.label_ids.clone();

        // Get thread's last_message_at for index timestamp
        let timestamp_millis = {
            let rtxn = self.env.read_txn()?;
            if let Some(data) = self.threads.get(&rtxn, &thread_id)? {
                let thread: Thread = serde_json::from_slice(data)?;
                thread.last_message_at.timestamp_millis()
            } else {
                message.received_at.timestamp_millis()
            }
        };

        let mut wtxn = self.env.write_txn()?;
        let data = serde_json::to_vec(&message)?;
        self.messages.put(&mut wtxn, message.id.as_str(), &data)?;
        wtxn.commit()?;

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
        let rtxn = self.env.read_txn()?;
        match self.threads.get(&rtxn, id.as_str())? {
            Some(data) => {
                let thread: Thread = serde_json::from_slice(data)?;
                Ok(Some(thread))
            }
            None => Ok(None),
        }
    }

    fn get_message(&self, id: &MessageId) -> Result<Option<Message>> {
        let rtxn = self.env.read_txn()?;
        match self.messages.get(&rtxn, id.as_str())? {
            Some(data) => {
                let message: Message = serde_json::from_slice(data)?;
                Ok(Some(message))
            }
            None => Ok(None),
        }
    }

    fn list_threads(&self, limit: usize, offset: usize) -> Result<Vec<Thread>> {
        let rtxn = self.env.read_txn()?;

        // Collect all threads
        let mut threads: Vec<Thread> = Vec::new();
        for result in self.threads.iter(&rtxn)? {
            let (_, data) = result?;
            let thread: Thread = serde_json::from_slice(data)?;
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
        let rtxn = self.env.read_txn()?;

        // Build prefix to scan for this label
        let prefix = format!("{}\0", label);

        // Range scan from prefix, already sorted by inverted timestamp (descending order)
        let mut threads = Vec::new();
        let mut skipped = 0;

        for result in self.label_thread_index.iter(&rtxn)? {
            let (key, _) = result?;

            // Stop if we've passed this label's entries
            if !key.starts_with(&prefix) {
                if key > prefix.as_str() {
                    break;
                }
                continue;
            }

            // Skip offset entries
            if skipped < offset {
                skipped += 1;
                continue;
            }

            // Extract thread_id from key: "{label}\0{inverted_ts}\0{thread_id}"
            let parts: Vec<&str> = key.splitn(3, '\0').collect();
            if parts.len() == 3 {
                let thread_id = parts[2];
                if let Some(data) = self.threads.get(&rtxn, thread_id)? {
                    let thread: Thread = serde_json::from_slice(data)?;
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

        let rtxn = self.env.read_txn()?;

        let mut messages: Vec<Message> = Vec::new();
        for msg_id in msg_ids {
            if let Some(data) = self.messages.get(&rtxn, &msg_id)? {
                let message: Message = serde_json::from_slice(data)?;
                messages.push(message);
            }
        }

        // Sort by received_at ascending
        messages.sort_by(|a, b| a.received_at.cmp(&b.received_at));

        Ok(messages)
    }

    fn has_message(&self, id: &MessageId) -> Result<bool> {
        let rtxn = self.env.read_txn()?;
        Ok(self.messages.get(&rtxn, id.as_str())?.is_some())
    }

    fn count_threads(&self) -> Result<usize> {
        let rtxn = self.env.read_txn()?;
        Ok(self.threads.len(&rtxn)? as usize)
    }

    fn count_messages_in_thread(&self, thread_id: &ThreadId) -> Result<usize> {
        let msg_ids = self.get_thread_message_ids(thread_id.as_str())?;
        Ok(msg_ids.len())
    }

    fn clear(&self) -> Result<()> {
        let mut wtxn = self.env.write_txn()?;
        self.threads.clear(&mut wtxn)?;
        self.messages.clear(&mut wtxn)?;
        self.sync_state.clear(&mut wtxn)?;
        self.thread_messages.clear(&mut wtxn)?;
        self.label_thread_index.clear(&mut wtxn)?;
        self.thread_label_ts.clear(&mut wtxn)?;
        wtxn.commit()?;
        Ok(())
    }

    fn get_sync_state(&self, account_id: &str) -> Result<Option<SyncState>> {
        let rtxn = self.env.read_txn()?;
        match self.sync_state.get(&rtxn, account_id)? {
            Some(data) => {
                let state: SyncState = serde_json::from_slice(data)?;
                Ok(Some(state))
            }
            None => Ok(None),
        }
    }

    fn save_sync_state(&self, state: SyncState) -> Result<()> {
        let mut wtxn = self.env.write_txn()?;
        let data = serde_json::to_vec(&state)?;
        self.sync_state.put(&mut wtxn, &state.account_id, &data)?;
        wtxn.commit()?;
        Ok(())
    }

    fn delete_sync_state(&self, account_id: &str) -> Result<()> {
        let mut wtxn = self.env.write_txn()?;
        self.sync_state.delete(&mut wtxn, account_id)?;
        wtxn.commit()?;
        Ok(())
    }

    fn has_thread(&self, id: &ThreadId) -> Result<bool> {
        let rtxn = self.env.read_txn()?;
        Ok(self.threads.get(&rtxn, id.as_str())?.is_some())
    }

    fn clear_mail_data(&self) -> Result<()> {
        let mut wtxn = self.env.write_txn()?;
        // Clear mail data but NOT sync state
        self.threads.clear(&mut wtxn)?;
        self.messages.clear(&mut wtxn)?;
        self.thread_messages.clear(&mut wtxn)?;
        self.label_thread_index.clear(&mut wtxn)?;
        self.thread_label_ts.clear(&mut wtxn)?;
        wtxn.commit()?;
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
    fn test_create_and_reopen() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.lmdb");

        // Create new database
        let store = HeedMailStore::new(&path).unwrap();
        store
            .upsert_thread(make_test_thread("t1", "Test"))
            .unwrap();
        drop(store);

        // Reopen and verify data persisted
        let store = HeedMailStore::new(&path).unwrap();
        let thread = store.get_thread(&ThreadId::new("t1")).unwrap();
        assert!(thread.is_some());
        assert_eq!(thread.unwrap().subject, "Test");
    }

    #[test]
    fn test_thread_crud() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.lmdb");
        let store = HeedMailStore::new(&path).unwrap();

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
        let path = dir.path().join("test.lmdb");
        let store = HeedMailStore::new(&path).unwrap();

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
    fn test_sync_state() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.lmdb");
        let store = HeedMailStore::new(&path).unwrap();

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
}
