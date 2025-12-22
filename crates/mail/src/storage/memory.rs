//! In-memory storage implementation
//!
//! This implementation is used for testing and as a stub before
//! the real cosmos-storage integration is available.

use anyhow::Result;
use std::cmp::Reverse;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::RwLock;

use super::{MailStore, PendingMessage};
use crate::models::{Message, MessageId, SyncState, Thread, ThreadId};

/// In-memory implementation of MailStore
///
/// Uses HashMaps protected by RwLocks for thread-safe access.
/// This is a stub implementation for Phase 1, extended for Phase 2.
/// Internal storage for pending messages
struct PendingMessageData {
    data: Vec<u8>,
    label_ids: Vec<String>,
}

pub struct InMemoryMailStore {
    threads: RwLock<HashMap<String, Thread>>,
    messages: RwLock<HashMap<String, Message>>,
    thread_messages: RwLock<HashMap<String, HashSet<String>>>,
    /// Sync state per account (Phase 2)
    sync_states: RwLock<HashMap<String, SyncState>>,
    /// Sorted index: label -> set of (Reverse<timestamp_millis>, thread_id)
    /// Using Reverse for descending order (newest first)
    label_thread_index: RwLock<HashMap<String, BTreeSet<(Reverse<i64>, String)>>>,
    /// Reverse index: (thread_id, label) -> timestamp_millis
    /// Used to find and remove old entries when timestamp changes
    thread_label_ts: RwLock<HashMap<(String, String), i64>>,
    /// Pending messages for deferred processing (Phase 4)
    pending_messages: RwLock<HashMap<String, PendingMessageData>>,
}

impl InMemoryMailStore {
    /// Create a new empty in-memory store
    pub fn new() -> Self {
        Self {
            threads: RwLock::new(HashMap::new()),
            messages: RwLock::new(HashMap::new()),
            thread_messages: RwLock::new(HashMap::new()),
            sync_states: RwLock::new(HashMap::new()),
            label_thread_index: RwLock::new(HashMap::new()),
            thread_label_ts: RwLock::new(HashMap::new()),
            pending_messages: RwLock::new(HashMap::new()),
        }
    }

    /// Update the label index for a thread
    fn update_label_index(&self, thread_id: &str, labels: &[String], timestamp_millis: i64) {
        let mut index = self.label_thread_index.write().unwrap();
        let mut reverse = self.thread_label_ts.write().unwrap();

        for label in labels {
            let key = (thread_id.to_string(), label.clone());

            // Check if we have an existing entry with a different timestamp
            if let Some(&old_ts) = reverse.get(&key) {
                if old_ts != timestamp_millis {
                    // Remove old entry from sorted index
                    if let Some(set) = index.get_mut(label) {
                        set.remove(&(Reverse(old_ts), thread_id.to_string()));
                    }
                }
            }

            // Insert new entry
            index
                .entry(label.clone())
                .or_default()
                .insert((Reverse(timestamp_millis), thread_id.to_string()));
            reverse.insert(key, timestamp_millis);
        }
    }
}

impl Default for InMemoryMailStore {
    fn default() -> Self {
        Self::new()
    }
}

impl MailStore for InMemoryMailStore {
    fn upsert_thread(&self, thread: Thread) -> Result<()> {
        let mut threads = self.threads.write().unwrap();
        threads.insert(thread.id.0.clone(), thread);
        Ok(())
    }

    fn upsert_message(&self, message: Message) -> Result<()> {
        let thread_id = message.thread_id.0.clone();
        let msg_id = message.id.0.clone();
        let labels = message.label_ids.clone();

        // Get thread's last_message_at for index timestamp
        let timestamp_millis = {
            let threads = self.threads.read().unwrap();
            threads
                .get(&thread_id)
                .map(|t| t.last_message_at.timestamp_millis())
                .unwrap_or_else(|| message.received_at.timestamp_millis())
        };

        let mut messages = self.messages.write().unwrap();
        messages.insert(msg_id.clone(), message);

        // Also link to thread
        let mut thread_messages = self.thread_messages.write().unwrap();
        thread_messages.entry(thread_id.clone()).or_default().insert(msg_id);

        drop(messages);
        drop(thread_messages);

        // Update label index
        if !labels.is_empty() {
            self.update_label_index(&thread_id, &labels, timestamp_millis);
        }

        Ok(())
    }

