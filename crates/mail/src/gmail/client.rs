//! Gmail API HTTP client
//!
//! Provides methods for fetching messages from the Gmail API.
//! Uses synchronous HTTP (ureq) to be executor-agnostic.

use anyhow::{Context, Result};
use std::time::Duration;

use super::api::{GmailMessage, HistoryResponse, ListLabelsResponse, ListMessagesResponse};
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
        let access_token = self.auth.get_access_token()?;

        let mut url = format!(
            "{}/users/me/messages?maxResults={}",
            Self::BASE_URL,
            max_results.min(500)
        );

        if let Some(token) = page_token {
            url.push_str(&format!("&pageToken={}", token));
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

    /// Get multiple messages with retry logic
    ///
    /// # Arguments
    /// * `ids` - The message IDs to fetch
    pub fn get_messages_batch(&self, ids: &[MessageId]) -> Vec<Result<GmailMessage>> {
        ids.iter()
            .map(|id| self.get_message_with_retry(id, 3))
            .collect()
    }

    /// Get a message with exponential backoff retry
    fn get_message_with_retry(&self, id: &MessageId, max_retries: u32) -> Result<GmailMessage> {
        let mut last_error = None;
        let mut delay = Duration::from_millis(100);

        for attempt in 0..max_retries {
            match self.get_message(id) {
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

        let mut url = format!(
            "{}/users/me/history?startHistoryId={}&historyTypes=messageAdded",
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
}

/// Generate a random jitter value (0-100ms)
fn rand_jitter() -> u64 {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};

    let hasher = RandomState::new().build_hasher();
    hasher.finish() % 100
}
