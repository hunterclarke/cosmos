//! Action handler for email operations
//!
//! Coordinates between Gmail API and local storage for mutations.

use anyhow::Result;
use log::info;
use std::sync::Arc;

use crate::gmail::GmailClient;
use crate::models::ThreadId;
use crate::storage::MailStore;

/// Label IDs used by Gmail for common states
pub mod labels {
    pub const INBOX: &str = "INBOX";
    pub const UNREAD: &str = "UNREAD";
    pub const STARRED: &str = "STARRED";
    pub const TRASH: &str = "TRASH";
    #[allow(dead_code)] // May be used in future spam handling
    pub const SPAM: &str = "SPAM";
}

/// Handler for email actions like archive, star, read/unread
///
/// Actions are performed in two steps:
/// 1. Call Gmail API to update server state
/// 2. Update local storage to reflect the change
///
/// This ensures the server is the source of truth, and local state
/// is kept in sync.
pub struct ActionHandler {
    gmail: Arc<GmailClient>,
    store: Arc<dyn MailStore>,
}

impl ActionHandler {
    /// Create a new action handler
    pub fn new(gmail: Arc<GmailClient>, store: Arc<dyn MailStore>) -> Self {
        Self { gmail, store }
    }

    /// Archive a thread (remove from INBOX)
    ///
    /// This removes the INBOX label from all messages in the thread,
    /// which is how Gmail's archive works.
    pub fn archive_thread(&self, thread_id: &ThreadId) -> Result<()> {
        let msg_ids = self.store.get_message_ids_for_thread(thread_id)?;
        if msg_ids.is_empty() {
            return Ok(());
        }

        info!("Archiving thread {} ({} messages)", thread_id.as_str(), msg_ids.len());

        // Use batch modify for efficiency
        let id_strs: Vec<&str> = msg_ids.iter().map(|id| id.as_str()).collect();
        self.gmail.batch_modify_messages(&id_strs, &[], &[labels::INBOX])?;

        // Update local storage
        for msg_id in &msg_ids {
            if let Some(msg) = self.store.get_message(msg_id)? {
                let mut new_labels = msg.label_ids.clone();
                new_labels.retain(|l| l != labels::INBOX);
                self.store.update_message_labels(msg_id, new_labels)?;
            }
        }

        info!("Archived thread {}", thread_id.as_str());
        Ok(())
    }

    /// Unarchive a thread (add back to INBOX)
    pub fn unarchive_thread(&self, thread_id: &ThreadId) -> Result<()> {
        let msg_ids = self.store.get_message_ids_for_thread(thread_id)?;
        if msg_ids.is_empty() {
            return Ok(());
        }

        info!("Unarchiving thread {} ({} messages)", thread_id.as_str(), msg_ids.len());

        let id_strs: Vec<&str> = msg_ids.iter().map(|id| id.as_str()).collect();
        self.gmail.batch_modify_messages(&id_strs, &[labels::INBOX], &[])?;

        // Update local storage
        for msg_id in &msg_ids {
            if let Some(msg) = self.store.get_message(msg_id)? {
                let mut new_labels = msg.label_ids.clone();
                if !new_labels.contains(&labels::INBOX.to_string()) {
                    new_labels.push(labels::INBOX.to_string());
                }
                self.store.update_message_labels(msg_id, new_labels)?;
            }
        }

        info!("Unarchived thread {}", thread_id.as_str());
        Ok(())
    }