    fn link_message_to_thread(&self, msg_id: &MessageId, thread_id: &ThreadId) -> Result<()> {
        let mut thread_messages = self.thread_messages.write().unwrap();
        thread_messages
            .entry(thread_id.0.clone())
            .or_default()
            .insert(msg_id.0.clone());
        Ok(())
    }

    fn get_thread(&self, id: &ThreadId) -> Result<Option<Thread>> {
        let threads = self.threads.read().unwrap();
        Ok(threads.get(&id.0).cloned())
    }

    fn get_message(&self, id: &MessageId) -> Result<Option<Message>> {
        let messages = self.messages.read().unwrap();
        Ok(messages.get(&id.0).cloned())
    }

    fn list_threads(&self, limit: usize, offset: usize) -> Result<Vec<Thread>> {
        let threads = self.threads.read().unwrap();
        let mut thread_list: Vec<_> = threads.values().cloned().collect();

        // Sort by last_message_at descending
        thread_list.sort_by(|a, b| b.last_message_at.cmp(&a.last_message_at));

        // Apply pagination
        let result = thread_list.into_iter().skip(offset).take(limit).collect();

        Ok(result)
    }

    fn list_threads_by_label(
        &self,
        label: &str,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<Thread>> {
        let index = self.label_thread_index.read().unwrap();
        let threads = self.threads.read().unwrap();

        let Some(label_set) = index.get(label) else {
            return Ok(Vec::new());
        };

        // BTreeSet is already sorted by (Reverse<timestamp>, thread_id)
        // so we can just iterate, skip offset, take limit
        let result: Vec<Thread> = label_set
            .iter()
            .skip(offset)
            .take(limit)
            .filter_map(|(_, thread_id)| threads.get(thread_id).cloned())
            .collect();

        Ok(result)
    }

    fn list_messages_for_thread(&self, thread_id: &ThreadId) -> Result<Vec<Message>> {
        let thread_messages = self.thread_messages.read().unwrap();
        let messages = self.messages.read().unwrap();

        let mut result = Vec::new();

        if let Some(msg_ids) = thread_messages.get(&thread_id.0) {
            for msg_id in msg_ids {
                if let Some(msg) = messages.get(msg_id) {
                    result.push(msg.clone());
                }
            }
        }

        // Sort by received_at ascending
        result.sort_by(|a, b| a.received_at.cmp(&b.received_at));

        Ok(result)
    }

    fn has_message(&self, id: &MessageId) -> Result<bool> {
        let messages = self.messages.read().unwrap();
        Ok(messages.contains_key(&id.0))
    }

    fn count_threads(&self) -> Result<usize> {
        let threads = self.threads.read().unwrap();
        Ok(threads.len())
    }

    fn count_messages_in_thread(&self, thread_id: &ThreadId) -> Result<usize> {
        let thread_messages = self.thread_messages.read().unwrap();
        Ok(thread_messages
            .get(&thread_id.0)
            .map(|s| s.len())
            .unwrap_or(0))
    }

    fn clear(&self) -> Result<()> {
        self.threads.write().unwrap().clear();
        self.messages.write().unwrap().clear();
        self.thread_messages.write().unwrap().clear();
        self.sync_states.write().unwrap().clear();
        self.label_thread_index.write().unwrap().clear();
        self.thread_label_ts.write().unwrap().clear();
        self.pending_messages.write().unwrap().clear();
        Ok(())
    }

    // === Phase 2: Sync State Methods ===

    fn get_sync_state(&self, account_id: &str) -> Result<Option<SyncState>> {
        let states = self.sync_states.read().unwrap();
        Ok(states.get(account_id).cloned())
    }

    fn save_sync_state(&self, state: SyncState) -> Result<()> {
        let mut states = self.sync_states.write().unwrap();
        states.insert(state.account_id.clone(), state);
        Ok(())
    }

    fn delete_sync_state(&self, account_id: &str) -> Result<()> {
        let mut states = self.sync_states.write().unwrap();
        states.remove(account_id);
        Ok(())
    }

    fn has_thread(&self, id: &ThreadId) -> Result<bool> {
        let threads = self.threads.read().unwrap();
        Ok(threads.contains_key(&id.0))
    }

    fn clear_mail_data(&self) -> Result<()> {
        self.threads.write().unwrap().clear();
        self.messages.write().unwrap().clear();
        self.thread_messages.write().unwrap().clear();
        self.label_thread_index.write().unwrap().clear();
        self.thread_label_ts.write().unwrap().clear();
        // Note: sync_states is NOT cleared
        Ok(())
    }

    // === Phase 3: Mutation Support Methods ===

    fn get_message_ids_for_thread(&self, thread_id: &ThreadId) -> Result<Vec<MessageId>> {
        let thread_messages = self.thread_messages.read().unwrap();
        let ids = thread_messages
            .get(&thread_id.0)
            .map(|set| set.iter().map(|s| MessageId::new(s)).collect())
            .unwrap_or_default();
        Ok(ids)
    }

    fn update_message_labels(&self, message_id: &MessageId, label_ids: Vec<String>) -> Result<()> {
        let mut messages = self.messages.write().unwrap();

        if let Some(message) = messages.get_mut(&message_id.0) {
            let old_labels = message.label_ids.clone();
            let was_unread = old_labels.contains(&"UNREAD".to_string());
            let is_unread = label_ids.contains(&"UNREAD".to_string());

            // Update message labels
            message.label_ids = label_ids.clone();

            // Get thread ID before dropping borrow
            let thread_id = message.thread_id.0.clone();
            let timestamp = message.received_at.timestamp_millis();

            drop(messages);

            // Update label index - remove old labels, add new ones
            {
                let mut index = self.label_thread_index.write().unwrap();
                let mut reverse = self.thread_label_ts.write().unwrap();

                // Remove entries for old labels that are no longer present
                for label in &old_labels {
                    if !label_ids.contains(label) {
                        let key = (thread_id.clone(), label.clone());
                        if let Some(&old_ts) = reverse.get(&key) {
                            if let Some(set) = index.get_mut(label) {
                                set.remove(&(Reverse(old_ts), thread_id.clone()));
                            }
                        }
                        reverse.remove(&key);
                    }
                }

                // Add entries for new labels
                for label in &label_ids {
                    if !old_labels.contains(label) {
                        index
                            .entry(label.clone())
                            .or_default()
                            .insert((Reverse(timestamp), thread_id.clone()));
                        reverse.insert((thread_id.clone(), label.clone()), timestamp);
                    }
                }
            }

            // Update thread is_unread flag if UNREAD status changed
            if was_unread != is_unread {
                let mut threads = self.threads.write().unwrap();
                if let Some(thread) = threads.get_mut(&thread_id) {
                    // Check if any message in thread is still unread
                    let messages = self.messages.read().unwrap();
                    let thread_messages = self.thread_messages.read().unwrap();

                    let any_unread = thread_messages
                        .get(&thread_id)
                        .map(|msg_ids| {
                            msg_ids.iter().any(|id| {
                                messages
                                    .get(id)
                                    .map(|m| m.label_ids.contains(&"UNREAD".to_string()))
                                    .unwrap_or(false)
                            })
                        })
                        .unwrap_or(false);

                    thread.is_unread = any_unread;
                }
            }
        }

        Ok(())
    }

    fn delete_message(&self, message_id: &MessageId) -> Result<()> {
        let mut messages = self.messages.write().unwrap();

        // Get the message to find its thread and labels
        let message = match messages.remove(&message_id.0) {
            Some(m) => m,
            None => return Ok(()), // Already deleted, nothing to do
        };

        let thread_id = message.thread_id.0.clone();

        // Remove from thread_messages index
        {
            let mut thread_messages = self.thread_messages.write().unwrap();
            if let Some(set) = thread_messages.get_mut(&thread_id) {
                set.remove(&message_id.0);
            }
        }

        // Remove from label index
        {
            let mut index = self.label_thread_index.write().unwrap();
            let mut reverse = self.thread_label_ts.write().unwrap();

            for label in &message.label_ids {
                let key = (thread_id.clone(), label.clone());
                if let Some(&ts) = reverse.get(&key) {
                    if let Some(set) = index.get_mut(label) {
                        set.remove(&(Reverse(ts), thread_id.clone()));
                    }
                }
                reverse.remove(&key);
            }
        }

        drop(messages);

        // Update thread message count, or delete thread if empty
        let mut threads = self.threads.write().unwrap();
        let thread_messages = self.thread_messages.read().unwrap();

        let remaining_count = thread_messages
            .get(&thread_id)
            .map(|s| s.len())
            .unwrap_or(0);

        if remaining_count == 0 {
            // Delete the thread entirely
            threads.remove(&thread_id);
        } else if let Some(thread) = threads.get_mut(&thread_id) {
            // Update message count
            thread.message_count = remaining_count;
        }

        Ok(())
    }

    // === Phase 4: Pending Message Queue ===

    fn store_pending_message(
        &self,
        id: &MessageId,
        data: &[u8],
        label_ids: Vec<String>,
    ) -> Result<()> {
        let pending_data = PendingMessageData {
            data: data.to_vec(),
            label_ids,
        };
        self.pending_messages
            .write()
            .unwrap()
            .insert(id.0.clone(), pending_data);
        Ok(())
    }

    fn has_pending_message(&self, id: &MessageId) -> Result<bool> {
        Ok(self.pending_messages.read().unwrap().contains_key(&id.0))
    }

    fn get_pending_messages(
        &self,
        label: Option<&str>,
        limit: usize,
    ) -> Result<Vec<PendingMessage>> {
        let pending = self.pending_messages.read().unwrap();

        let mut inbox_messages = Vec::new();
        let mut other_messages = Vec::new();

        for (id, data) in pending.iter() {
            let msg = PendingMessage {
                id: MessageId::new(id),
                data: data.data.clone(),
                label_ids: data.label_ids.clone(),
            };

            if let Some(filter_label) = label {
                if data.label_ids.iter().any(|l| l == filter_label) {
                    inbox_messages.push(msg);
                }
            } else {
                if data.label_ids.iter().any(|l| l == "INBOX") {
                    inbox_messages.push(msg);
                } else {
                    other_messages.push(msg);
                }
            }

            if label.is_some() && inbox_messages.len() >= limit {
                break;
            }
            if label.is_none() && inbox_messages.len() + other_messages.len() >= limit {
                break;
            }
        }

        if label.is_some() {
            inbox_messages.truncate(limit);
            Ok(inbox_messages)
        } else {
            inbox_messages.extend(other_messages);
            inbox_messages.truncate(limit);
            Ok(inbox_messages)
        }
    }

    fn delete_pending_message(&self, id: &MessageId) -> Result<()> {
        self.pending_messages.write().unwrap().remove(&id.0);
        Ok(())
    }

    fn count_pending_messages(&self, label: Option<&str>) -> Result<usize> {
        let pending = self.pending_messages.read().unwrap();

        if label.is_none() {
            return Ok(pending.len());
        }

        let count = pending
            .values()
            .filter(|data| data.label_ids.iter().any(|l| l == label.unwrap()))
            .count();

        Ok(count)
    }

    fn clear_pending_messages(&self) -> Result<()> {
        self.pending_messages.write().unwrap().clear();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::EmailAddress;
    use chrono::Utc;

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
    fn test_upsert_and_get_thread() {
        let store = InMemoryMailStore::new();
        let thread = make_test_thread("t1", "Test Thread");

        store.upsert_thread(thread.clone()).unwrap();
        let retrieved = store.get_thread(&ThreadId::new("t1")).unwrap();

        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().subject, "Test Thread");
    }

    #[test]
    fn test_upsert_and_get_message() {
        let store = InMemoryMailStore::new();
        let message = make_test_message("m1", "t1");

        store.upsert_message(message.clone()).unwrap();
        let retrieved = store.get_message(&MessageId::new("m1")).unwrap();

        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().id.0, "m1");
    }

