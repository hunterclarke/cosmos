//! Full-text search module using Tantivy
//!
//! Provides Gmail-style search with operators like `from:`, `to:`, `subject:`,
//! `is:unread`, `in:inbox`, `before:`, `after:`, etc.

mod index;
mod query_parser;
mod schema;

pub use index::SearchIndex;
pub use query_parser::{parse_query, ParsedQuery};

use crate::models::ThreadId;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A highlighted text span within a field
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HighlightSpan {
    /// Start byte offset
    pub start: usize,
    /// End byte offset
    pub end: usize,
}

/// Match highlights for a specific field
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldHighlight {
    /// Field name (e.g., "subject", "body_text")
    pub field: String,
    /// The text containing highlights
    pub text: String,
    /// Highlight spans within the text
    pub highlights: Vec<HighlightSpan>,
}

/// A single search result representing a thread
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    /// Thread ID
    pub thread_id: ThreadId,
    /// Thread subject
    pub subject: String,
    /// Thread snippet/preview
    pub snippet: String,
    /// Timestamp of the last message in the thread
    pub last_message_at: DateTime<Utc>,
    /// Number of messages in the thread
    pub message_count: usize,
    /// Sender display name (if available)
    pub sender_name: Option<String>,
    /// Sender email address
    pub sender_email: String,
    /// Whether the thread has unread messages
    pub is_unread: bool,
    /// Highlighted matches in various fields
    pub highlights: Vec<FieldHighlight>,
    /// Relevance score from Tantivy
    pub score: f32,
}

/// Search threads by query string
///
/// This is the main entry point for searching. It parses the query string,
/// executes the search against the index, and returns results with thread metadata.
///
/// # Arguments
/// * `index` - The search index to query
/// * `store` - Mail store for fetching thread metadata
/// * `query` - Search query string (supports Gmail-style operators)
/// * `limit` - Maximum number of results to return
///
/// # Example
/// ```ignore
/// let results = search_threads(&index, store, "from:alice is:unread", 50)?;
/// ```
pub fn search_threads(
    index: &SearchIndex,
    store: &dyn crate::storage::MailStore,
    query: &str,
    limit: usize,
) -> anyhow::Result<Vec<SearchResult>> {
    let parsed = parse_query(query);
    index.search(&parsed, limit, store)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_result_serialization() {
        let result = SearchResult {
            thread_id: ThreadId::new("thread123"),
            subject: "Test Subject".to_string(),
            snippet: "This is a test...".to_string(),
            last_message_at: Utc::now(),
            message_count: 3,
            sender_name: Some("Alice".to_string()),
            sender_email: "alice@example.com".to_string(),
            is_unread: true,
            highlights: vec![FieldHighlight {
                field: "subject".to_string(),
                text: "Test Subject".to_string(),
                highlights: vec![HighlightSpan { start: 0, end: 4 }],
            }],
            score: 1.5,
        };

        let json = serde_json::to_string(&result).unwrap();
        let deserialized: SearchResult = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.thread_id.as_str(), "thread123");
    }
}
