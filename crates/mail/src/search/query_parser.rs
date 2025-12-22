//! Gmail-style query parser
//!
//! Parses search queries with operators like:
//! - `from:john@example.com` - sender filter
//! - `to:team@company.com` - recipient filter
//! - `subject:meeting` - subject filter
//! - `in:inbox` - label filter
//! - `is:unread`, `is:read`, `is:starred` - boolean filters
//! - `has:attachment` - attachment filter
//! - `before:2024/12/01`, `after:2024/01/01` - date filters

use chrono::{DateTime, NaiveDate, TimeZone, Utc};

/// Parsed query with structured components
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ParsedQuery {
    /// Free-text search terms
    pub terms: Vec<String>,
    /// from: filter values
    pub from: Vec<String>,
    /// to: filter values
    pub to: Vec<String>,
    /// subject: filter values
    pub subject: Vec<String>,
    /// in: label filter (e.g., "INBOX", "SENT")
    pub in_label: Option<String>,
    /// is:unread / is:read
    pub is_unread: Option<bool>,
    /// is:starred
    pub is_starred: Option<bool>,
    /// has:attachment
    pub has_attachment: Option<bool>,
    /// before: date filter
    pub before: Option<DateTime<Utc>>,
    /// after: date filter
    pub after: Option<DateTime<Utc>>,
}

impl ParsedQuery {
    /// Check if the query is empty (no terms or filters)
    pub fn is_empty(&self) -> bool {
        self.terms.is_empty()
            && self.from.is_empty()
            && self.to.is_empty()
            && self.subject.is_empty()
            && self.in_label.is_none()
            && self.is_unread.is_none()
            && self.is_starred.is_none()
            && self.has_attachment.is_none()
            && self.before.is_none()
            && self.after.is_none()
    }
}

/// Parse a search query string into structured components
///
/// Supports Gmail-style operators:
/// - `from:value` or `from:"quoted value"`
/// - `to:value`
/// - `subject:value`
/// - `in:label`
/// - `is:unread`, `is:read`, `is:starred`
/// - `has:attachment`
/// - `before:YYYY/MM/DD` or `before:YYYY-MM-DD`
/// - `after:YYYY/MM/DD` or `after:YYYY-MM-DD`
///
/// Everything else is treated as free-text search terms.
pub fn parse_query(input: &str) -> ParsedQuery {
    let mut query = ParsedQuery::default();

    let mut i = 0;
    let chars: Vec<char> = input.chars().collect();

    while i < chars.len() {
        // Skip whitespace
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        if i >= chars.len() {
            break;
        }

        // Check for operator patterns
        let rest: String = chars[i..].iter().collect();

        if let Some((key, value, consumed)) = parse_operator(&rest) {
            match key.to_lowercase().as_str() {
                "from" => query.from.push(value),
                "to" => query.to.push(value),
                "subject" => query.subject.push(value),
                "in" => query.in_label = Some(value.to_uppercase()),
                "is" => match value.to_lowercase().as_str() {
                    "unread" => query.is_unread = Some(true),
                    "read" => query.is_unread = Some(false),
                    "starred" => query.is_starred = Some(true),
                    _ => {}
                },
                "has" => {
                    if value.to_lowercase() == "attachment" {
                        query.has_attachment = Some(true);
                    }
                }
                "before" => {
                    if let Some(date) = parse_date(&value) {
                        query.before = Some(date);
                    }
                }
                "after" => {
                    if let Some(date) = parse_date(&value) {
                        query.after = Some(date);
                    }
                }
                _ => {}
            }
            i += consumed;
        } else {
            // Regular word or quoted string
            let (word, consumed) = parse_word(&rest);
            if !word.is_empty() {
                query.terms.push(word);
            }
            i += consumed;
        }
    }

    query
}

/// Parse an operator like "from:value" or "from:\"quoted value\""
fn parse_operator(input: &str) -> Option<(String, String, usize)> {
    let colon_pos = input.find(':')?;
    let key = &input[..colon_pos];

    // Validate key is a known operator
    let valid_ops = [
        "from", "to", "subject", "in", "is", "has", "before", "after",
    ];
    if !valid_ops.contains(&key.to_lowercase().as_str()) {
        return None;
    }

    // Key must not contain whitespace
    if key.chars().any(|c| c.is_whitespace()) {
        return None;
    }

    let after_colon = &input[colon_pos + 1..];
    let (value, value_len) = parse_value(after_colon);

    // Don't match if value is empty
    if value.is_empty() {
        return None;
    }

    Some((key.to_string(), value, colon_pos + 1 + value_len))
}

/// Parse a value (quoted or unquoted)
fn parse_value(input: &str) -> (String, usize) {
    let chars: Vec<char> = input.chars().collect();

    if chars.is_empty() {
        return (String::new(), 0);
    }

    // Quoted value
    if chars[0] == '"' {
        let mut value = String::new();
        let mut i = 1;
        while i < chars.len() && chars[i] != '"' {
            value.push(chars[i]);
            i += 1;
        }
        let consumed = if i < chars.len() { i + 1 } else { i };
        return (value, consumed);
    }

    // Unquoted value (until whitespace)
    let mut value = String::new();
    let mut i = 0;
    while i < chars.len() && !chars[i].is_whitespace() {
        value.push(chars[i]);
        i += 1;
    }

    (value, i)
}

