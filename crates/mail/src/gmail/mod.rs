//! Gmail API integration
//!
//! This module provides:
//! - OAuth2 authentication flow
//! - Gmail API client for fetching messages
//! - Response normalization to domain models

mod auth;
mod client;
mod normalize;

pub use auth::GmailAuth;
pub use client::GmailClient;
pub use normalize::normalize_message;

/// Gmail API response types
pub mod api {
    use serde::{Deserialize, Serialize};

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

    /// Full message from Gmail API
    #[derive(Debug, Deserialize)]
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
    #[derive(Debug, Deserialize)]
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
    #[derive(Debug, Deserialize)]
    pub struct MessageBody {
        pub size: Option<u32>,
        pub data: Option<String>,
    }

    /// Message part (for multipart messages)
    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct MessagePart {
        pub part_id: Option<String>,
        pub mime_type: Option<String>,
        pub filename: Option<String>,
        pub headers: Option<Vec<Header>>,
        pub body: Option<MessageBody>,
        pub parts: Option<Vec<MessagePart>>,
    }
}
