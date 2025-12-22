//! Gmail API HTTP client
//!
//! Provides methods for fetching messages from the Gmail API.
//! Uses synchronous HTTP (ureq) to be executor-agnostic.

use anyhow::{Context, Result};
use log::info;
use rayon::prelude::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use super::api::{
    BatchModifyRequest, GmailMessage, HistoryResponse, ListLabelsResponse, ListMessagesResponse,
    ModifyMessageRequest,
};
use super::GmailAuth;
use crate::models::MessageId;

/// Error indicating the history ID has expired
#[derive(Debug, thiserror::Error)]
#[error("History ID expired or invalid")]
pub struct HistoryExpiredError;

/// Gmail API client for fetching messages
pub struct GmailClient {
    auth: GmailAuth,
}

impl GmailClient {
    /// Gmail API base URL
    const BASE_URL: &'static str = "https://gmail.googleapis.com/gmail/v1";

    /// Create a new Gmail client
    pub fn new(auth: GmailAuth) -> Self {
        Self { auth }
    }

    /// List message IDs from the user's mailbox
    ///
    /// # Arguments
    /// * `max_results` - Maximum number of messages to return per page (1-500)
    /// * `page_token` - Optional page token for pagination
    pub fn list_messages(
        &self,
        max_results: usize,
        page_token: Option<&str>,
    ) -> Result<ListMessagesResponse> {
        self.list_messages_with_label(max_results, page_token, None)
    }

    /// List message IDs from the user's mailbox, optionally filtered by label
    ///
    /// # Arguments
    /// * `max_results` - Maximum number of messages to return per page (1-500)
    /// * `page_token` - Optional page token for pagination
    /// * `label_id` - Optional label ID to filter by (e.g., "INBOX")
    pub fn list_messages_with_label(
        &self,
        max_results: usize,
        page_token: Option<&str>,
        label_id: Option<&str>,
    ) -> Result<ListMessagesResponse> {
        let access_token = self.auth.get_access_token()?;

        let mut url = format!(
            "{}/users/me/messages?maxResults={}",
            Self::BASE_URL,
            max_results.min(500)
        );

        if let Some(token) = page_token {
            url.push_str(&format!("&pageToken={}", token));
        }

        if let Some(label) = label_id {
            url.push_str(&format!("&labelIds={}", label));
        }

        let mut response = ureq::get(&url)
            .header("Authorization", &format!("Bearer {}", access_token))
            .call()
            .context("Failed to send list messages request")?;

        let list: ListMessagesResponse = response
            .body_mut()
            .read_json()
            .context("Failed to parse list messages response")?;

        Ok(list)
    }

    /// List ALL message IDs from the user's mailbox
    ///
    /// Automatically handles pagination to fetch all messages.
    /// Use with caution for large mailboxes.
    ///
    /// # Arguments
    /// * `max_messages` - Optional maximum total messages to fetch (None = all messages)
    /// * `progress_callback` - Optional callback called with (fetched_count, total_estimate)
    pub fn list_messages_all<F>(&self, max_messages: Option<usize>, mut progress_callback: F) -> Result<ListMessagesResponse>
    where
        F: FnMut(usize, Option<u32>),
    {
        use super::api::MessageRef;

        let mut all_messages: Vec<MessageRef> = Vec::new();
        let mut page_token = None;
        let mut result_size_estimate = None;

        loop {
            // Check if we've hit the limit
            if let Some(max) = max_messages {
                if all_messages.len() >= max {
                    break;
                }
            }

            let response = self.list_messages(500, page_token.as_deref())?;

            // Track total estimate
            if response.result_size_estimate.is_some() {
                result_size_estimate = response.result_size_estimate;
            }

            // Collect messages
            if let Some(messages) = response.messages {
                all_messages.extend(messages);
            }

            // Call progress callback
            progress_callback(all_messages.len(), result_size_estimate);

            // Check for next page
            match response.next_page_token {
                Some(token) => page_token = Some(token),
                None => break,
            }
        }

        // Trim to max if needed
        if let Some(max) = max_messages {
            all_messages.truncate(max);
        }

        Ok(ListMessagesResponse {
            messages: if all_messages.is_empty() {
                None
            } else {
                Some(all_messages)
            },
            next_page_token: None,
            result_size_estimate,
        })
    }

    /// Get full message details by ID
    ///
    /// # Arguments
    /// * `id` - The message ID to fetch
    pub fn get_message(&self, id: &MessageId) -> Result<GmailMessage> {
        let access_token = self.auth.get_access_token()?;

        let url = format!(
            "{}/users/me/messages/{}?format=full",
            Self::BASE_URL,
            id.as_str()
        );

        let mut response = ureq::get(&url)
            .header("Authorization", &format!("Bearer {}", access_token))
            .call()
            .context("Failed to send get message request")?;

        let message: GmailMessage = response
            .body_mut()
            .read_json()
            .context("Failed to parse message response")?;

        Ok(message)
    }