    /// Toggle star status for a thread
    ///
    /// Stars/unstars all messages in the thread.
    /// Returns the new starred state (true = starred, false = unstarred).
    pub fn toggle_star(&self, thread_id: &ThreadId) -> Result<bool> {
        let msg_ids = self.store.get_message_ids_for_thread(thread_id)?;
        if msg_ids.is_empty() {
            return Ok(false);
        }

        // Check if any message is currently starred
        let is_starred = msg_ids.iter().any(|id| {
            self.store
                .get_message(id)
                .ok()
                .flatten()
                .map(|m| m.label_ids.contains(&labels::STARRED.to_string()))
                .unwrap_or(false)
        });

        let new_starred = !is_starred;
        info!(
            "Toggling star for thread {} to {}",
            thread_id.as_str(),
            if new_starred { "starred" } else { "unstarred" }
        );

        let id_strs: Vec<&str> = msg_ids.iter().map(|id| id.as_str()).collect();
        if new_starred {
            self.gmail.batch_modify_messages(&id_strs, &[labels::STARRED], &[])?;
        } else {
            self.gmail.batch_modify_messages(&id_strs, &[], &[labels::STARRED])?;
        }

        // Update local storage
        for msg_id in &msg_ids {
            if let Some(msg) = self.store.get_message(msg_id)? {
                let mut new_labels = msg.label_ids.clone();
                if new_starred {
                    if !new_labels.contains(&labels::STARRED.to_string()) {
                        new_labels.push(labels::STARRED.to_string());
                    }
                } else {
                    new_labels.retain(|l| l != labels::STARRED);
                }
                self.store.update_message_labels(msg_id, new_labels)?;
            }
        }

        Ok(new_starred)
    }

    /// Set the read status for a thread
    ///
    /// Marks all messages in the thread as read or unread.
    pub fn set_read(&self, thread_id: &ThreadId, is_read: bool) -> Result<()> {
        let msg_ids = self.store.get_message_ids_for_thread(thread_id)?;
        if msg_ids.is_empty() {
            return Ok(());
        }

        info!(
            "Marking thread {} as {}",
            thread_id.as_str(),
            if is_read { "read" } else { "unread" }
        );

        let id_strs: Vec<&str> = msg_ids.iter().map(|id| id.as_str()).collect();
        if is_read {
            // Remove UNREAD label to mark as read
            self.gmail.batch_modify_messages(&id_strs, &[], &[labels::UNREAD])?;
        } else {
            // Add UNREAD label to mark as unread
            self.gmail.batch_modify_messages(&id_strs, &[labels::UNREAD], &[])?;
        }

        // Update local storage
        for msg_id in &msg_ids {
            if let Some(msg) = self.store.get_message(msg_id)? {
                let mut new_labels = msg.label_ids.clone();
                if is_read {
                    new_labels.retain(|l| l != labels::UNREAD);
                } else if !new_labels.contains(&labels::UNREAD.to_string()) {
                    new_labels.push(labels::UNREAD.to_string());
                }
                self.store.update_message_labels(msg_id, new_labels)?;
            }
        }

        Ok(())
    }

    /// Toggle read status for a thread
    ///
    /// Returns the new read state (true = read, false = unread).
    pub fn toggle_read(&self, thread_id: &ThreadId) -> Result<bool> {
        let msg_ids = self.store.get_message_ids_for_thread(thread_id)?;
        if msg_ids.is_empty() {
            return Ok(true); // Empty thread is "read"
        }

        // Check if any message is currently unread
        let has_unread = msg_ids.iter().any(|id| {
            self.store
                .get_message(id)
                .ok()
                .flatten()
                .map(|m| m.label_ids.contains(&labels::UNREAD.to_string()))
                .unwrap_or(false)
        });

        // If unread, mark as read. If read, mark as unread.
        let new_is_read = has_unread;
        self.set_read(thread_id, new_is_read)?;

        Ok(new_is_read)
    }

