# Phase 3: Full-Text Search — Implementation Plan

**Project**: COSMOS / ORION v0.3
**Status**: Implemented
**Depends On**: Phase 2 Complete

---

## Executive Summary

Implement Gmail/Superhuman-style full-text search using **Tantivy** (pure Rust, embedded search engine). This phase adds instant search with query operators, highlighted results, and keyboard-driven navigation.

**Phase Goal**: Fast, feature-rich full-text search with Gmail operator support
**Phase Status**: Read-only (still no mutations)

---

## 1. Goals & Non-Goals

### In Scope
- Full-text search across subject, body, sender, recipients
- **Search operators**: `from:`, `to:`, `subject:`, `is:unread`, `is:starred`, `has:attachment`, `in:inbox`, `before:`, `after:`
- Instant search results (150ms debounce)
- **Highlighted matches** in search results
- **Keyboard navigation** (`/` to focus, `j/k` to navigate, `Enter` to open)
- Persistent search index at `~/.config/cosmos/mail.search.idx`
- Incremental indexing during sync

### Out of Scope (Future Phases)
- Send / archive / delete / label mail
- Semantic search (embeddings)
- Search suggestions / autocomplete
- Saved searches
- Multi-account search

---

## 2. Architectural Constraints (Reaffirmed)

### mail Crate (Functional Core)
- Owns search index and query logic
- No direct disk I/O (index path passed in)
- UniFFI-safe APIs
- Zero UI dependencies

### Search Library
- **Tantivy** (pure Rust, embeddable)
- Sub-millisecond queries for 10K-100K messages
- Built-in tokenization and highlighting support
- No external server required

---

## 3. New Concepts Introduced in Phase 3

### 3.1 Search Index

A Tantivy index stored separately from the mail database. Indexed fields include subject, body, sender, recipients, labels, and timestamps.

### 3.2 Query Operators

Gmail-style search operators parsed from the query string:
- `from:john@example.com` - sender filter
- `to:jane@example.com` - recipient filter
- `subject:meeting` - subject filter
- `is:unread`, `is:starred` - boolean filters
- `has:attachment` - attachment filter
- `in:inbox`, `in:sent` - label filter
- `before:2024/12/01`, `after:2024/01/01` - date range

### 3.3 Search Results with Highlights

Search results include match positions for UI highlighting:
```rust
pub struct SearchResult {
    thread_id: ThreadId,
    subject: String,
    snippet: String,
    highlights: Vec<FieldHighlight>,
    score: f32,
    // ... thread metadata
}
```

---

## 4. Data Model (Additions)

### 4.1 Search Result Types

```rust
// mail/src/search/mod.rs

/// A highlighted text span
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HighlightSpan {
    pub start: usize,
    pub end: usize,
}

/// Match highlights for a specific field
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldHighlight {
    pub field: String,
    pub text: String,
    pub highlights: Vec<HighlightSpan>,
}

/// A single search result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub thread_id: ThreadId,
    pub subject: String,
    pub snippet: String,
    pub last_message_at: DateTime<Utc>,
    pub message_count: usize,
    pub sender_name: Option<String>,
    pub sender_email: String,
    pub is_unread: bool,
    pub highlights: Vec<FieldHighlight>,
    pub score: f32,
}
```

### 4.2 Parsed Query

```rust
// mail/src/search/mod.rs

/// Search query with parsed operators
#[derive(Debug, Clone, Default)]
pub struct ParsedQuery {
    pub terms: Vec<String>,        // Free-text terms
    pub from: Vec<String>,         // from: values
    pub to: Vec<String>,           // to: values
    pub subject: Vec<String>,      // subject: values
    pub in_label: Option<String>,  // in: value
    pub is_unread: Option<bool>,   // is:unread / is:read
    pub is_starred: Option<bool>,  // is:starred
    pub has_attachment: Option<bool>, // has:attachment
    pub before: Option<DateTime<Utc>>, // before: date
    pub after: Option<DateTime<Utc>>,  // after: date
}
```

---

## 5. Tantivy Schema

### 5.1 Index Fields

