//! Blob storage trait for large content (message bodies, attachments)

use anyhow::Result;

/// Types of content that can be stored in blob storage
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ContentType {
    /// Plain text body
    BodyText,
    /// HTML body
    BodyHtml,
    /// Attachment (future)
    Attachment,
}

impl ContentType {
    /// File extension for this content type
    pub fn extension(&self) -> &'static str {
        match self {
            ContentType::BodyText => "txt",
            ContentType::BodyHtml => "html",
            ContentType::Attachment => "bin",
        }
    }
}

/// Key for storing/retrieving blob content
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BlobKey {
    /// Message ID this content belongs to
    pub message_id: String,
    /// Type of content
    pub content_type: ContentType,
    /// Optional part identifier (for attachments: "0", "1", etc.)
    pub part_id: Option<String>,
}

impl BlobKey {
    /// Create a key for plain text body content
    pub fn body_text(message_id: &str) -> Self {
        Self {
            message_id: message_id.to_string(),
            content_type: ContentType::BodyText,
            part_id: None,
        }
    }

    /// Create a key for HTML body content
    pub fn body_html(message_id: &str) -> Self {
        Self {
            message_id: message_id.to_string(),
            content_type: ContentType::BodyHtml,
            part_id: None,
        }
    }

    /// Create a key for an attachment
    pub fn attachment(message_id: &str, part_id: &str) -> Self {
        Self {
            message_id: message_id.to_string(),
            content_type: ContentType::Attachment,
            part_id: Some(part_id.to_string()),
        }
    }
}

/// Trait for blob storage operations
///
/// Implementations handle compression/decompression internally.
pub trait BlobStore: Send + Sync {
    /// Store blob content
    ///
    /// Content is compressed before storage if the implementation supports it.
    fn put(&self, key: &BlobKey, data: &[u8]) -> Result<()>;

    /// Retrieve blob content
    ///
    /// Returns None if the blob doesn't exist.
    /// Content is decompressed automatically.
    fn get(&self, key: &BlobKey) -> Result<Option<Vec<u8>>>;

    /// Check if a blob exists
    fn exists(&self, key: &BlobKey) -> Result<bool>;

    /// Delete a blob
    fn delete(&self, key: &BlobKey) -> Result<()>;

    /// Delete all blobs for a message
    fn delete_all_for_message(&self, message_id: &str) -> Result<()>;

    /// Clear all blobs (for testing/reset)
    fn clear(&self) -> Result<()>;
}
