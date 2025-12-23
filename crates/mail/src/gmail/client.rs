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
    ModifyMessageRequest, ProfileResponse,
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
    ///
    /// Note: This includes TRASH and SPAM for full Gmail parity.
    pub fn list_messages_with_label(
        &self,
        max_results: usize,
        page_token: Option<&str>,
        label_id: Option<&str>,
    ) -> Result<ListMessagesResponse> {
        let access_token = self.auth.get_access_token()?;

        // Include spam and trash for full Gmail parity
        let mut url = format!(
            "{}/users/me/messages?maxResults={}&includeSpamTrash=true",
            Self::BASE_URL,
            max_results.min(500)
        );

        if let Some(token) = page_token {
            url.push_str(&format!("&pageToken={}", token));
        }

        if let Some(label) = label_id {
            url.push_str(&format!("&labelIds={}", label));
        }

        let mut response = with_retry(
            || {
                ureq::get(&url)
                    .header("Authorization", &format!("Bearer {}", access_token))
                    .call()
            },
            3,
        )
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

        let mut response = with_retry(
            || {
                ureq::get(&url)
                    .header("Authorization", &format!("Bearer {}", access_token))
                    .call()
            },
            3,
        )
        .context("Failed to send get message request")?;

        let message: GmailMessage = response
            .body_mut()
            .read_json()
            .context("Failed to parse message response")?;

        Ok(message)
    }

    /// Get multiple messages using Gmail Batch API
    ///
    /// Uses the batch endpoint to combine up to 100 requests per HTTP call,
    /// dramatically reducing network overhead compared to individual requests.
    ///
    /// # Arguments
    /// * `ids` - The message IDs to fetch
    pub fn get_messages_batch(&self, ids: &[MessageId]) -> Vec<Result<GmailMessage>> {
        if ids.is_empty() {
            return Vec::new();
        }

        let access_token = match self.auth.get_access_token() {
            Ok(token) => token,
            Err(e) => {
                let err_msg = format!("Failed to get access token: {}", e);
                return ids
                    .iter()
                    .map(|_| Err(anyhow::anyhow!("{}", err_msg)))
                    .collect();
            }
        };

        let total = ids.len();
        // Gmail batch API max is 100 requests per batch
        let batch_size = 100;
        let num_batches = (total + batch_size - 1) / batch_size;

        info!(
            "Fetching {} messages via batch API ({} batches of up to {})",
            total, num_batches, batch_size
        );

        // Results indexed by original position
        let mut results: Vec<Option<Result<GmailMessage>>> =
            (0..total).map(|_| None).collect();

        // Adaptive rate limiting: adjust delay based on rate limit feedback
        // Start with small delay, increase on rate limits, decrease on success
        let mut inter_batch_delay_ms = 200u64; // Start conservative
        let mut backoff_ms = 0u64; // Extra backoff when rate limited

        for (batch_idx, chunk) in ids.chunks(batch_size).enumerate() {
            let chunk_start = batch_idx * batch_size;

            // Track which indices in this chunk still need fetching
            let mut pending: Vec<(usize, &MessageId)> = chunk.iter().enumerate().collect();
            let mut retry_count = 0u32;

            // Keep retrying until all messages succeed - sync must be complete
            while !pending.is_empty() {
                // Apply delays: inter-batch delay + any backoff from rate limiting
                let total_delay = if backoff_ms > 0 {
                    backoff_ms
                } else if retry_count == 0 && batch_idx > 0 {
                    inter_batch_delay_ms
                } else {
                    0
                };

                if total_delay > 0 {
                    if backoff_ms > 0 {
                        info!(
                            "[BATCH] Rate limited ({} pending), backing off {}ms (retry {})",
                            pending.len(),
                            total_delay,
                            retry_count
                        );
                    }
                    std::thread::sleep(Duration::from_millis(total_delay));
                }

                // Fetch pending messages
                let pending_ids: Vec<MessageId> =
                    pending.iter().map(|(_, id)| (*id).clone()).collect();
                let batch_results =
                    self.fetch_batch(&access_token, &pending_ids, batch_idx + 1, num_batches);

                // Process results, separating successes from retriable errors
                // Retry: 408 (timeout), 429 (rate limit), 5xx (server errors)
                let mut next_pending = Vec::new();
                for ((chunk_idx, id), result) in pending.into_iter().zip(batch_results) {
                    let is_retriable = result.as_ref().is_err_and(|e| {
                        let msg = e.to_string();
                        msg.contains("408")
                            || msg.contains("429")
                            || msg.contains("500")
                            || msg.contains("502")
                            || msg.contains("503")
                            || msg.contains("504")
                    });

                    if is_retriable {
                        next_pending.push((chunk_idx, id));
                    } else {
                        results[chunk_start + chunk_idx] = Some(result);
                    }
                }

                if next_pending.is_empty() {
                    // Success - reset backoff and try to speed up slightly
                    backoff_ms = 0;
                    if retry_count == 0 {
                        // No rate limits this batch, try going faster (min 50ms)
                        inter_batch_delay_ms = inter_batch_delay_ms.saturating_sub(25).max(50);
                    }
                } else {
                    // Rate limited - slow down for future batches and back off now
                    inter_batch_delay_ms = (inter_batch_delay_ms + 100).min(1000);
                    backoff_ms = if backoff_ms == 0 {
                        500
                    } else {
                        (backoff_ms * 2).min(16000)
                    };
                    retry_count += 1;
                }

                pending = next_pending;
            }
        }

        info!("Batch fetch complete: {} messages", total);
        results
            .into_iter()
            .map(|r| r.unwrap_or_else(|| Err(anyhow::anyhow!("Missing result"))))
            .collect()
    }

    /// Fetch a batch of messages using Gmail's batch endpoint
    fn fetch_batch(
        &self,
        access_token: &str,
        ids: &[MessageId],
        batch_num: usize,
        total_batches: usize,
    ) -> Vec<Result<GmailMessage>> {
        use log::debug;
        use std::io::Read;

        let boundary = format!("batch_{}", std::process::id());

        // Build multipart request body
        let mut body = String::new();
        for (i, id) in ids.iter().enumerate() {
            body.push_str(&format!("--{}\r\n", boundary));
            body.push_str("Content-Type: application/http\r\n");
            body.push_str(&format!("Content-ID: <msg{}>\r\n", i));
            body.push_str("\r\n");
            body.push_str(&format!(
                "GET /gmail/v1/users/me/messages/{}?format=full\r\n",
                id.as_str()
            ));
            body.push_str("\r\n");
        }
        body.push_str(&format!("--{}--\r\n", boundary));

        debug!(
            "[BATCH] Sending batch {}/{} with {} messages",
            batch_num, total_batches, ids.len()
        );

        // Send batch request
        let response = ureq::post("https://www.googleapis.com/batch/gmail/v1")
            .header("Authorization", &format!("Bearer {}", access_token))
            .header(
                "Content-Type",
                &format!("multipart/mixed; boundary={}", boundary),
            )
            .send(body.as_bytes());

        match response {
            Ok(mut resp) => {
                // Get response content type to extract boundary
                let content_type = resp
                    .headers()
                    .get("content-type")
                    .and_then(|v| v.to_str().ok())
                    .map(|s| s.to_string())
                    .unwrap_or_default();

                // Read full response body
                let mut response_body = String::new();
                if let Err(e) = resp.body_mut().as_reader().read_to_string(&mut response_body) {
                    return ids
                        .iter()
                        .map(|_| Err(anyhow::anyhow!("Failed to read batch response: {}", e)))
                        .collect();
                }

                // Parse multipart response
                self.parse_batch_response(&content_type, &response_body, ids)
            }
            Err(e) => {
                // Extract status code for retry logic if available
                let error_msg = match &e {
                    ureq::Error::StatusCode(code) => {
                        format!("Batch request failed ({}): {}", code, e)
                    }
                    _ => format!("Batch request failed: {}", e),
                };
                ids.iter()
                    .map(|_| Err(anyhow::anyhow!("{}", error_msg)))
                    .collect()
            }
        }
    }

    /// Parse a multipart batch response from Gmail
    fn parse_batch_response(
        &self,
        content_type: &str,
        body: &str,
        ids: &[MessageId],
    ) -> Vec<Result<GmailMessage>> {
        use log::{debug, warn};

        // Extract boundary from content type
        let boundary = content_type
            .split("boundary=")
            .nth(1)
            .map(|s| s.trim())
            .unwrap_or("");

        if boundary.is_empty() {
            return ids
                .iter()
                .map(|_| Err(anyhow::anyhow!("No boundary in batch response")))
                .collect();
        }

        let mut results: Vec<Result<GmailMessage>> = Vec::with_capacity(ids.len());
        let delimiter = format!("--{}", boundary);
        let parts: Vec<&str> = body.split(&delimiter).collect();

        // Skip first (empty) and last (closing --) parts
        for part in parts.iter().skip(1) {
            if part.starts_with("--") || part.trim().is_empty() {
                continue;
            }

            // Structure of each part:
            // 1. Part headers (Content-Type: application/http, Content-ID: ...)
            // 2. Blank line
            // 3. HTTP status line (HTTP/1.1 200 OK)
            // 4. HTTP headers
            // 5. Blank line
            // 6. JSON body
            //
            // We need to find the JSON body which starts after the SECOND blank line

            // Find the start of JSON by looking for opening brace
            if let Some(json_start) = part.find('{') {
                // Find matching closing brace by counting braces
                let json_slice = &part[json_start..];
                let mut brace_count = 0;
                let mut json_end = 0;

                for (i, c) in json_slice.char_indices() {
                    match c {
                        '{' => brace_count += 1,
                        '}' => {
                            brace_count -= 1;
                            if brace_count == 0 {
                                json_end = i + 1;
                                break;
                            }
                        }
                        _ => {}
                    }
                }

                if json_end > 0 {
                    let json = &json_slice[..json_end];
                    let json_trimmed = json.trim();

                    // Check for error response - error JSON has "error" as the first/only key
                    // Handle various formatting: {"error", { "error", {\n  "error", etc.
                    let is_error = {
                        // Remove leading { and whitespace, check if starts with "error"
                        let after_brace = json_trimmed.strip_prefix('{').unwrap_or("");
                        let trimmed = after_brace.trim_start();
                        trimmed.starts_with("\"error\"")
                    };
                    if is_error {
                        // Extract status code from error for retry logic
                        // Retriable: 408, 429, 500, 502, 503, 504
                        let error_msg = if json.contains("429") || json.contains("Too many") {
                            "Rate limited (429)"
                        } else if json.contains("503") {
                            "Service unavailable (503)"
                        } else if json.contains("502") {
                            "Bad gateway (502)"
                        } else if json.contains("500") {
                            "Internal server error (500)"
                        } else if json.contains("504") {
                            "Gateway timeout (504)"
                        } else if json.contains("408") {
                            "Request timeout (408)"
                        } else {
                            let preview: String = json.chars().take(100).collect();
                            warn!("Gmail API error: {}", preview);
                            "API error"
                        };
                        results.push(Err(anyhow::anyhow!("{}", error_msg)));
                        continue;
                    }

                    match serde_json::from_str::<GmailMessage>(json) {
                        Ok(msg) => results.push(Ok(msg)),
                        Err(e) => {
                            // Log first 200 chars of JSON for debugging
                            let preview: String = json.chars().take(200).collect();
                            debug!("Failed JSON preview: {}", preview);
                            warn!("Failed to parse message JSON: {}", e);
                            results.push(Err(anyhow::anyhow!("Failed to parse message: {}", e)));
                        }
                    }
                } else {
                    results.push(Err(anyhow::anyhow!("Malformed JSON in batch response")));
                }
            }
        }

        // If we didn't get enough results, fill with errors
        while results.len() < ids.len() {
            results.push(Err(anyhow::anyhow!("Missing response in batch")));
        }

        results
    }

    /// Get multiple messages in parallel with progress reporting (legacy method)
    ///
    /// Uses individual requests with rayon parallelism. Prefer get_messages_batch()
    /// which uses the batch API for better performance.
    #[allow(dead_code)]
    fn get_messages_parallel<F>(
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

        let mut response = with_retry(
            || {
                ureq::get(&url)
                    .header("Authorization", &format!("Bearer {}", access_token))
                    .call()
            },
            3,
        )
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

        // Retry loop with special handling for history expired errors
        let mut delay = Duration::from_millis(100);
        let max_retries = 3u32;

        for attempt in 0..max_retries {
            let response = ureq::get(&url)
                .header("Authorization", &format!("Bearer {}", access_token))
                .call();

            match response {
                Ok(mut resp) => {
                    let history: HistoryResponse = resp
                        .body_mut()
                        .read_json()
                        .context("Failed to parse history response")?;
                    return Ok(history);
                }
                Err(ureq::Error::StatusCode(404)) | Err(ureq::Error::StatusCode(400)) => {
                    // History ID expired, invalid, or malformed - triggers full resync
                    // Don't retry these, they're not transient
                    return Err(HistoryExpiredError.into());
                }
                Err(ref e) if is_retriable_error(e) && attempt < max_retries - 1 => {
                    let jitter = Duration::from_millis(rand_jitter());
                    std::thread::sleep(delay + jitter);
                    delay = (delay * 2).min(Duration::from_secs(16));
                }
                Err(e) => return Err(anyhow::anyhow!("Failed to fetch history: {}", e)),
            }
        }

        Err(anyhow::anyhow!("Failed to fetch history after {} retries", max_retries))
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

    // === Profile Methods ===

    /// Get the user's Gmail profile
    ///
    /// Returns profile information including the current history ID,
    /// which is needed for incremental sync.
    pub fn get_profile(&self) -> Result<ProfileResponse> {
        let access_token = self.auth.get_access_token()?;

        let url = format!("{}/users/me/profile", Self::BASE_URL);

        let mut response = with_retry(
            || {
                ureq::get(&url)
                    .header("Authorization", &format!("Bearer {}", access_token))
                    .call()
            },
            3,
        )
        .context("Failed to get Gmail profile")?;

        let profile: ProfileResponse = response
            .body_mut()
            .read_json()
            .context("Failed to parse profile response")?;

        Ok(profile)
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

        let mut response = with_retry(
            || {
                ureq::post(&url)
                    .header("Authorization", &format!("Bearer {}", access_token))
                    .header("Content-Type", "application/json")
                    .send_json(&request)
            },
            3,
        )
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

        with_retry(
            || {
                ureq::post(&url)
                    .header("Authorization", &format!("Bearer {}", access_token))
                    .header("Content-Type", "application/json")
                    .send_json(&request)
            },
            3,
        )
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

/// Check if an error is retriable (transient server error)
fn is_retriable_error(e: &ureq::Error) -> bool {
    matches!(
        e,
        ureq::Error::StatusCode(408)  // Request Timeout
            | ureq::Error::StatusCode(429)  // Too Many Requests
            | ureq::Error::StatusCode(500)  // Internal Server Error
            | ureq::Error::StatusCode(502)  // Bad Gateway
            | ureq::Error::StatusCode(503)  // Service Unavailable
            | ureq::Error::StatusCode(504)  // Gateway Timeout
    )
}

/// Execute an HTTP request with retry for transient errors
fn with_retry<T, F>(mut f: F, max_retries: u32) -> Result<T>
where
    F: FnMut() -> std::result::Result<T, ureq::Error>,
{
    let mut delay = Duration::from_millis(100);

    for attempt in 0..max_retries {
        match f() {
            Ok(result) => return Ok(result),
            Err(e) if is_retriable_error(&e) && attempt < max_retries - 1 => {
                let jitter = Duration::from_millis(rand_jitter());
                std::thread::sleep(delay + jitter);
                delay = (delay * 2).min(Duration::from_secs(16));
            }
            Err(e) => return Err(anyhow::anyhow!("{}", e)),
        }
    }

    unreachable!()
}