```rust
// mail/src/search/schema.rs

pub fn build_schema() -> Schema {
    let mut schema_builder = Schema::builder();

    // IDs (stored for retrieval)
    schema_builder.add_text_field("thread_id", STRING | STORED);
    schema_builder.add_text_field("message_id", STRING | STORED);

    // Full-text fields (with positions for highlighting)
    let text_options = TextOptions::default()
        .set_indexing_options(
            TextFieldIndexing::default()
                .set_index_option(IndexRecordOption::WithFreqsAndPositions)
        )
        .set_stored();

    schema_builder.add_text_field("subject", text_options.clone());
    schema_builder.add_text_field("body_text", text_options.clone());
    schema_builder.add_text_field("snippet", text_options.clone());
    schema_builder.add_text_field("sender_name", text_options.clone());
    schema_builder.add_text_field("sender_email", text_options.clone());
    schema_builder.add_text_field("from", text_options.clone());
    schema_builder.add_text_field("to", text_options.clone());
    schema_builder.add_text_field("cc", text_options);

    // Exact match fields
    schema_builder.add_text_field("labels", STRING);

    // Numeric fields for filtering
    schema_builder.add_i64_field("received_at_ms", FAST | STORED);
    schema_builder.add_u64_field("is_unread", FAST);
    schema_builder.add_u64_field("is_starred", FAST);
    schema_builder.add_u64_field("has_attachment", FAST);

    schema_builder.build()
}
```

---

## 6. Search Index Implementation

### 6.1 SearchIndex Struct

```rust
// mail/src/search/index.rs

pub struct SearchIndex {
    index: Index,
    reader: IndexReader,
    schema: Schema,
}

impl SearchIndex {
    /// Open or create index at path
    pub fn open(path: impl AsRef<Path>) -> Result<Self>;

    /// Create in-memory index (for testing)
    pub fn in_memory() -> Result<Self>;

    /// Get a writer for indexing
    pub fn writer(&self, heap_size: usize) -> Result<IndexWriter>;

    /// Index a single message
    pub fn index_message(
        &self,
        writer: &mut IndexWriter,
        message: &Message,
        thread: &Thread,
    ) -> Result<()>;

    /// Delete all documents for a thread
    pub fn delete_thread(
        &self,
        writer: &mut IndexWriter,
        thread_id: &ThreadId,
    ) -> Result<()>;

    /// Search for threads
    pub fn search(
        &self,
        query: &ParsedQuery,
        limit: usize,
        store: &dyn MailStore,
    ) -> Result<Vec<SearchResult>>;

    /// Rebuild entire index from storage
    pub fn rebuild(&self, store: &dyn MailStore) -> Result<usize>;
}
```

### 6.2 Query Building

The search method builds a Tantivy `BooleanQuery` from the `ParsedQuery`:
- Free-text terms → multi-field text query (subject, body, snippet, sender)
- `from:` → term query on `from` field
- `to:` → term query on `to` field
- `subject:` → term query on `subject` field
- `in:` → term query on `labels` field
- `is:unread/starred` → term query on boolean fields
- `before:/after:` → range query on `received_at_ms`

---

## 7. Query Parser

### 7.1 Parser Implementation

```rust
// mail/src/search/query_parser.rs

/// Parse a search query string into structured components
pub fn parse_query(input: &str) -> ParsedQuery {
    // Parses operators and free-text terms
    // Supports quoted values: from:"John Doe"
    // Normalizes label names: inbox → INBOX
}
```

### 7.2 Supported Operators

| Operator | Example | Description |
|----------|---------|-------------|
| `from:` | `from:john@example.com` | Sender email/name |
| `to:` | `to:team@company.com` | Recipient |
| `subject:` | `subject:meeting` | Subject line |
| `in:` | `in:inbox`, `in:sent` | Label filter |
| `is:unread` | `is:unread` | Unread messages |
| `is:read` | `is:read` | Read messages |
| `is:starred` | `is:starred` | Starred messages |
| `has:attachment` | `has:attachment` | Has attachments |
| `before:` | `before:2024/12/01` | Before date |
| `after:` | `after:2024/01/01` | After date |

### 7.3 Date Formats

Supported: `YYYY/MM/DD` and `YYYY-MM-DD`

---

## 8. Public API

### 8.1 Module Exports

```rust
// mail/src/lib.rs

pub mod search;

pub use search::{
    SearchIndex,
    SearchResult,
    ParsedQuery,
    FieldHighlight,
    HighlightSpan,
    parse_query,
    search_threads,
};
```

### 8.2 High-Level Search Function

```rust
// mail/src/search/mod.rs

/// Search threads by query string
pub fn search_threads(
    index: &SearchIndex,
    store: &dyn MailStore,
    query: &str,
    limit: usize,
) -> Result<Vec<SearchResult>> {
    let parsed = parse_query(query);
    index.search(&parsed, limit, store)
}
```

---

## 9. Sync Integration

### 9.1 Incremental Indexing

During sync, index messages as they are stored:

```rust
// mail/src/sync/inbox.rs

pub struct SyncOptions {
    pub max_messages: Option<usize>,
    pub full_resync: bool,
    pub label_filter: Option<String>,
    pub search_index: Option<Arc<SearchIndex>>, // NEW
}
```