    /// Get multiple messages in parallel with retry logic
    ///
    /// Uses rayon for parallel fetching to significantly speed up bulk downloads.
    /// Progress callback receives (completed_count, total_count).
    ///
    /// # Arguments
    /// * `ids` - The message IDs to fetch
    pub fn get_messages_batch(&self, ids: &[MessageId]) -> Vec<Result<GmailMessage>> {
        self.get_messages_batch_parallel(ids, |_, _| {})
    }

    /// Get multiple messages in parallel with progress reporting
    ///
    /// # Arguments
    /// * `ids` - The message IDs to fetch
    /// * `progress` - Callback called with (completed, total) after each message
    pub fn get_messages_batch_parallel<F>(
        &self,
        ids: &[MessageId],
        progress: F,
    ) -> Vec<Result<GmailMessage>>
    where
        F: Fn(usize, usize) + Sync,
    {
        if ids.is_empty() {
            return Vec::new();
        }

        // Pre-fetch access token to avoid contention during parallel fetches
        let access_token = match self.auth.get_access_token() {
            Ok(token) => token,
            Err(e) => {
                // Return error for all messages if we can't get a token
                let err_msg = format!("Failed to get access token: {}", e);
                return ids
                    .iter()
                    .map(|_| Err(anyhow::anyhow!("{}", err_msg)))
                    .collect();
            }
        };

        let total = ids.len();
        let completed = AtomicUsize::new(0);
        let num_threads = rayon::current_num_threads();
        info!(
            "Fetching {} messages in parallel ({} threads)",
            total, num_threads
        );

        // Fetch messages in parallel using rayon
        let results: Vec<_> = ids
            .par_iter()
            .map(|id| {
                let result = self.get_message_with_token_retry(id, &access_token, 3);
                let done = completed.fetch_add(1, Ordering::Relaxed) + 1;
                progress(done, total);
                result
            })
            .collect();

        info!("Parallel fetch complete: {} messages", total);
        results
    }

    /// Get a message using a pre-fetched access token with retry
    fn get_message_with_token_retry(
        &self,
        id: &MessageId,
        access_token: &str,
        max_retries: u32,
    ) -> Result<GmailMessage> {
        let mut last_error = None;
        let mut delay = Duration::from_millis(100);

        for attempt in 0..max_retries {
            match self.get_message_with_token(id, access_token) {
                Ok(msg) => return Ok(msg),
                Err(e) => {
                    last_error = Some(e);
                    if attempt < max_retries - 1 {
                        // Add jitter to delay
                        let jitter = Duration::from_millis(rand_jitter());
                        std::thread::sleep(delay + jitter);
                        delay *= 2;
                    }
                }
            }
        }

        Err(last_error.unwrap())
    }

    /// Get a message using a pre-fetched access token
    fn get_message_with_token(&self, id: &MessageId, access_token: &str) -> Result<GmailMessage> {
        let url = format!(
            "{}/users/me/messages/{}?format=full",
            Self::BASE_URL,
            id.as_str()
        );

        let mut response = ureq::get(&url)
            .header("Authorization", &format!("Bearer {}", access_token))
            .call()
            .context("Failed to send get message request")?;

        let message: GmailMessage = response
            .body_mut()
            .read_json()
            .context("Failed to parse message response")?;

        Ok(message)
    }

    /// Check if the client is authenticated
    pub fn is_authenticated(&self) -> bool {
        self.auth.is_authenticated()
    }

    /// Trigger authentication flow
    pub fn authenticate(&self) -> Result<()> {
        self.auth.get_access_token()?;
        Ok(())
    }

    // === Labels API ===

    /// List all labels (folders) in the user's mailbox
    pub fn list_labels(&self) -> Result<ListLabelsResponse> {
        let access_token = self.auth.get_access_token()?;

        let url = format!("{}/users/me/labels", Self::BASE_URL);

        let mut response = ureq::get(&url)
            .header("Authorization", &format!("Bearer {}", access_token))
            .call()
            .context("Failed to send list labels request")?;

        let labels: ListLabelsResponse = response
            .body_mut()
            .read_json()
            .context("Failed to parse labels response")?;

        Ok(labels)
    }

    // === Phase 2: History API Methods ===

    /// List history since a given historyId
    ///
    /// Returns changes (added messages, etc.) since the specified historyId.
    /// Used for incremental sync.
    ///
    /// # Arguments
    /// * `start_history_id` - The history ID to start from (from previous sync)
    /// * `page_token` - Optional page token for pagination
    ///
    /// # Errors
    /// Returns `HistoryExpiredError` if the history ID is too old (404 from Gmail)
    pub fn list_history(
        &self,
        start_history_id: &str,
        page_token: Option<&str>,
    ) -> Result<HistoryResponse> {
        let access_token = self.auth.get_access_token()?;

        // Request all relevant history types: new messages and label changes
        let mut url = format!(
            "{}/users/me/history?startHistoryId={}&historyTypes=messageAdded&historyTypes=labelAdded&historyTypes=labelRemoved",
            Self::BASE_URL,
            start_history_id
        );

        if let Some(token) = page_token {
            url.push_str(&format!("&pageToken={}", token));
        }

        let response = ureq::get(&url)
            .header("Authorization", &format!("Bearer {}", access_token))
            .call();

        match response {
            Ok(mut resp) => {
                let history: HistoryResponse = resp
                    .body_mut()
                    .read_json()
                    .context("Failed to parse history response")?;
                Ok(history)
            }
            Err(ureq::Error::StatusCode(404)) => {
                // History ID expired or invalid
                Err(HistoryExpiredError.into())
            }
            Err(e) => Err(anyhow::anyhow!("Failed to fetch history: {}", e)),
        }
    }