    #[test]
    fn test_list_threads_sorted() {
        let store = InMemoryMailStore::new();

        let mut t1 = make_test_thread("t1", "Thread 1");
        t1.last_message_at = Utc::now() - chrono::Duration::hours(2);

        let mut t2 = make_test_thread("t2", "Thread 2");
        t2.last_message_at = Utc::now() - chrono::Duration::hours(1);

        let mut t3 = make_test_thread("t3", "Thread 3");
        t3.last_message_at = Utc::now();

        store.upsert_thread(t1).unwrap();
        store.upsert_thread(t2).unwrap();
        store.upsert_thread(t3).unwrap();

        let threads = store.list_threads(10, 0).unwrap();
        assert_eq!(threads.len(), 3);
        assert_eq!(threads[0].id.0, "t3"); // Most recent first
        assert_eq!(threads[1].id.0, "t2");
        assert_eq!(threads[2].id.0, "t1");
    }

    #[test]
    fn test_list_threads_pagination() {
        let store = InMemoryMailStore::new();

        for i in 0..5 {
            let thread = make_test_thread(&format!("t{}", i), &format!("Thread {}", i));
            store.upsert_thread(thread).unwrap();
        }

        let page1 = store.list_threads(2, 0).unwrap();
        assert_eq!(page1.len(), 2);

        let page2 = store.list_threads(2, 2).unwrap();
        assert_eq!(page2.len(), 2);

        let page3 = store.list_threads(2, 4).unwrap();
        assert_eq!(page3.len(), 1);
    }