After storing each message:
```rust
if let Some(index) = &options.search_index {
    if let Ok(mut writer) = index.writer(10_000_000) {
        let _ = index.index_message(&mut writer, &message, &thread);
        let _ = writer.commit();
    }
}
```

### 9.2 Index Rebuild

On first launch or if index is corrupted:
```rust
let indexed = search_index.rebuild(&store)?;
println!("Indexed {} messages", indexed);
```

---

## 10. UI: Search Components

### 10.1 Search Box

```rust
// orion/src/components/search_box.rs

pub struct SearchBox {
    input_state: Entity<InputState>,  // Uses gpui-component Input
    focus_handle: FocusHandle,
    debounce_task: Option<Task<()>>,
}

pub enum SearchBoxEvent {
    QueryChanged(String),  // Debounced, 150ms
    Submitted(String),     // Enter pressed
    Cleared,               // X clicked or cleared
    Cancelled,             // Escape pressed
}
```

**Implementation Notes:**
- Uses `gpui-component::input::Input` component (requires `Root` wrapper)
- Debounce implemented with `cx.spawn()` and `cx.background_executor().timer()`
- Search icon from `gpui_component::IconName::Search`
- Positioned in top-right of header

**Features:**
- Placeholder: "Search mail..."
- Focus with `/` or `Cmd+F`
- Search icon prefix
- Enter key submits and focuses results

### 10.2 Search Result Item

```rust
// orion/src/components/search_result_item.rs

#[derive(IntoElement)]
pub struct SearchResultItem {
    result: SearchResult,
    is_selected: bool,
    query_terms: Vec<String>,  // For highlighting
}
```

**Layout (100px height):**
```
┌─────────────────────────────────────────────────────┐
│ ● │ Sender Name                              │ Date │
│   │ Subject with [highlighted] matches       │ (3)  │
│   │ Snippet with [highlighted] matches...          │
└─────────────────────────────────────────────────────┘
```

**Highlighting Implementation:**
- Uses GPUI's `StyledText::with_highlights()` API
- `HighlightStyle { background_color: Some(hsla(50./360., 0.9, 0.5, 0.4)) }`
- Query terms parsed and matched case-insensitively
- Overlapping matches merged before rendering

**Overflow Handling:**
- `overflow_hidden()` + `whitespace_nowrap()` on text containers
- `min_w_0()` + `flex_1()` on flex containers to enable truncation

### 10.3 Search Results View

```rust
// orion/src/views/search_results.rs

const RESULT_ITEM_HEIGHT: f32 = 100.0;

pub struct SearchResultsView {
    store: Arc<dyn MailStore>,
    index: Arc<SearchIndex>,
    query: String,
    results: Vec<SearchResult>,
    selected_index: usize,
    focus_handle: FocusHandle,
    scroll_handle: VirtualListScrollHandle,
}
```

**Implementation Notes:**
- Uses `virtual_list()` for efficient rendering of large result sets
- `focus()` method to programmatically focus the view when Enter pressed in search box
- Navigation state managed with `selected_index`
- Scroll-to-selection on keyboard navigation

**Header:** "X results for 'query'"
**List:** Virtual scrolled list of `SearchResultItem`
**Empty state:** "No results found"

---

## 11. App Integration

### 11.1 View Enum

```rust
// orion/src/app.rs

pub enum View {
    Inbox,
    Thread { html: String },
    Search { query: String },  // NEW
}
```

### 11.2 State Additions

```rust
pub struct OrionApp {
    // ... existing ...
    search_index: Option<Arc<SearchIndex>>,
    search_box: Option<Entity<SearchBox>>,
    search_results_view: Option<Entity<SearchResultsView>>,
    pending_focus_results: bool,  // Deferred focus flag
}
```

**Note on Deferred Focus:**
The `pending_focus_results` flag handles focus transfer from search box to results.
GPUI event subscriptions don't have access to `Window`, so focus changes are
deferred to the next render cycle where `Window` is available.

### 11.3 Methods

```rust
impl OrionApp {
    /// Focus search box (/ or Cmd+F)
    pub fn focus_search(&mut self, window: &mut Window, cx: &mut Context<Self>);

    /// Execute search and show results
    fn update_search(&mut self, query: String, cx: &mut Context<Self>);

    /// Clear search and return to inbox
    fn clear_search(&mut self, cx: &mut Context<Self>);
}
```

### 11.4 Layout

