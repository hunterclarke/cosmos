//! In-memory storage implementation
//!
//! This implementation is used for testing and as a stub before
//! the real cosmos-storage integration is available.

use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::sync::RwLock;

use super::MailStore;
use crate::models::{Message, MessageId, Thread, ThreadId};

/// In-memory implementation of MailStore
///
/// Uses HashMaps protected by RwLocks for thread-safe access.
/// This is a stub implementation for Phase 1.
pub struct InMemoryMailStore {
    threads: RwLock<HashMap<String, Thread>>,
    messages: RwLock<HashMap<String, Message>>,
    thread_messages: RwLock<HashMap<String, HashSet<String>>>,
}

impl InMemoryMailStore {
    /// Create a new empty in-memory store
    pub fn new() -> Self {
        Self {
            threads: RwLock::new(HashMap::new()),
            messages: RwLock::new(HashMap::new()),
            thread_messages: RwLock::new(HashMap::new()),
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

        let mut messages = self.messages.write().unwrap();
        messages.insert(msg_id.clone(), message);

        // Also link to thread
        let mut thread_messages = self.thread_messages.write().unwrap();
        thread_messages.entry(thread_id).or_default().insert(msg_id);

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
}
