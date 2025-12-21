//! Label model representing a Gmail label/folder

use serde::{Deserialize, Serialize};

/// Unique identifier for a label (Gmail label ID)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LabelId(pub String);

impl LabelId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    // Well-known Gmail system labels
    pub const INBOX: &'static str = "INBOX";
    pub const SENT: &'static str = "SENT";
    pub const DRAFTS: &'static str = "DRAFT";
    pub const TRASH: &'static str = "TRASH";
    pub const SPAM: &'static str = "SPAM";
    pub const STARRED: &'static str = "STARRED";
    pub const IMPORTANT: &'static str = "IMPORTANT";
    pub const UNREAD: &'static str = "UNREAD";
    pub const ALL_MAIL: &'static str = "ALL";
}

impl From<String> for LabelId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for LabelId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

/// A mail label (folder)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Label {
    /// Label ID (e.g., "INBOX", "SENT", "Label_123")
    pub id: LabelId,
    /// Display name
    pub name: String,
    /// Whether this is a system label
    pub is_system: bool,
    /// Number of messages with this label
    pub message_count: u32,
    /// Number of unread messages
    pub unread_count: u32,
}

impl Label {
    /// Create a new label
    pub fn new(id: impl Into<LabelId>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            is_system: false,
            message_count: 0,
            unread_count: 0,
        }
    }

    /// Create a system label
    pub fn system(id: impl Into<LabelId>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            is_system: true,
            message_count: 0,
            unread_count: 0,
        }
    }

    /// Builder method to set message count
    pub fn with_message_count(mut self, count: u32) -> Self {
        self.message_count = count;
        self
    }

    /// Builder method to set unread count
    pub fn with_unread_count(mut self, count: u32) -> Self {
        self.unread_count = count;
        self
    }
}

/// Get the display icon for a label
pub fn label_icon(label_id: &str) -> &'static str {
    match label_id {
        LabelId::INBOX => "📥",
        LabelId::SENT => "📤",
        LabelId::DRAFTS => "📝",
        LabelId::TRASH => "🗑",
        LabelId::SPAM => "⚠️",
        LabelId::STARRED => "⭐",
        LabelId::IMPORTANT => "❗",
        LabelId::ALL_MAIL => "📬",
        _ => "📁",
    }
}

/// Get the display order for system labels
pub fn label_sort_order(label_id: &str) -> u32 {
    match label_id {
        LabelId::INBOX => 0,
        LabelId::STARRED => 1,
        LabelId::IMPORTANT => 2,
        LabelId::SENT => 3,
        LabelId::DRAFTS => 4,
        LabelId::ALL_MAIL => 5,
        LabelId::SPAM => 6,
        LabelId::TRASH => 7,
        _ => 100, // User labels come after system labels
    }
}
