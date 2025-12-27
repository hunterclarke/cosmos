//! Search index implementation using Tantivy

use std::collections::HashSet;
use std::ops::Bound;
use std::path::Path;
use std::sync::RwLock;

use anyhow::{Context, Result};
use tantivy::collector::TopDocs;
use tantivy::directory::MmapDirectory;
use tantivy::query::{BooleanQuery, Occur, Query, QueryParser, RangeQuery, TermQuery};
use tantivy::schema::{IndexRecordOption, Schema, Term, Value};
use tantivy::{Index, IndexReader, IndexWriter, ReloadPolicy, TantivyDocument};

use crate::models::{Message, Thread, ThreadId};
use crate::storage::MailStore;

use super::query_parser::ParsedQuery;
use super::schema::{build_schema, SchemaFields};
use super::{FieldHighlight, HighlightSpan, SearchResult};

/// Default heap size for index writer (50MB)
const DEFAULT_HEAP_SIZE: usize = 50_000_000;

/// Thread-safe search index wrapper
pub struct SearchIndex {
    index: Index,
    reader: IndexReader,
    #[allow(dead_code)]
    schema: Schema,
    fields: SchemaFields,
    /// Writer is wrapped in RwLock for thread-safe access
    writer: RwLock<Option<IndexWriter>>,
}

impl std::fmt::Debug for SearchIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SearchIndex")
            .field("index", &"<tantivy::Index>")
            .finish()
    }
}

impl SearchIndex {
    /// Open or create index at the given path
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        std::fs::create_dir_all(path).context("Failed to create index directory")?;

        let schema = build_schema();
        let dir = MmapDirectory::open(path).context("Failed to open index directory")?;

        let index =
            Index::open_or_create(dir, schema.clone()).context("Failed to open or create index")?;

        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()
            .context("Failed to create index reader")?;

        let fields = SchemaFields::new(&schema);

