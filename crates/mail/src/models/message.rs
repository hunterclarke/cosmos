//! Message model representing a Gmail message

use super::ThreadId;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Unique identifier for a message (Gmail message ID)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MessageId(pub String);

impl MessageId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for MessageId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for MessageId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

/// An email address with optional display name
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailAddress {
    /// Display name (e.g., "John Doe")
    pub name: Option<String>,
    /// Email address (e.g., "john@example.com")
    pub email: String,
}

impl EmailAddress {
    /// Create a new email address with just the email
    pub fn new(email: impl Into<String>) -> Self {
        Self {
            name: None,
            email: email.into(),
        }
    }

    /// Create a new email address with a display name
    pub fn with_name(name: impl Into<String>, email: impl Into<String>) -> Self {
        Self {
            name: Some(name.into()),
            email: email.into(),
        }
    }

    /// Parse an email address from a string like "John Doe <john@example.com>"
    pub fn parse(s: &str) -> Self {
        let s = s.trim();

        // Try to parse "Name <email>" format
        if let Some(angle_start) = s.rfind('<')
            && let Some(angle_end) = s.rfind('>')
            && angle_start < angle_end
        {
            let name = s[..angle_start].trim();
            let email = s[angle_start + 1..angle_end].trim();
            return Self {
                name: if name.is_empty() {
                    None
                } else {
                    Some(name.to_string())
                },
                email: email.to_string(),
            };
        }

        // Otherwise, treat the whole string as an email
        Self {
            name: None,
            email: s.to_string(),
        }
    }

    /// Format the email address for display
    pub fn display(&self) -> String {
        match &self.name {
            Some(name) => format!("{} <{}>", name, self.email),
            None => self.email.clone(),
        }
    }
}

/// A single email message within a thread
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Gmail message ID
    pub id: MessageId,
    /// ID of the thread this message belongs to
    pub thread_id: ThreadId,
    /// Sender's email address
    pub from: EmailAddress,
    /// Recipients (To field)
    pub to: Vec<EmailAddress>,
    /// CC recipients
    pub cc: Vec<EmailAddress>,
    /// Subject line
    pub subject: String,
    /// Plain text preview of the body
    pub body_preview: String,
    /// When the message was received
    pub received_at: DateTime<Utc>,
    /// Gmail's internal timestamp (milliseconds since epoch)
    pub internal_date: i64,
    /// Gmail label IDs (e.g., "INBOX", "SENT", "UNREAD")
    pub label_ids: Vec<String>,
}

impl Message {
    /// Create a new message builder
    pub fn builder(id: MessageId, thread_id: ThreadId) -> MessageBuilder {
        MessageBuilder::new(id, thread_id)
    }
}

/// Builder for creating Message instances
pub struct MessageBuilder {
    id: MessageId,
    thread_id: ThreadId,
    from: Option<EmailAddress>,
    to: Vec<EmailAddress>,
    cc: Vec<EmailAddress>,
    subject: String,
    body_preview: String,
    received_at: Option<DateTime<Utc>>,
    internal_date: i64,
    label_ids: Vec<String>,
}

impl MessageBuilder {
    fn new(id: MessageId, thread_id: ThreadId) -> Self {
        Self {
            id,
            thread_id,
            from: None,
            to: Vec::new(),
            cc: Vec::new(),
            subject: String::new(),
            body_preview: String::new(),
            received_at: None,
            internal_date: 0,
            label_ids: Vec::new(),
        }
    }

    pub fn from(mut self, from: EmailAddress) -> Self {
        self.from = Some(from);
        self
    }

    pub fn to(mut self, to: Vec<EmailAddress>) -> Self {
        self.to = to;
        self
    }

    pub fn cc(mut self, cc: Vec<EmailAddress>) -> Self {
        self.cc = cc;
        self
    }

    pub fn subject(mut self, subject: impl Into<String>) -> Self {
        self.subject = subject.into();
        self
    }

    pub fn body_preview(mut self, body_preview: impl Into<String>) -> Self {
        self.body_preview = body_preview.into();
        self
    }

    pub fn received_at(mut self, received_at: DateTime<Utc>) -> Self {
        self.received_at = Some(received_at);
        self
    }

    pub fn internal_date(mut self, internal_date: i64) -> Self {
        self.internal_date = internal_date;
        self
    }

    pub fn label_ids(mut self, label_ids: Vec<String>) -> Self {
        self.label_ids = label_ids;
        self
    }

    pub fn build(self) -> Message {
        Message {
            id: self.id,
            thread_id: self.thread_id,
            from: self
                .from
                .unwrap_or_else(|| EmailAddress::new("unknown@unknown.com")),
            to: self.to,
            cc: self.cc,
            subject: self.subject,
            body_preview: self.body_preview,
            received_at: self.received_at.unwrap_or_else(Utc::now),
            internal_date: self.internal_date,
            label_ids: self.label_ids,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_email_with_name() {
        let addr = EmailAddress::parse("John Doe <john@example.com>");
        assert_eq!(addr.name, Some("John Doe".to_string()));
        assert_eq!(addr.email, "john@example.com");
    }

    #[test]
    fn test_parse_email_without_name() {
        let addr = EmailAddress::parse("john@example.com");
        assert_eq!(addr.name, None);
        assert_eq!(addr.email, "john@example.com");
    }

    #[test]
    fn test_parse_email_with_angle_brackets_no_name() {
        let addr = EmailAddress::parse("<john@example.com>");
        assert_eq!(addr.name, None);
        assert_eq!(addr.email, "john@example.com");
    }

    #[test]
    fn test_display_with_name() {
        let addr = EmailAddress::with_name("John Doe", "john@example.com");
        assert_eq!(addr.display(), "John Doe <john@example.com>");
    }

    #[test]
    fn test_display_without_name() {
        let addr = EmailAddress::new("john@example.com");
        assert_eq!(addr.display(), "john@example.com");
    }
}