    /// List all history pages since a given historyId
    ///
    /// Automatically handles pagination to fetch all history records.
    pub fn list_history_all(&self, start_history_id: &str) -> Result<HistoryResponse> {
        let mut all_records = Vec::new();
        let mut final_history_id = None;
        let mut page_token = None;

        loop {
            let response = self.list_history(start_history_id, page_token.as_deref())?;

            // Collect history records
            if let Some(records) = response.history {
                all_records.extend(records);
            }

            // Update final history ID
            if response.history_id.is_some() {
                final_history_id = response.history_id;
            }

            // Check for next page
            match response.next_page_token {
                Some(token) => page_token = Some(token),
                None => break,
            }
        }

        Ok(HistoryResponse {
            history_id: final_history_id,
            history: if all_records.is_empty() {
                None
            } else {
                Some(all_records)
            },
            next_page_token: None,
        })
    }

    // === Message Mutation Methods ===

    /// Modify labels on a single message
    ///
    /// This is the core mutation primitive for archive, star, read/unread operations.
    ///
    /// # Arguments
    /// * `message_id` - The message ID to modify
    /// * `add_labels` - Label IDs to add (e.g., "STARRED", "UNREAD")
    /// * `remove_labels` - Label IDs to remove (e.g., "INBOX", "UNREAD")
    ///
    /// # Examples
    /// - Archive: `modify_message(id, &[], &["INBOX"])`
    /// - Star: `modify_message(id, &["STARRED"], &[])`
    /// - Mark read: `modify_message(id, &[], &["UNREAD"])`
    /// - Mark unread: `modify_message(id, &["UNREAD"], &[])`
    pub fn modify_message(
        &self,
        message_id: &str,
        add_labels: &[&str],
        remove_labels: &[&str],
    ) -> Result<GmailMessage> {
        let access_token = self.auth.get_access_token()?;

        let url = format!(
            "{}/users/me/messages/{}/modify",
            Self::BASE_URL,
            message_id
        );

        let request = ModifyMessageRequest {
            add_label_ids: add_labels.iter().map(|s| s.to_string()).collect(),
            remove_label_ids: remove_labels.iter().map(|s| s.to_string()).collect(),
        };

        let mut response = ureq::post(&url)
            .header("Authorization", &format!("Bearer {}", access_token))
            .header("Content-Type", "application/json")
            .send_json(&request)
            .context("Failed to send modify message request")?;

        let message: GmailMessage = response
            .body_mut()
            .read_json()
            .context("Failed to parse modify message response")?;

        info!(
            "Modified message {}: +{:?} -{:?}",
            message_id, add_labels, remove_labels
        );

        Ok(message)
    }

    /// Batch modify labels on multiple messages
    ///
    /// More efficient than calling modify_message in a loop.
    /// Note: This endpoint has no response body on success.
    ///
    /// # Arguments
    /// * `message_ids` - The message IDs to modify
    /// * `add_labels` - Label IDs to add
    /// * `remove_labels` - Label IDs to remove
    pub fn batch_modify_messages(
        &self,
        message_ids: &[&str],
        add_labels: &[&str],
        remove_labels: &[&str],
    ) -> Result<()> {
        if message_ids.is_empty() {
            return Ok(());
        }

        let access_token = self.auth.get_access_token()?;

        let url = format!("{}/users/me/messages/batchModify", Self::BASE_URL);

        let request = BatchModifyRequest {
            ids: message_ids.iter().map(|s| s.to_string()).collect(),
            add_label_ids: add_labels.iter().map(|s| s.to_string()).collect(),
            remove_label_ids: remove_labels.iter().map(|s| s.to_string()).collect(),
        };

        ureq::post(&url)
            .header("Authorization", &format!("Bearer {}", access_token))
            .header("Content-Type", "application/json")
            .send_json(&request)
            .context("Failed to send batch modify request")?;

        info!(
            "Batch modified {} messages: +{:?} -{:?}",
            message_ids.len(),
            add_labels,
            remove_labels
        );

        Ok(())
    }
}

/// Generate a random jitter value (0-100ms)
fn rand_jitter() -> u64 {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};

    let hasher = RandomState::new().build_hasher();
    hasher.finish() % 100
}