/// Parse a word or quoted phrase
fn parse_word(input: &str) -> (String, usize) {
    let chars: Vec<char> = input.chars().collect();

    if chars.is_empty() {
        return (String::new(), 0);
    }

    // Quoted phrase
    if chars[0] == '"' {
        let mut word = String::new();
        let mut i = 1;
        while i < chars.len() && chars[i] != '"' {
            word.push(chars[i]);
            i += 1;
        }
        let consumed = if i < chars.len() { i + 1 } else { i };
        return (word, consumed);
    }

    // Unquoted word
    let mut word = String::new();
    let mut i = 0;
    while i < chars.len() && !chars[i].is_whitespace() {
        word.push(chars[i]);
        i += 1;
    }

    (word, i)
}

/// Parse a date string (YYYY/MM/DD or YYYY-MM-DD)
fn parse_date(input: &str) -> Option<DateTime<Utc>> {
    // Try YYYY/MM/DD format
    if let Ok(date) = NaiveDate::parse_from_str(input, "%Y/%m/%d") {
        return date
            .and_hms_opt(0, 0, 0)
            .map(|dt| Utc.from_utc_datetime(&dt));
    }

    // Try YYYY-MM-DD format
    if let Ok(date) = NaiveDate::parse_from_str(input, "%Y-%m-%d") {
        return date
            .and_hms_opt(0, 0, 0)
            .map(|dt| Utc.from_utc_datetime(&dt));
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_query() {
        let query = parse_query("hello world");
        assert_eq!(query.terms, vec!["hello", "world"]);
        assert!(query.from.is_empty());
    }

    #[test]
    fn test_parse_quoted_phrase() {
        let query = parse_query("\"hello world\"");
        assert_eq!(query.terms, vec!["hello world"]);
    }

    #[test]
    fn test_parse_from_operator() {
        let query = parse_query("from:john@example.com");
        assert_eq!(query.from, vec!["john@example.com"]);
        assert!(query.terms.is_empty());
    }

    #[test]
    fn test_parse_quoted_operator_value() {
        let query = parse_query("from:\"John Doe\"");
        assert_eq!(query.from, vec!["John Doe"]);
    }

    #[test]
    fn test_parse_multiple_operators() {
        let query = parse_query("from:alice to:bob subject:meeting");
        assert_eq!(query.from, vec!["alice"]);
        assert_eq!(query.to, vec!["bob"]);
        assert_eq!(query.subject, vec!["meeting"]);
    }

    #[test]
    fn test_parse_multiple_from() {
        let query = parse_query("from:alice from:bob");
        assert_eq!(query.from, vec!["alice", "bob"]);
    }

    #[test]
    fn test_parse_is_unread() {
        let query = parse_query("is:unread important");
        assert_eq!(query.is_unread, Some(true));
        assert_eq!(query.terms, vec!["important"]);
    }

    #[test]
    fn test_parse_is_read() {
        let query = parse_query("is:read");
        assert_eq!(query.is_unread, Some(false));
    }

    #[test]
    fn test_parse_is_starred() {
        let query = parse_query("is:starred");
        assert_eq!(query.is_starred, Some(true));
    }

    #[test]
    fn test_parse_has_attachment() {
        let query = parse_query("has:attachment");
        assert_eq!(query.has_attachment, Some(true));
    }

    #[test]
    fn test_parse_in_label() {
        let query = parse_query("in:inbox");
        assert_eq!(query.in_label, Some("INBOX".to_string()));

        let query2 = parse_query("in:sent");
        assert_eq!(query2.in_label, Some("SENT".to_string()));
    }

    #[test]
    fn test_parse_date_filter_slash() {
        let query = parse_query("after:2024/01/01 before:2024/12/31");
        assert!(query.after.is_some());
        assert!(query.before.is_some());

        let after = query.after.unwrap();
        assert_eq!(after.format("%Y-%m-%d").to_string(), "2024-01-01");

        let before = query.before.unwrap();
        assert_eq!(before.format("%Y-%m-%d").to_string(), "2024-12-31");
    }

    #[test]
    fn test_parse_date_filter_dash() {
        let query = parse_query("after:2024-06-15");
        assert!(query.after.is_some());
        let after = query.after.unwrap();
        assert_eq!(after.format("%Y-%m-%d").to_string(), "2024-06-15");
    }

    #[test]
    fn test_parse_mixed_query() {
        let query = parse_query("from:alice is:unread important meeting");
        assert_eq!(query.from, vec!["alice"]);
        assert_eq!(query.is_unread, Some(true));
        assert_eq!(query.terms, vec!["important", "meeting"]);
    }

    #[test]
    fn test_parse_empty_query() {
        let query = parse_query("");
        assert!(query.is_empty());

        let query2 = parse_query("   ");
        assert!(query2.is_empty());
    }

    #[test]
    fn test_parse_invalid_operator_ignored() {
        // Unknown operator should be treated as text
        let query = parse_query("foo:bar");
        assert_eq!(query.terms, vec!["foo:bar"]);
    }

    #[test]
    fn test_parse_operator_with_empty_value() {
        // Empty value after colon - treat as text
        let query = parse_query("from: hello");
        assert!(query.from.is_empty());
        assert_eq!(query.terms, vec!["from:", "hello"]);
    }

    #[test]
    fn test_is_empty() {
        let empty = ParsedQuery::default();
        assert!(empty.is_empty());

        let mut with_terms = ParsedQuery::default();
        with_terms.terms.push("hello".to_string());
        assert!(!with_terms.is_empty());

        let mut with_from = ParsedQuery::default();
        with_from.from.push("alice".to_string());
        assert!(!with_from.is_empty());
    }
}