    #[test]
    fn test_list_messages_for_thread() {
        let store = InMemoryMailStore::new();

        let m1 = make_test_message("m1", "t1");
        let m2 = make_test_message("m2", "t1");
        let m3 = make_test_message("m3", "t2");

        store.upsert_message(m1).unwrap();
        store.upsert_message(m2).unwrap();
        store.upsert_message(m3).unwrap();

        let messages = store
            .list_messages_for_thread(&ThreadId::new("t1"))
            .unwrap();
        assert_eq!(messages.len(), 2);
    }

    #[test]
    fn test_has_message() {
        let store = InMemoryMailStore::new();
        let message = make_test_message("m1", "t1");

        assert!(!store.has_message(&MessageId::new("m1")).unwrap());
        store.upsert_message(message).unwrap();
        assert!(store.has_message(&MessageId::new("m1")).unwrap());
    }

    #[test]
    fn test_clear() {
        let store = InMemoryMailStore::new();

        store.upsert_thread(make_test_thread("t1", "Test")).unwrap();
        store.upsert_message(make_test_message("m1", "t1")).unwrap();

        assert_eq!(store.count_threads().unwrap(), 1);

        store.clear().unwrap();

        assert_eq!(store.count_threads().unwrap(), 0);
    }

    // === Phase 2: Sync State Tests ===

