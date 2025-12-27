//! Gmail API response normalization
//!
//! Converts Gmail API responses to Orion domain models.

use anyhow::{Context, Result};
use base64::prelude::*;
use chrono::{TimeZone, Utc};

use super::api::{GmailMessage, MessagePart, MessagePayload};
use crate::models::{EmailAddress, Message, MessageId, ThreadId};

/// Normalize a Gmail API message to an Orion Message
pub fn normalize_message(gmail_msg: GmailMessage, account_id: i64) -> Result<Message> {
    let id = MessageId::new(&gmail_msg.id);
    let thread_id = ThreadId::new(&gmail_msg.thread_id);

    let payload = gmail_msg
        .payload
        .as_ref()
        .context("Message has no payload")?;

    // Extract headers
    let from = extract_header(payload, "From")
        .map(|s| EmailAddress::parse(&s))
        .unwrap_or_else(|| EmailAddress::new("unknown@unknown.com"));

    let to = extract_header(payload, "To")
        .map(|s| parse_address_list(&s))
        .unwrap_or_default();

    let cc = extract_header(payload, "Cc")
        .map(|s| parse_address_list(&s))
        .unwrap_or_default();

    let subject = extract_header(payload, "Subject").unwrap_or_default();

    // Parse internal date (milliseconds since epoch)
    let internal_date: i64 = gmail_msg.internal_date.parse().unwrap_or(0);
    let received_at = Utc
        .timestamp_millis_opt(internal_date)
        .single()
        .unwrap_or_else(Utc::now);

    // Extract full body content (both text and HTML)
    let body_text = extract_plain_text_body(payload);
    let body_html = extract_html_body(payload);

    // Extract body preview - prefer the snippet, fall back to extracting from body
    let body_preview = if !gmail_msg.snippet.is_empty() {
        decode_html_entities(&gmail_msg.snippet)
    } else {
        body_text.clone().unwrap_or_default()
    };

    // Extract label IDs
    let label_ids = gmail_msg.label_ids.unwrap_or_default();

    Ok(Message::builder(id, thread_id)
        .account_id(account_id)
        .from(from)
        .to(to)
        .cc(cc)
        .subject(subject)
        .body_preview(body_preview)
        .body_text(body_text)
        .body_html(body_html)
        .received_at(received_at)
        .internal_date(internal_date)
        .label_ids(label_ids)
        .build())
}

/// Extract a header value by name
fn extract_header(payload: &MessagePayload, name: &str) -> Option<String> {
    payload.headers.as_ref()?.iter().find_map(|h| {
        if h.name.eq_ignore_ascii_case(name) {
            Some(h.value.clone())
        } else {
            None
        }
    })
}

/// Parse a comma-separated list of email addresses
fn parse_address_list(s: &str) -> Vec<EmailAddress> {
    s.split(',')
        .map(|addr| EmailAddress::parse(addr.trim()))
        .collect()
}

/// Extract plain text body from message payload
fn extract_plain_text_body(payload: &MessagePayload) -> Option<String> {
    // Check if this is a simple message with body data
    if let Some(body) = &payload.body
        && let Some(data) = &body.data
        && payload
            .mime_type
            .as_ref()
            .is_some_and(|m| m.starts_with("text/plain"))
    {
        return decode_base64_body(data);
    }

    // Check parts for text/plain
    if let Some(parts) = &payload.parts
        && let Some(text) = find_plain_text_in_parts(parts)
    {
        return Some(text);
    }

    // Fall back to any text content
    if let Some(body) = &payload.body
        && let Some(data) = &body.data
    {
        return decode_base64_body(data);
    }

    None
}

/// Recursively search message parts for text/plain content
fn find_plain_text_in_parts(parts: &[MessagePart]) -> Option<String> {
    for part in parts {
        // Check if this part is text/plain
        if part
            .mime_type
            .as_ref()
            .is_some_and(|m| m.starts_with("text/plain"))
            && let Some(body) = &part.body
            && let Some(data) = &body.data
            && let Some(text) = decode_base64_body(data)
        {
            return Some(text);
        }

        // Recursively check nested parts
        if let Some(nested) = &part.parts
            && let Some(text) = find_plain_text_in_parts(nested)
        {
            return Some(text);
        }
    }

    None
}