        Ok(Self {
            index,
            reader,
            schema,
            fields,
            writer: RwLock::new(None),
        })
    }

    /// Create an in-memory index (for testing)
    pub fn in_memory() -> Result<Self> {
        let schema = build_schema();
        let index = Index::create_in_ram(schema.clone());

        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::Manual)
            .try_into()?;

        let fields = SchemaFields::new(&schema);

        Ok(Self {
            index,
            reader,
            schema,
            fields,
            writer: RwLock::new(None),
        })
    }

    /// Get or create a writer with the given heap size
    fn get_writer(&self) -> Result<std::sync::RwLockWriteGuard<'_, Option<IndexWriter>>> {
        let mut guard = self.writer.write().map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?;
        if guard.is_none() {
            *guard = Some(self.index.writer(DEFAULT_HEAP_SIZE)?);
        }
        Ok(guard)
    }

    /// Index a single message
    ///
    /// This implements upsert semantics - if a message with the same ID exists,
    /// it will be replaced.
    pub fn index_message(&self, message: &Message, thread: &Thread) -> Result<()> {
        let mut writer_guard = self.get_writer()?;
        let writer = writer_guard.as_mut().unwrap();

        // Delete existing document for this message (upsert semantics)
        writer.delete_term(Term::from_field_text(
            self.fields.message_id,
            message.id.as_str(),
        ));

        // Build document
        let mut doc = TantivyDocument::new();

        // IDs
        doc.add_text(self.fields.thread_id, thread.id.as_str());
        doc.add_text(self.fields.message_id, message.id.as_str());

        // Account ID for multi-account filtering
        doc.add_i64(self.fields.account_id, thread.account_id);

        // Text content
        doc.add_text(self.fields.subject, &message.subject);
        if let Some(ref body) = message.body_text {
            doc.add_text(self.fields.body_text, body);
        }
        doc.add_text(self.fields.snippet, &message.body_preview);

        // Sender
        if let Some(ref name) = message.from.name {
            doc.add_text(self.fields.from, name);
        }
        doc.add_text(self.fields.from_email, &message.from.email);

        // Recipients
        for to in &message.to {
            let display = to.display();
            doc.add_text(self.fields.to, &display);
        }
        for cc in &message.cc {
            let display = cc.display();
            doc.add_text(self.fields.cc, &display);
        }

        // Labels (each label as separate field value)
        for label in &message.label_ids {
            doc.add_text(self.fields.labels, label);
        }

        // Numeric fields
        doc.add_i64(
            self.fields.received_at_ms,
            message.received_at.timestamp_millis(),
        );
        doc.add_u64(
            self.fields.is_unread,
            if message.label_ids.iter().any(|l| l == "UNREAD") {
                1
            } else {
                0
            },
        );
        doc.add_u64(
            self.fields.is_starred,
            if message.label_ids.iter().any(|l| l == "STARRED") {
                1
            } else {
                0
            },
        );
        // TODO: Detect attachments from message headers/parts
        doc.add_u64(self.fields.has_attachment, 0);

        writer.add_document(doc)?;
        Ok(())
    }

    /// Delete all documents for a thread
    pub fn delete_thread(&self, thread_id: &ThreadId) -> Result<()> {
        let mut writer_guard = self.get_writer()?;
        let writer = writer_guard.as_mut().unwrap();

        writer.delete_term(Term::from_field_text(
            self.fields.thread_id,
            thread_id.as_str(),
        ));
        Ok(())
    }

    /// Commit pending changes
    pub fn commit(&self) -> Result<()> {
        let mut writer_guard = self.writer.write().map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?;
        if let Some(ref mut writer) = *writer_guard {
            writer.commit()?;
        }
        self.reader.reload()?;
        Ok(())
    }

    /// Clear all documents from the index
    pub fn clear(&self) -> Result<()> {
        let mut writer_guard = self.get_writer()?;
        let writer = writer_guard.as_mut().unwrap();
        writer.delete_all_documents()?;
        writer.commit()?;
        self.reader.reload()?;
        Ok(())
    }

    /// Search for threads matching the query
    ///
    /// Returns deduplicated results by thread_id, sorted by relevance score.
    /// If `account_id` is Some, only returns results from that account.
    pub fn search(
        &self,
        query: &ParsedQuery,
        limit: usize,
        store: &dyn MailStore,
        account_id: Option<i64>,
    ) -> Result<Vec<SearchResult>> {
        let searcher = self.reader.searcher();

        // Build Tantivy query from ParsedQuery
        let tantivy_query = self.build_query(query, account_id)?;

        // Execute search - fetch extra to account for deduplication
        let top_docs = searcher.search(&tantivy_query, &TopDocs::with_limit(limit * 3))?;

        // Deduplicate by thread_id and build results
        let mut seen_threads = HashSet::new();
        let mut results = Vec::with_capacity(limit);

        for (score, doc_address) in top_docs {
            let doc: TantivyDocument = searcher.doc(doc_address)?;

            // Extract thread_id
            let thread_id_str = doc
                .get_first(self.fields.thread_id)
                .and_then(|v| v.as_str())
                .unwrap_or_default();

            let thread_id = ThreadId::new(thread_id_str);

            // Skip if we've already seen this thread
            if !seen_threads.insert(thread_id.clone()) {
                continue;
            }

            // Load thread from store for full metadata
            if let Ok(Some(thread)) = store.get_thread(&thread_id) {
                // Generate highlights
                let highlights = self.generate_highlights(&doc, query);

                results.push(SearchResult {
                    thread_id,
                    subject: thread.subject,
                    snippet: thread.snippet,
                    last_message_at: thread.last_message_at,
                    message_count: thread.message_count,
                    sender_name: thread.sender_name,
                    sender_email: thread.sender_email,
                    is_unread: thread.is_unread,
                    highlights,
                    score,
                });
            }

            if results.len() >= limit {
                break;
            }
        }

        Ok(results)
    }

    /// Build a Tantivy query from ParsedQuery
    fn build_query(&self, query: &ParsedQuery, account_id: Option<i64>) -> Result<Box<dyn Query>> {
        let mut clauses: Vec<(Occur, Box<dyn Query>)> = Vec::new();

        // Account filter
        if let Some(id) = account_id {
            let term = Term::from_field_i64(self.fields.account_id, id);
            clauses.push((
                Occur::Must,
                Box::new(TermQuery::new(term, IndexRecordOption::Basic)),
            ));
        }

        // Free-text terms - search across multiple fields
        if !query.terms.is_empty() {
            let query_text = query.terms.join(" ");
            let parser = QueryParser::for_index(
                &self.index,
                vec![
                    self.fields.subject,
                    self.fields.body_text,
                    self.fields.snippet,
                    self.fields.from,
                    self.fields.from_email,
                ],
            );
            if let Ok(text_query) = parser.parse_query(&query_text) {
                clauses.push((Occur::Must, text_query));
            }
        }

        // from: filter - search in both from and from_email fields
        for from_val in &query.from {
            let from_val_lower = from_val.to_lowercase();
            let parser = QueryParser::for_index(
                &self.index,
                vec![self.fields.from, self.fields.from_email],
            );
            if let Ok(from_query) = parser.parse_query(&from_val_lower) {
                clauses.push((Occur::Must, from_query));
            }
        }

        // to: filter
        for to_val in &query.to {
            let to_val_lower = to_val.to_lowercase();
            let parser = QueryParser::for_index(&self.index, vec![self.fields.to]);
            if let Ok(to_query) = parser.parse_query(&to_val_lower) {
                clauses.push((Occur::Must, to_query));
            }
        }

        // subject: filter
        for subj_val in &query.subject {
            let parser = QueryParser::for_index(&self.index, vec![self.fields.subject]);
            if let Ok(subj_query) = parser.parse_query(subj_val) {
                clauses.push((Occur::Must, subj_query));
            }
        }

        // in:label filter
        if let Some(ref label) = query.in_label {
            let term = Term::from_field_text(self.fields.labels, label);
            clauses.push((
                Occur::Must,
                Box::new(TermQuery::new(term, IndexRecordOption::Basic)),
            ));
        }

        // is:unread filter
        if let Some(is_unread) = query.is_unread {
            let val = if is_unread { 1u64 } else { 0u64 };
            let term = Term::from_field_u64(self.fields.is_unread, val);
            clauses.push((
                Occur::Must,
                Box::new(TermQuery::new(term, IndexRecordOption::Basic)),
            ));
        }

        // is:starred filter
        if let Some(is_starred) = query.is_starred {
            let val = if is_starred { 1u64 } else { 0u64 };
            let term = Term::from_field_u64(self.fields.is_starred, val);
            clauses.push((
                Occur::Must,
                Box::new(TermQuery::new(term, IndexRecordOption::Basic)),
            ));
        }

        // has:attachment filter
        if let Some(has_attachment) = query.has_attachment {
            let val = if has_attachment { 1u64 } else { 0u64 };
            let term = Term::from_field_u64(self.fields.has_attachment, val);
            clauses.push((
                Occur::Must,
                Box::new(TermQuery::new(term, IndexRecordOption::Basic)),
            ));
        }

        // Date range filters (before:/after:)
        if let Some(ref before) = query.before {
            let before_ms = before.timestamp_millis();
            let upper_term = Term::from_field_i64(self.fields.received_at_ms, before_ms);
            let range = RangeQuery::new(Bound::Unbounded, Bound::Excluded(upper_term));
            clauses.push((Occur::Must, Box::new(range)));
        }

        if let Some(ref after) = query.after {
            let after_ms = after.timestamp_millis();
            let lower_term = Term::from_field_i64(self.fields.received_at_ms, after_ms);
            let range = RangeQuery::new(Bound::Included(lower_term), Bound::Unbounded);
            clauses.push((Occur::Must, Box::new(range)));
        }

        // Combine all clauses
        if clauses.is_empty() {
            // Match all if no constraints
            Ok(Box::new(tantivy::query::AllQuery))
        } else {
            Ok(Box::new(BooleanQuery::new(clauses)))
        }
    }

    /// Generate highlights for search results
    ///
    /// Currently returns simplified highlights based on query terms.
    /// Full implementation would use Tantivy's snippet generator.
    fn generate_highlights(&self, doc: &TantivyDocument, query: &ParsedQuery) -> Vec<FieldHighlight> {
        let mut highlights = Vec::new();

        // Get the subject and snippet from the document
        let subject = doc
            .get_first(self.fields.subject)
            .and_then(|v| v.as_str())
            .unwrap_or_default();

        let snippet = doc
            .get_first(self.fields.snippet)
            .and_then(|v| v.as_str())
            .unwrap_or_default();

        // Find matches for each query term
        for term in &query.terms {
            let term_lower = term.to_lowercase();

            // Check subject
            let subject_lower = subject.to_lowercase();
            if let Some(pos) = subject_lower.find(&term_lower) {
                highlights.push(FieldHighlight {
                    field: "subject".to_string(),
                    text: subject.to_string(),
                    highlights: vec![HighlightSpan {
                        start: pos,
                        end: pos + term.len(),
                    }],
                });
            }

            // Check snippet
            let snippet_lower = snippet.to_lowercase();
            if let Some(pos) = snippet_lower.find(&term_lower) {
                highlights.push(FieldHighlight {
                    field: "snippet".to_string(),
                    text: snippet.to_string(),
                    highlights: vec![HighlightSpan {
                        start: pos,
                        end: pos + term.len(),
                    }],
                });
            }
        }

        highlights
    }

    /// Rebuild entire index from storage
    ///
    /// Clears the existing index and re-indexes all messages from the store.
    /// Returns the number of messages indexed.
    pub fn rebuild(&self, store: &dyn MailStore) -> Result<usize> {
        // Clear existing index
        {
            let mut writer_guard = self.get_writer()?;
            let writer = writer_guard.as_mut().unwrap();
            writer.delete_all_documents()?;
            writer.commit()?;
        }

        let mut count = 0;
        let threads = store.list_threads(100_000, 0)?;

        for thread in threads {
            let messages = store.list_messages_for_thread_with_bodies(&thread.id)?;
            for message in &messages {
                self.index_message(message, &thread)?;
                count += 1;
            }
        }

        self.commit()?;
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{EmailAddress, Message, MessageId};
    use crate::storage::InMemoryMailStore;
    use chrono::Utc;

    fn create_test_message(id: &str, thread_id: &str, subject: &str, body: &str) -> Message {
        Message::builder(MessageId::new(id), ThreadId::new(thread_id))
            .from(EmailAddress::new("sender@example.com"))
            .subject(subject)
            .body_preview(body)
            .body_text(Some(body.to_string()))
            .received_at(Utc::now())
            .internal_date(Utc::now().timestamp_millis())
            .label_ids(vec!["INBOX".to_string()])
            .build()
    }

    fn create_test_thread(id: &str, subject: &str) -> Thread {
        Thread {
            id: ThreadId::new(id),
            account_id: 1,
            subject: subject.to_string(),
            snippet: "Test snippet".to_string(),
            last_message_at: Utc::now(),
            message_count: 1,
            sender_name: Some("Sender".to_string()),
            sender_email: "sender@example.com".to_string(),
            is_unread: false,
        }
    }

    #[test]
    fn test_index_and_search() -> Result<()> {
        let index = SearchIndex::in_memory()?;
        let store = InMemoryMailStore::new();

        // Create test data
        let thread = create_test_thread("thread1", "Meeting tomorrow");
        let message = create_test_message("msg1", "thread1", "Meeting tomorrow", "Let's discuss the project");

        store.upsert_thread(thread.clone())?;
        store.upsert_message(message.clone())?;

        // Index the message
        index.index_message(&message, &thread)?;
        index.commit()?;

        // Search for it
        let query = super::super::parse_query("meeting");
        let results = index.search(&query, 10, &store, None)?;

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].thread_id.as_str(), "thread1");
        assert_eq!(results[0].subject, "Meeting tomorrow");

        Ok(())
    }

    #[test]
    fn test_search_with_from_filter() -> Result<()> {
        let index = SearchIndex::in_memory()?;
        let store = InMemoryMailStore::new();

        let thread = create_test_thread("thread1", "Hello");
        let mut message = create_test_message("msg1", "thread1", "Hello", "Test body");
        message.from = EmailAddress::with_name("Alice", "alice@example.com");

        store.upsert_thread(thread.clone())?;
        store.upsert_message(message.clone())?;
        index.index_message(&message, &thread)?;
        index.commit()?;

        // Search by from
        let query = super::super::parse_query("from:alice");
        let results = index.search(&query, 10, &store, None)?;
        assert_eq!(results.len(), 1);

        // Search by different sender (no results)
        let query2 = super::super::parse_query("from:bob");
        let results2 = index.search(&query2, 10, &store, None)?;
        assert_eq!(results2.len(), 0);

        Ok(())
    }

    #[test]
    fn test_search_with_label_filter() -> Result<()> {
        let index = SearchIndex::in_memory()?;
        let store = InMemoryMailStore::new();

        let thread = create_test_thread("thread1", "Test");
        let mut message = create_test_message("msg1", "thread1", "Test", "Body");
        message.label_ids = vec!["INBOX".to_string(), "IMPORTANT".to_string()];

        store.upsert_thread(thread.clone())?;
        store.upsert_message(message.clone())?;
        index.index_message(&message, &thread)?;
        index.commit()?;

        // Search in:inbox
        let query = super::super::parse_query("in:inbox");
        let results = index.search(&query, 10, &store, None)?;
        assert_eq!(results.len(), 1);

        // Search in:sent (no results)
        let query2 = super::super::parse_query("in:sent");
        let results2 = index.search(&query2, 10, &store, None)?;
        assert_eq!(results2.len(), 0);

        Ok(())
    }

    #[test]
    fn test_search_deduplication() -> Result<()> {
        let index = SearchIndex::in_memory()?;
        let store = InMemoryMailStore::new();

        // Create thread with multiple messages
        let thread = create_test_thread("thread1", "Discussion");
        let msg1 = create_test_message("msg1", "thread1", "Discussion", "First message about project");
        let msg2 = create_test_message("msg2", "thread1", "Re: Discussion", "Second message about project");

        store.upsert_thread(thread.clone())?;
        store.upsert_message(msg1.clone())?;
        store.upsert_message(msg2.clone())?;
        index.index_message(&msg1, &thread)?;
        index.index_message(&msg2, &thread)?;
        index.commit()?;

        // Search should return only one result (deduplicated by thread)
        let query = super::super::parse_query("project");
        let results = index.search(&query, 10, &store, None)?;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].thread_id.as_str(), "thread1");

        Ok(())
    }

    #[test]
    fn test_delete_thread() -> Result<()> {
        let index = SearchIndex::in_memory()?;
        let store = InMemoryMailStore::new();

        let thread = create_test_thread("thread1", "Test");
        let message = create_test_message("msg1", "thread1", "Test", "Body");

        store.upsert_thread(thread.clone())?;
        store.upsert_message(message.clone())?;
        index.index_message(&message, &thread)?;
        index.commit()?;

        // Verify it's indexed
        let query = super::super::parse_query("test");
        let results = index.search(&query, 10, &store, None)?;
        assert_eq!(results.len(), 1);

        // Delete the thread
        index.delete_thread(&thread.id)?;
        index.commit()?;

        // Verify it's gone
        let results2 = index.search(&query, 10, &store, None)?;
        assert_eq!(results2.len(), 0);

        Ok(())
    }

    #[test]
    fn test_rebuild() -> Result<()> {
        let index = SearchIndex::in_memory()?;
        let store = InMemoryMailStore::new();

        // Add data to store
        let thread = create_test_thread("thread1", "Rebuild test");
        let message = create_test_message("msg1", "thread1", "Rebuild test", "Content");

        store.upsert_thread(thread)?;
        store.upsert_message(message)?;

        // Rebuild index
        let count = index.rebuild(&store)?;
        assert_eq!(count, 1);

        // Verify search works
        let query = super::super::parse_query("rebuild");
        let results = index.search(&query, 10, &store, None)?;
        assert_eq!(results.len(), 1);

        Ok(())
    }
}