    #[test]
    fn test_sync_state_crud() {
        let store = InMemoryMailStore::new();

        // Initially no sync state
        assert!(store.get_sync_state("user@gmail.com").unwrap().is_none());

        // Save sync state
        let state = SyncState::new("user@gmail.com", "12345");
        store.save_sync_state(state).unwrap();

        // Retrieve it
        let retrieved = store.get_sync_state("user@gmail.com").unwrap();
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.account_id, "user@gmail.com");
        assert_eq!(retrieved.history_id, "12345");

        // Update it
        let updated = SyncState::new("user@gmail.com", "67890");
        store.save_sync_state(updated).unwrap();

        let retrieved = store.get_sync_state("user@gmail.com").unwrap().unwrap();
        assert_eq!(retrieved.history_id, "67890");

        // Delete it
        store.delete_sync_state("user@gmail.com").unwrap();
        assert!(store.get_sync_state("user@gmail.com").unwrap().is_none());
    }

    #[test]
    fn test_has_thread() {
        let store = InMemoryMailStore::new();

        assert!(!store.has_thread(&ThreadId::new("t1")).unwrap());

        store.upsert_thread(make_test_thread("t1", "Test")).unwrap();

        assert!(store.has_thread(&ThreadId::new("t1")).unwrap());
    }

    #[test]
    fn test_clear_mail_data_preserves_sync_state() {
        let store = InMemoryMailStore::new();

        // Add mail data and sync state
        store.upsert_thread(make_test_thread("t1", "Test")).unwrap();
        store.upsert_message(make_test_message("m1", "t1")).unwrap();
        store
            .save_sync_state(SyncState::new("user@gmail.com", "12345"))
            .unwrap();

        assert_eq!(store.count_threads().unwrap(), 1);
        assert!(store.get_sync_state("user@gmail.com").unwrap().is_some());

        // Clear mail data only
        store.clear_mail_data().unwrap();

        // Mail data is gone
        assert_eq!(store.count_threads().unwrap(), 0);
        assert!(!store.has_message(&MessageId::new("m1")).unwrap());

        // But sync state is preserved
        assert!(store.get_sync_state("user@gmail.com").unwrap().is_some());
    }
}
