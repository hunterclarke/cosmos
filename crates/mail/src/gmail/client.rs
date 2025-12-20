//! Gmail API HTTP client
//!
//! Provides methods for fetching messages from the Gmail API.
//! Uses synchronous HTTP (ureq) to be executor-agnostic.

use anyhow::{Context, Result};
use std::time::Duration;

use super::api::{GmailMessage, ListMessagesResponse};
use super::GmailAuth;
use crate::models::MessageId;

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

    /// List message IDs from the inbox
    ///
    /// # Arguments
    /// * `max_results` - Maximum number of messages to return (1-500)
    /// * `page_token` - Optional page token for pagination
    pub fn list_messages(
        &self,
        max_results: usize,
        page_token: Option<&str>,
    ) -> Result<ListMessagesResponse> {
        let access_token = self.auth.get_access_token()?;

        let mut url = format!(
            "{}/users/me/messages?maxResults={}&q=in:inbox",
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
}

/// Generate a random jitter value (0-100ms)
fn rand_jitter() -> u64 {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};

    let hasher = RandomState::new().build_hasher();
    hasher.finish() % 100
}
