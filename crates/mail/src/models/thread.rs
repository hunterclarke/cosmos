//! Thread model representing a Gmail thread (conversation)

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Unique identifier for a thread (Gmail thread ID)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ThreadId(pub String);

impl ThreadId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for ThreadId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for ThreadId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

/// A thread represents a conversation containing one or more messages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Thread {
    /// Gmail thread ID
    pub id: ThreadId,
    /// Subject line of the thread
    pub subject: String,
    /// Preview text (snippet) of the latest message
    pub snippet: String,
    /// Timestamp of the most recent message in the thread
    pub last_message_at: DateTime<Utc>,
    /// Number of messages in the thread
    pub message_count: usize,
    /// Display name of the thread sender (from first message)
    #[serde(default)]
    pub sender_name: Option<String>,
    /// Email address of the thread sender (from first message)
    #[serde(default)]
    pub sender_email: String,
    /// Whether the thread has unread messages
    #[serde(default)]
    pub is_unread: bool,
}

impl Thread {
    /// Create a new thread with the given properties
    pub fn new(
        id: ThreadId,
        subject: String,
        snippet: String,
        last_message_at: DateTime<Utc>,
        message_count: usize,
        sender_name: Option<String>,
        sender_email: String,
        is_unread: bool,
    ) -> Self {
        Self {
            id,
            subject,
            snippet,
            last_message_at,
            message_count,
            sender_name,
            sender_email,
            is_unread,
        }
    }
}
