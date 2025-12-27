//! Gmail API integration
//!
//! This module provides:
//! - OAuth2 authentication flow
//! - Gmail API client for fetching messages
//! - Response normalization to domain models

mod auth;
mod client;
mod normalize;

pub use auth::{GmailAuth, StoredToken};
pub use client::{GmailClient, HistoryExpiredError};
pub use normalize::normalize_message;

/// Gmail API request and response types
pub mod api {
    use serde::{Deserialize, Serialize};

    // === Mutation Request Types ===

    /// Request body for modifying message labels
    /// POST /gmail/v1/users/me/messages/{id}/modify
    #[derive(Debug, Serialize)]
    #[serde(rename_all = "camelCase")]
    pub struct ModifyMessageRequest {
        /// Label IDs to add
        #[serde(skip_serializing_if = "Vec::is_empty")]
        pub add_label_ids: Vec<String>,
        /// Label IDs to remove
        #[serde(skip_serializing_if = "Vec::is_empty")]
        pub remove_label_ids: Vec<String>,
    }

    /// Request body for batch modifying messages
    /// POST /gmail/v1/users/me/messages/batchModify
    #[derive(Debug, Serialize)]
    #[serde(rename_all = "camelCase")]
    pub struct BatchModifyRequest {
        /// Message IDs to modify
        pub ids: Vec<String>,
        /// Label IDs to add
        #[serde(skip_serializing_if = "Vec::is_empty")]
        pub add_label_ids: Vec<String>,
        /// Label IDs to remove
        #[serde(skip_serializing_if = "Vec::is_empty")]
        pub remove_label_ids: Vec<String>,
    }

    /// Response from listing messages
    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct ListMessagesResponse {
        pub messages: Option<Vec<MessageRef>>,
        pub next_page_token: Option<String>,
        pub result_size_estimate: Option<u32>,
    }

    /// Reference to a message (just ID and thread ID)
    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct MessageRef {
        pub id: String,
        pub thread_id: String,
    }

    /// Gmail API error response
    #[derive(Debug, Deserialize)]
    pub struct GmailErrorResponse {
        pub error: GmailError,
    }

    /// Gmail API error details
    #[derive(Debug, Deserialize)]
    pub struct GmailError {
        pub code: u16,
        pub message: String,
    }

    /// Batch response that can be either a message or an error
    #[derive(Debug, Deserialize)]
    #[serde(untagged)]
    pub enum BatchResponse {
        Message(GmailMessage),
        Error(GmailErrorResponse),
    }

    /// Full message from Gmail API
    #[derive(Debug, Deserialize, Serialize)]
    #[serde(rename_all = "camelCase")]
    pub struct GmailMessage {
        pub id: String,
        pub thread_id: String,
        pub label_ids: Option<Vec<String>>,
        pub snippet: String,
        pub internal_date: String,
        pub payload: Option<MessagePayload>,
    }

    /// Message payload containing headers and body
    #[derive(Debug, Deserialize, Serialize)]
    #[serde(rename_all = "camelCase")]
    pub struct MessagePayload {
        pub headers: Option<Vec<Header>>,
        pub body: Option<MessageBody>,
        pub parts: Option<Vec<MessagePart>>,
        pub mime_type: Option<String>,
    }

    /// Email header (name-value pair)
    #[derive(Debug, Deserialize, Serialize)]
    pub struct Header {
        pub name: String,
        pub value: String,
    }

    /// Message body (may be base64 encoded)
    #[derive(Debug, Deserialize, Serialize)]
    pub struct MessageBody {
        pub size: Option<u32>,
        pub data: Option<String>,
    }

    /// Message part (for multipart messages)
    #[derive(Debug, Deserialize, Serialize)]
    #[serde(rename_all = "camelCase")]
    pub struct MessagePart {
        pub part_id: Option<String>,
        pub mime_type: Option<String>,
        pub filename: Option<String>,
        pub headers: Option<Vec<Header>>,
        pub body: Option<MessageBody>,
        pub parts: Option<Vec<MessagePart>>,
    }

    // === Phase 2: History API Types ===

    /// Response from Gmail History API
    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct HistoryResponse {
        /// New history ID for next sync
        pub history_id: Option<String>,
        /// List of history records
        pub history: Option<Vec<HistoryRecord>>,
        /// Token for next page (if paginated)
        pub next_page_token: Option<String>,
    }

    /// A single history record containing changes
    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct HistoryRecord {
        /// The history ID of this record
        pub id: String,
        /// Messages added to the mailbox
        pub messages_added: Option<Vec<MessageAdded>>,
        /// Messages deleted from the mailbox
        pub messages_deleted: Option<Vec<MessageDeleted>>,
        /// Labels added to messages
        pub labels_added: Option<Vec<LabelChange>>,
        /// Labels removed from messages
        pub labels_removed: Option<Vec<LabelChange>>,
    }

    /// Message added to mailbox
    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct MessageAdded {
        pub message: MessageRef,
    }

    /// Message deleted from mailbox
    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct MessageDeleted {
        pub message: MessageRef,
    }

    /// Label change on a message
    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct LabelChange {
        pub message: MessageRef,
        pub label_ids: Vec<String>,
    }

    // === Profile API Types ===

    /// Response from Gmail Profile API (GET /users/me/profile)
    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct ProfileResponse {
        /// The user's email address
        pub email_address: String,
        /// Total number of messages in the mailbox
        pub messages_total: Option<u32>,
        /// Total number of threads in the mailbox
        pub threads_total: Option<u32>,
        /// The current history ID for incremental sync
        pub history_id: String,
    }

    // === Labels API Types ===

    /// Response from Gmail Labels API
    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct ListLabelsResponse {
        pub labels: Option<Vec<GmailLabel>>,
    }

    /// A Gmail label (folder)
    #[derive(Debug, Clone, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct GmailLabel {
        /// Label ID (e.g., "INBOX", "SENT", "Label_123")
        pub id: String,
        /// Display name
        pub name: String,
        /// Label type: "system" or "user"
        #[serde(rename = "type")]
        pub label_type: Option<String>,
        /// Number of messages with this label
        pub messages_total: Option<u32>,
        /// Number of unread messages
        pub messages_unread: Option<u32>,
        /// Number of threads with this label
        pub threads_total: Option<u32>,
        /// Number of unread threads
        pub threads_unread: Option<u32>,
    }
}