```
┌──────────────────────────────────────────────────────────────┐
│  Orion                              [Search mail...    /]    │
├────────────┬─────────────────────────────────────────────────┤
│            │  X results for "query"                          │
│  Sidebar   │ ────────────────────────────────────────────────│
│            │  ● Sender                                  Date │
│            │    Subject with [matches]                       │
│            │    Snippet with [matches]...                    │
│            │ ────────────────────────────────────────────────│
│            │  ● Sender                                  Date │
│            │    ...                                          │
└────────────┴─────────────────────────────────────────────────┘
```

---

## 12. Keyboard Navigation

### 12.1 Global Bindings

| Key | Action |
|-----|--------|
| `/` | Focus search box |
| `cmd-f` | Focus search box |

### 12.2 Search Box Context

| Key | Action |
|-----|--------|
| `escape` | Clear search, return to inbox |
| `enter` | Confirm search |

### 12.3 Search Results Context

| Key | Action |
|-----|--------|
| `j` / `↓` | Move selection down |
| `k` / `↑` | Move selection up |
| `enter` | Open selected thread |
| `escape` | Clear search |

---

## 13. Data Files

| File | Purpose |
|------|---------|
| `~/.config/cosmos/mail.search.idx/` | Tantivy index directory |
| `~/.config/cosmos/mail.redb` | Mail storage (existing) |

---

## 14. Files to Create

| File | Purpose |
|------|---------|
| `crates/mail/src/search/mod.rs` | Types, public API |
| `crates/mail/src/search/schema.rs` | Tantivy schema |
| `crates/mail/src/search/index.rs` | SearchIndex implementation |
| `crates/mail/src/search/query_parser.rs` | Query parsing |
| `crates/apps/orion/src/components/search_box.rs` | Search input |
| `crates/apps/orion/src/components/search_result_item.rs` | Result item |
| `crates/apps/orion/src/views/search_results.rs` | Results list |

## 15. Files to Modify

| File | Changes |
|------|---------|
| `crates/mail/Cargo.toml` | Add tantivy dependency |
| `crates/mail/src/lib.rs` | Export search module |
| `crates/mail/src/sync/inbox.rs` | Index during sync |
| `crates/apps/orion/src/app.rs` | View::Search, search state |
| `crates/apps/orion/src/components/mod.rs` | Export search components |
| `crates/apps/orion/src/views/mod.rs` | Export search_results |
| `crates/apps/orion/src/input/keymap.rs` | Search keybindings |

---

## 16. Implementation Order

1. Add Tantivy to mail crate, create schema
2. Implement SearchIndex with basic search
3. Implement query parser with operators
4. Add unit tests for query parser
5. Create SearchBox component
6. Create SearchResultItem with highlighting
7. Create SearchResultsView
8. Integrate into OrionApp (state, methods, render)
9. Add keybindings
10. Hook index into sync operations
11. Test end-to-end

---

## 17. Definition of Done (Phase 3)

Phase 3 is complete when:

- [x] Tantivy-based search index created on startup
- [x] Messages indexed during sync (initial + incremental)
- [x] Query parser handles all operators
- [x] Search box in header with `/` focus
- [x] Instant search results (150ms debounce)
- [x] Highlighted matches in results
- [x] Keyboard navigation in results (j/k, arrows)
- [x] Opening result shows thread
- [x] Escape clears search
- [x] `mail` crate remains UI-agnostic
- [x] CLAUDE.md and docs updated

---

## 18. Implementation Notes & Gotchas

### gpui-component Root Wrapper
The `gpui-component` Input component requires the window root to be wrapped in
`gpui_component::Root`. Without this, the app crashes on startup with a panic
in `root.rs`. Solution: wrap OrionApp in Root in `main.rs`.

### GPUI StyledText API
For text highlighting, use `StyledText::new(text).with_highlights(highlights)`
where highlights is `Vec<(Range<usize>, HighlightStyle)>`. Do NOT use `TextRun`
directly as it requires complex Font setup. The `with_highlights` API defers
font resolution to the render phase.

### Focus Without Window Access
GPUI event subscriptions (`cx.subscribe()`) don't have access to `Window`,
which is required for `focus_handle.focus(window)`. Solution: use a boolean
flag (`pending_focus_results`) checked in the render method where Window is
available.

### Flex Text Overflow
Using multiple flex children (divs/spans) inside a container breaks text
ellipsis/truncation. For text that needs to truncate, use a single text
element with proper styling, not multiple styled spans in a flex container.

---

## 19. Future Phases (Preview)

| Phase | Focus |
|-------|-------|
| Phase 4 | Message mutations (archive, delete, label, mark read) |
| Phase 5 | Attachments + blob storage |
| Phase 6 | Semantic search (embeddings) |
| Phase 7 | Agents (summaries, task extraction) |
| Phase 8 | Multi-account support |

---

**Document Owner**: Engineering Team
**Last Updated**: Phase 3 Planning