/// Extract HTML body from message payload
fn extract_html_body(payload: &MessagePayload) -> Option<String> {
    // Check if this is a simple message with HTML body
    if let Some(body) = &payload.body
        && let Some(data) = &body.data
        && payload
            .mime_type
            .as_ref()
            .is_some_and(|m| m.starts_with("text/html"))
    {
        return decode_base64_body(data);
    }

    // Check parts for text/html
    if let Some(parts) = &payload.parts {
        if let Some(html) = find_html_in_parts(parts) {
            return Some(html);
        }
    }

    None
}

/// Recursively search message parts for text/html content
fn find_html_in_parts(parts: &[MessagePart]) -> Option<String> {
    for part in parts {
        // Check if this part is text/html
        if part
            .mime_type
            .as_ref()
            .is_some_and(|m| m.starts_with("text/html"))
            && let Some(body) = &part.body
            && let Some(data) = &body.data
            && let Some(html) = decode_base64_body(data)
        {
            return Some(html);
        }

        // Recursively check nested parts
        if let Some(nested) = &part.parts
            && let Some(html) = find_html_in_parts(nested)
        {
            return Some(html);
        }
    }

    None
}

/// Decode base64-encoded body data
///
/// Gmail uses URL-safe base64 but padding can vary, so we try multiple decoders.
fn decode_base64_body(data: &str) -> Option<String> {
    use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD, URL_SAFE};

    let decoders: &[&base64::engine::GeneralPurpose] =
        &[&BASE64_URL_SAFE_NO_PAD, &URL_SAFE, &STANDARD, &STANDARD_NO_PAD];

    for decoder in decoders {
        if let Ok(decoded) = decoder.decode(data) {
            if let Ok(s) = String::from_utf8(decoded) {
                return Some(s);
            }
        }
    }

    None
}

/// Decode HTML entities in snippet text
fn decode_html_entities(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gmail::api::{Header, MessageBody};

    fn make_test_payload(headers: Vec<(&str, &str)>) -> MessagePayload {
        MessagePayload {
            headers: Some(
                headers
                    .into_iter()
                    .map(|(n, v)| Header {
                        name: n.to_string(),
                        value: v.to_string(),
                    })
                    .collect(),
            ),
            body: Some(MessageBody {
                size: Some(0),
                data: None,
            }),
            parts: None,
            mime_type: Some("text/plain".to_string()),
        }
    }

    #[test]
    fn test_extract_header() {
        let payload = make_test_payload(vec![
            ("From", "test@example.com"),
            ("Subject", "Test Subject"),
        ]);

        assert_eq!(
            extract_header(&payload, "From"),
            Some("test@example.com".to_string())
        );
        assert_eq!(
            extract_header(&payload, "Subject"),
            Some("Test Subject".to_string())
        );
        assert_eq!(extract_header(&payload, "Cc"), None);
    }

    #[test]
    fn test_extract_header_case_insensitive() {
        let payload = make_test_payload(vec![("FROM", "test@example.com")]);
        assert_eq!(
            extract_header(&payload, "from"),
            Some("test@example.com".to_string())
        );
    }

    #[test]
    fn test_parse_address_list() {
        let addrs = parse_address_list("alice@example.com, Bob <bob@example.com>");
        assert_eq!(addrs.len(), 2);
        assert_eq!(addrs[0].email, "alice@example.com");
        assert_eq!(addrs[1].email, "bob@example.com");
        assert_eq!(addrs[1].name, Some("Bob".to_string()));
    }

    #[test]
    fn test_decode_html_entities() {
        let input = "Hello &amp; welcome &lt;user&gt;";
        let output = decode_html_entities(input);
        assert_eq!(output, "Hello & welcome <user>");
    }

    #[test]
    fn test_decode_base64_body() {
        // "Hello, World!" in base64url
        let encoded = "SGVsbG8sIFdvcmxkIQ";
        let decoded = decode_base64_body(encoded);
        assert_eq!(decoded, Some("Hello, World!".to_string()));
    }
}
