//! Tantivy schema definition for email indexing

use tantivy::schema::{
    Field, IndexRecordOption, Schema, TextFieldIndexing, TextOptions, FAST, STORED, STRING,
};

/// Build the Tantivy schema for email indexing
///
/// Fields indexed:
/// - thread_id, message_id: String IDs for retrieval
/// - subject, body_text, snippet: Full-text searchable content
/// - from, from_email, to, cc: Sender/recipient search
/// - labels: Exact match label filtering
/// - received_at_ms: Date range queries
/// - is_unread, is_starred, has_attachment: Boolean filters
pub fn build_schema() -> Schema {
    let mut builder = Schema::builder();

    // ID fields (stored for retrieval, STRING for exact match)
    builder.add_text_field("thread_id", STRING | STORED);
    builder.add_text_field("message_id", STRING | STORED);

    // Full-text fields with positions for phrase queries and highlighting
    let text_opts = TextOptions::default()
        .set_indexing_options(
            TextFieldIndexing::default()
                .set_index_option(IndexRecordOption::WithFreqsAndPositions)
                .set_tokenizer("default"),
        )
        .set_stored();

    builder.add_text_field("subject", text_opts.clone());
    builder.add_text_field("body_text", text_opts.clone());
    builder.add_text_field("snippet", text_opts.clone());

    // Sender/recipient fields (full-text for name search)
    builder.add_text_field("from", text_opts.clone());
    builder.add_text_field("from_email", text_opts.clone());
    builder.add_text_field("to", text_opts.clone());
    builder.add_text_field("cc", text_opts);

    // Exact match fields for label filtering (multi-valued via multiple additions)
    builder.add_text_field("labels", STRING);

    // Numeric fields for filtering (FAST for range queries)
    builder.add_i64_field("received_at_ms", FAST | STORED);
    builder.add_u64_field("is_unread", FAST);
    builder.add_u64_field("is_starred", FAST);
    builder.add_u64_field("has_attachment", FAST);

    builder.build()
}

/// Field handles for quick access during indexing and searching
pub struct SchemaFields {
    pub thread_id: Field,
    pub message_id: Field,
    pub subject: Field,
    pub body_text: Field,
    pub snippet: Field,
    pub from: Field,
    pub from_email: Field,
    pub to: Field,
    pub cc: Field,
    pub labels: Field,
    pub received_at_ms: Field,
    pub is_unread: Field,
    pub is_starred: Field,
    pub has_attachment: Field,
}

impl SchemaFields {
    /// Create field handles from a schema
    pub fn new(schema: &Schema) -> Self {
        Self {
            thread_id: schema.get_field("thread_id").expect("thread_id field"),
            message_id: schema.get_field("message_id").expect("message_id field"),
            subject: schema.get_field("subject").expect("subject field"),
            body_text: schema.get_field("body_text").expect("body_text field"),
            snippet: schema.get_field("snippet").expect("snippet field"),
            from: schema.get_field("from").expect("from field"),
            from_email: schema.get_field("from_email").expect("from_email field"),
            to: schema.get_field("to").expect("to field"),
            cc: schema.get_field("cc").expect("cc field"),
            labels: schema.get_field("labels").expect("labels field"),
            received_at_ms: schema.get_field("received_at_ms").expect("received_at_ms field"),
            is_unread: schema.get_field("is_unread").expect("is_unread field"),
            is_starred: schema.get_field("is_starred").expect("is_starred field"),
            has_attachment: schema.get_field("has_attachment").expect("has_attachment field"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schema_creation() {
        let schema = build_schema();
        let fields = SchemaFields::new(&schema);

        // Verify all fields exist
        assert!(schema.get_field("thread_id").is_ok());
        assert!(schema.get_field("message_id").is_ok());
        assert!(schema.get_field("subject").is_ok());
        assert!(schema.get_field("body_text").is_ok());
        assert!(schema.get_field("snippet").is_ok());
        assert!(schema.get_field("from").is_ok());
        assert!(schema.get_field("from_email").is_ok());
        assert!(schema.get_field("to").is_ok());
        assert!(schema.get_field("cc").is_ok());
        assert!(schema.get_field("labels").is_ok());
        assert!(schema.get_field("received_at_ms").is_ok());
        assert!(schema.get_field("is_unread").is_ok());
        assert!(schema.get_field("is_starred").is_ok());
        assert!(schema.get_field("has_attachment").is_ok());

        // Verify SchemaFields matches
        assert_eq!(fields.thread_id, schema.get_field("thread_id").unwrap());
    }
}