    /// Move a thread to trash
    pub fn trash_thread(&self, thread_id: &ThreadId) -> Result<()> {
        let msg_ids = self.store.get_message_ids_for_thread(thread_id)?;
        if msg_ids.is_empty() {
            return Ok(());
        }

        info!("Trashing thread {} ({} messages)", thread_id.as_str(), msg_ids.len());

        let id_strs: Vec<&str> = msg_ids.iter().map(|id| id.as_str()).collect();
        // Add TRASH and remove INBOX
        self.gmail.batch_modify_messages(&id_strs, &[labels::TRASH], &[labels::INBOX])?;

        // Update local storage
        for msg_id in &msg_ids {
            if let Some(msg) = self.store.get_message(msg_id)? {
                let mut new_labels = msg.label_ids.clone();
                new_labels.retain(|l| l != labels::INBOX);
                if !new_labels.contains(&labels::TRASH.to_string()) {
                    new_labels.push(labels::TRASH.to_string());
                }
                self.store.update_message_labels(msg_id, new_labels)?;
            }
        }

        info!("Trashed thread {}", thread_id.as_str());
        Ok(())
    }

    /// Check if a thread is in the inbox
    pub fn is_in_inbox(&self, thread_id: &ThreadId) -> Result<bool> {
        let msg_ids = self.store.get_message_ids_for_thread(thread_id)?;
        Ok(msg_ids.iter().any(|id| {
            self.store
                .get_message(id)
                .ok()
                .flatten()
                .map(|m| m.label_ids.contains(&labels::INBOX.to_string()))
                .unwrap_or(false)
        }))
    }

    /// Check if a thread is starred
    pub fn is_starred(&self, thread_id: &ThreadId) -> Result<bool> {
        let msg_ids = self.store.get_message_ids_for_thread(thread_id)?;
        Ok(msg_ids.iter().any(|id| {
            self.store
                .get_message(id)
                .ok()
                .flatten()
                .map(|m| m.label_ids.contains(&labels::STARRED.to_string()))
                .unwrap_or(false)
        }))
    }

    /// Check if a thread has unread messages
    pub fn is_unread(&self, thread_id: &ThreadId) -> Result<bool> {
        let msg_ids = self.store.get_message_ids_for_thread(thread_id)?;
        Ok(msg_ids.iter().any(|id| {
            self.store
                .get_message(id)
                .ok()
                .flatten()
                .map(|m| m.label_ids.contains(&labels::UNREAD.to_string()))
                .unwrap_or(false)
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{EmailAddress, Message, MessageId, Thread};
    use crate::storage::InMemoryMailStore;
    use chrono::Utc;

    // Note: These tests require mocking the GmailClient, which we don't have yet.
    // For now, we'll just test the logic that doesn't require API calls.

    fn make_test_thread(id: &str) -> Thread {
        Thread::new(
            ThreadId::new(id),
            1, // account_id
            "Test Subject".to_string(),
            "Test snippet".to_string(),
            Utc::now(),
            1,
            Some("Test User".to_string()),
            "test@example.com".to_string(),
            false,
        )
    }

    fn make_test_message(id: &str, thread_id: &str, labels: Vec<&str>) -> Message {
        Message::builder(MessageId::new(id), ThreadId::new(thread_id))
            .from(EmailAddress::new("test@example.com"))
            .subject("Test")
            .body_preview("Test body")
            .label_ids(labels.into_iter().map(|s| s.to_string()).collect())
            .build()
    }

    #[test]
    fn test_is_starred() {
        let store = Arc::new(InMemoryMailStore::new());

        // Set up test data
        store.upsert_thread(make_test_thread("t1")).unwrap();
        store.upsert_message(make_test_message("m1", "t1", vec!["INBOX", "STARRED"])).unwrap();

        // We can't create a real ActionHandler without a GmailClient,
        // but we can verify the store methods work correctly
        let msg = store.get_message(&MessageId::new("m1")).unwrap().unwrap();
        assert!(msg.label_ids.contains(&"STARRED".to_string()));
    }

    #[test]
    fn test_is_unread() {
        let store = Arc::new(InMemoryMailStore::new());

        store.upsert_thread(make_test_thread("t1")).unwrap();
        store.upsert_message(make_test_message("m1", "t1", vec!["INBOX", "UNREAD"])).unwrap();

        let msg = store.get_message(&MessageId::new("m1")).unwrap().unwrap();
        assert!(msg.label_ids.contains(&"UNREAD".to_string()));
    }
}
