# Phase 1: Read-Only Gmail Inbox — Implementation Plan

**Project**: COSMOS / ORION v0.1
**Status**: Planning
**Last Updated**: 2025-12-20

---

## Executive Summary

Implement a read-only Gmail inbox viewer in Orion with a clean separation between functional core logic (`mail` crate) and UI (within `orion` app). This phase establishes the foundational architecture for all future mail features.

**Timeline**: 3-4 weeks (parallelizable)
**Team Size**: 2-4 engineers

---

## 1. Goals & Non-Goals

### In Scope
- ✅ OAuth authentication with Gmail
- ✅ Fetch messages from Gmail inbox (latest N messages)
- ✅ Normalize Gmail API responses to Orion data model
- ✅ Store messages in local persistence layer
- ✅ Display inbox list view (threads)
- ✅ Display thread detail view (messages)
- ✅ Manual refresh/sync
- ✅ Read-only operations only

### Out of Scope (Future Phases)
- ❌ Sending, archiving, deleting mail
- ❌ Background sync daemon
- ❌ Multiple account support
- ❌ Labels/folders beyond inbox
- ❌ Attachment download/preview
- ❌ Full-text search
- ❌ Agent/LLM features
- ❌ Workstreams
- ❌ Real-time push updates

---

## 2. Architecture Overview

```
┌─────────────────────────────────────────────────────┐
│  orion (binary crate)                               │
│                                                     │
│  - main.rs: Application entry point                 │
│  - UI modules: views, components (GPUI)             │
│  - Calls into mail crate for all business logic     │
└──────────────────────┬──────────────────────────────┘
                       │ depends on
                       ▼
            ┌──────────────────────┐
            │   mail               │
            │   (library crate)    │
            │                      │
            │  - Domain models     │
            │  - Gmail adapter     │
            │  - Sync engine       │
            │  - Storage interface │
            │  - Query API         │
            └──────────┬───────────┘
                       │ depends on
            ┌──────────▼───────────┐
            │  cosmos-* crates     │
            │  (stubbed for now)   │
            │                      │
            │  - cosmos-storage    │
            │  - cosmos-graph      │
            │  - cosmos-query      │
            └──────────────────────┘
```

### Key Architectural Constraints

1. **mail crate** must be:
   - Pure Rust (no platform-specific code)
   - Side-effect free (effects through traits)
   - Deterministic & testable
   - UniFFI-ready (for future mobile support)
   - Zero UI dependencies (no GPUI imports)

2. **orion UI code** must:
   - Contain zero business logic
   - Only handle rendering & user input
   - Delegate all decisions to mail crate

---

## 3. Workspace Structure (After Phase 1)

```
cosmos/
├── Cargo.toml                    # Workspace root
├── phase_1.md                    # This file
├── crates/
│   ├── apps/
│   │   └── orion/                # Main binary with UI
│   │       ├── Cargo.toml
│   │       └── src/
│   │           ├── main.rs       # App entry point
│   │           ├── app.rs        # Root app component
│   │           ├── views/        # GPUI views
│   │           │   ├── mod.rs
│   │           │   ├── inbox.rs  # Inbox list view
│   │           │   └── thread.rs # Thread detail view
│   │           └── components/   # Reusable UI components
│   │               ├── mod.rs
│   │               ├── message_card.rs
│   │               └── thread_list_item.rs
│   └── mail/                     # NEW: Business logic
│       ├── Cargo.toml
│       ├── src/
│       │   ├── lib.rs
│       │   ├── models/           # Domain models
│       │   │   ├── mod.rs
│       │   │   ├── thread.rs
│       │   │   └── message.rs
│       │   ├── gmail/            # Gmail integration
│       │   │   ├── mod.rs
│       │   │   ├── client.rs
│       │   │   ├── auth.rs
│       │   │   └── normalize.rs
│       │   ├── storage/          # Storage traits
│       │   │   ├── mod.rs
│       │   │   └── traits.rs
│       │   ├── sync/             # Sync engine
│       │   │   ├── mod.rs
│       │   │   └── inbox.rs
│       │   └── query/            # Query API
│       │       ├── mod.rs
│       │       └── threads.rs
│       └── tests/
│           └── integration_tests.rs
└── cosmos-stubs/                 # NEW: Temporary stubs
    ├── cosmos-storage/
    ├── cosmos-graph/
    └── cosmos-query/
```

---

## 4. Data Model

### 4.1 Core Domain Types (mail/src/models/)

```rust
// models/thread.rs
pub struct Thread {
    pub id: ThreadId,              // Gmail thread ID
    pub subject: String,
    pub snippet: String,           // Preview text
    pub last_message_at: DateTime<Utc>,
    pub message_count: usize,
}

pub struct ThreadId(pub String);

// models/message.rs
pub struct Message {
    pub id: MessageId,             // Gmail message ID
    pub thread_id: ThreadId,
    pub from: EmailAddress,
    pub to: Vec<EmailAddress>,
    pub cc: Vec<EmailAddress>,
    pub subject: String,
    pub body_preview: String,      // Plain text only (v0)
    pub received_at: DateTime<Utc>,
    pub internal_date: i64,        // Gmail's internal timestamp
}

pub struct MessageId(pub String);

pub struct EmailAddress {
    pub name: Option<String>,      // Display name
    pub email: String,             // email@domain.com
}
```

### 4.2 Storage Schema

```
Nodes:
  mail::Thread
    - id: string (gmail thread id)
    - subject: string
    - snippet: string
    - last_message_at: timestamp
    - message_count: int

  mail::Message
    - id: string (gmail message id)
    - from_name: string?
    - from_email: string
    - subject: string
    - body_preview: string
    - received_at: timestamp
    - internal_date: i64

Edges:
  mail::Thread -[contains]-> mail::Message
  mail::Message -[from]-> identity::Person (future)
  mail::Message -[to]-> identity::Person (future)
```

---

## 5. Work Packages (Parallelizable)

### Package A: Project Structure & Stubs
**Owner**: Engineer A
**Duration**: 2-3 days
**Depends on**: Nothing

**Tasks**:
1. Create `mail` crate under `crates/mail/`
   - Basic Cargo.toml with dependencies (no UI deps!)
   - Module structure (models, gmail, storage, sync, query)
   - Integration test scaffolding
2. Add UI modules to `orion` crate
   - Update orion's Cargo.toml to depend on `mail`
   - Create module structure (app, views, components)
   - Keep main.rs minimal (just bootstrap)
3. Create `cosmos-stubs` workspace member
   - Stub `cosmos-storage` with in-memory HashMap
   - Stub `cosmos-graph` with basic edge storage
   - Stub `cosmos-query` with simple Vec filtering
4. Update workspace Cargo.toml with new members
5. Ensure `cargo build --workspace` succeeds

**Definition of Done**:
- [ ] `mail` crate compiles (library crate)
- [ ] `orion` crate compiles with UI modules (binary crate)
- [ ] `cargo test --workspace` passes (even if no tests)
- [ ] Basic module structure in place
- [ ] `mail` has zero GPUI dependencies

---

### Package B: Gmail API Client
**Owner**: Engineer B
**Duration**: 5-7 days
**Depends on**: Package A (crate structure)

**Tasks**:
1. Set up OAuth2 flow (use `oauth2` crate)
   - Implement device flow or localhost redirect
   - Token storage (encrypted file for v0)
   - Token refresh logic
2. Implement Gmail API client (`gmail/client.rs`)
   - `list_messages(max_results: usize) -> Vec<MessageId>`
   - `get_message(id: MessageId) -> GmailMessage`
   - Use `reqwest` for HTTP
   - Add exponential backoff retry logic
3. Parse Gmail API responses (`gmail/normalize.rs`)
   - Extract headers (From, To, Cc, Subject, Date)
   - Extract snippet
   - Extract plain text body (prefer text/plain)
   - Handle base64 decoding
4. Unit tests with fixture data
   - Mock Gmail responses (use `serde_json` fixtures)
   - Test normalization edge cases (missing headers, etc.)

**Definition of Done**:
- [ ] Can authenticate with Gmail (manual OAuth flow)
- [ ] Can fetch 100 messages from inbox
- [ ] Can parse all required fields
- [ ] 90%+ test coverage on normalization code
- [ ] Error handling for API failures

**Key Dependencies**:
```toml
[dependencies]
reqwest = { version = "0.12", features = ["json"] }
oauth2 = "4.4"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
chrono = "0.4"
base64 = "0.22"
tokio = { version = "1", features = ["full"] }
```

---

### Package C: Sync Engine & Storage
**Owner**: Engineer C
**Duration**: 5-7 days
**Depends on**: Package A, partial Package B (gmail client API)

**Tasks**:
1. Define storage traits (`storage/traits.rs`)
   ```rust
   pub trait MailStore {
       fn upsert_thread(&self, thread: Thread) -> Result<()>;
       fn upsert_message(&self, message: Message) -> Result<()>;
       fn link_message_to_thread(&self, msg_id: MessageId, thread_id: ThreadId) -> Result<()>;
       fn get_thread(&self, id: ThreadId) -> Result<Option<Thread>>;
       fn get_message(&self, id: MessageId) -> Result<Option<Message>>;
       fn list_threads(&self, limit: usize) -> Result<Vec<Thread>>;
   }
   ```
2. Implement in-memory store for testing
3. Implement sync algorithm (`sync/inbox.rs`)
   ```rust
   pub async fn sync_inbox(
       gmail: &GmailClient,
       store: &dyn MailStore,
       max_messages: usize,
   ) -> Result<SyncStats> {
       // 1. Fetch message IDs from Gmail
       // 2. Filter out already-synced messages
       // 3. Fetch full message details
       // 4. Normalize to Orion models
       // 5. Group by thread
       // 6. Upsert threads + messages atomically
       // 7. Return stats
   }
   ```
4. Implement thread grouping logic
   - Group messages by thread_id
   - Compute thread subject, snippet, last_message_at
5. Integration tests
   - Test idempotent sync (run twice, same result)
   - Test incremental sync (new messages only)
   - Test thread reconstruction

**Definition of Done**:
- [ ] Storage traits defined and documented
- [ ] In-memory implementation works
- [ ] Sync algorithm is idempotent (critical!)
- [ ] Handles duplicate messages gracefully
- [ ] Integration tests pass with mock Gmail data
- [ ] No data loss scenarios

---

### Package D: Query API
**Owner**: Engineer D (or piggyback on C)
**Duration**: 2-3 days
**Depends on**: Package C (storage traits)

**Tasks**:
1. Implement query functions (`query/threads.rs`)
   ```rust
   pub fn list_threads(
       store: &dyn MailStore,
       limit: usize,
       offset: usize,
   ) -> Result<Vec<ThreadSummary>>;

   pub fn get_thread_detail(
       store: &dyn MailStore,
       thread_id: ThreadId,
   ) -> Result<ThreadDetail>;

   pub struct ThreadSummary {
       pub id: ThreadId,
       pub subject: String,
       pub snippet: String,
       pub last_message_at: DateTime<Utc>,
       pub message_count: usize,
   }

   pub struct ThreadDetail {
       pub thread: Thread,
       pub messages: Vec<Message>,
   }
   ```
2. Add pagination support (offset/limit)
3. Add sorting (by last_message_at desc)
4. Unit tests for query logic

**Definition of Done**:
- [ ] Query API compiles and is well-documented
- [ ] Returns expected results from in-memory store
- [ ] Pagination works correctly
- [ ] Unit tests cover edge cases (empty results, etc.)

---

### Package E: UI Implementation
**Owner**: Engineer E
**Duration**: 5-7 days
**Depends on**: Package A, Package D (query API)

**Tasks**:
1. Implement root app component (`orion/src/app.rs`)
   - Initialize mail crate
   - Handle app state (current view, selected thread)
   - Route between inbox/thread views
2. Implement inbox view (`orion/src/views/inbox.rs`)
   - Display list of threads (subject, snippet, date)
   - Handle thread selection
   - Show loading state during sync
   - Show error states
3. Implement thread view (`orion/src/views/thread.rs`)
   - Display messages in thread (chronological order)
   - Show From, To, Date, Body preview
   - Back button to inbox
4. Create reusable components (`orion/src/components/`)
   - `ThreadListItem` (one row in inbox)
   - `MessageCard` (one message in thread view)
   - `RefreshButton` (trigger sync)
5. Wire up `mail` crate calls
   - Call `sync_inbox()` on refresh
   - Call `list_threads()` for inbox
   - Call `get_thread_detail()` for thread view
6. Handle async operations
   - Use GPUI's async support
   - Show spinners during network calls
   - Display errors to user

**Definition of Done**:
- [ ] Inbox view renders threads
- [ ] Thread view renders messages
- [ ] Refresh button triggers sync
- [ ] Loading states are clear
- [ ] Error handling is user-friendly
- [ ] No crashes or panics
- [ ] Basic styling (readable, not beautiful)
- [ ] UI code properly imports from `mail` crate

**Key GPUI Patterns**:
```rust
use gpui::*;
use gpui_component::prelude::*;
use mail::{sync_inbox, list_threads};

impl Render for InboxView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .child(self.render_header(cx))
            .child(self.render_thread_list(cx))
    }
}
```

---

### Package F: Integration & Polish
**Owner**: Lead Engineer
**Duration**: 3-5 days
**Depends on**: All packages (A-E)

**Tasks**:
1. End-to-end integration
   - Wire orion binary to use UI modules + mail crate
   - Test full flow: auth → sync → display
2. Error handling audit
   - Network failures
   - Auth errors
   - Malformed Gmail responses
   - Storage errors
3. Performance testing
   - 100 messages
   - 1,000 messages
   - 10,000 messages (should be snappy)
4. Create README.md
   - Setup instructions
   - OAuth configuration steps
   - How to run the app
5. Write basic user documentation
6. Code review & cleanup
   - Remove dead code
   - Add missing documentation
   - Fix Clippy warnings

**Definition of Done**:
- [ ] App runs end-to-end without crashes
- [ ] OAuth flow works for new users
- [ ] Sync completes successfully
- [ ] UI is responsive and usable
- [ ] README is clear and complete
- [ ] All tests pass (`cargo test --workspace`)
- [ ] Clippy has zero warnings (`cargo clippy`)
- [ ] Code is formatted (`cargo fmt`)

---

## 6. Testing Strategy

### 6.1 Unit Tests
**Location**: Within each crate
**Coverage Target**: 80%+

- **mail/gmail**: Mock API responses, test normalization
- **mail/sync**: Test idempotency, thread grouping
- **mail/query**: Test pagination, sorting
- **mail/storage**: Test in-memory implementation

### 6.2 Integration Tests
**Location**: `mail/tests/`

1. **Full sync test**:
   - Mock Gmail API with 50 messages
   - Run sync
   - Verify all messages stored
   - Run sync again (idempotency)
   - Verify no duplicates

2. **Thread reconstruction test**:
   - Mock messages with same thread_id
   - Verify thread properties computed correctly

### 6.3 Manual Testing
**Location**: Real Gmail account (test account)

- [ ] Auth flow (first time)
- [ ] Auth flow (with cached tokens)
- [ ] Sync 100+ messages
- [ ] Navigate inbox
- [ ] Open thread
- [ ] Refresh inbox (new messages appear)
- [ ] Handle network loss gracefully
- [ ] Handle invalid tokens (re-auth prompt)

---

## 7. Risk Mitigation

### Risk 1: Gmail API Rate Limits
**Likelihood**: High
**Impact**: Medium
**Mitigation**:
- Implement exponential backoff
- Add jitter to retries
- Cache tokens properly
- Use batch APIs where possible (future)

### Risk 2: Malformed Gmail Data
**Likelihood**: Medium
**Impact**: Medium
**Mitigation**:
- Defensive parsing (assume headers missing)
- Extensive unit tests with real-world fixtures
- Log parse errors (don't crash)

### Risk 3: Storage Performance
**Likelihood**: Low (v0 is small scale)
**Impact**: Low
**Mitigation**:
- Use in-memory storage for v0
- Profile before optimizing
- Design storage traits to allow swapping implementations

### Risk 4: OAuth Flow Complexity
**Likelihood**: Medium
**Impact**: High (can't use app without auth)
**Mitigation**:
- Use well-tested `oauth2` crate
- Start with device flow (simpler than web redirect)
- Document setup steps clearly
- Consider stubbing auth for demo purposes

---

## 8. Dependencies & External APIs

### Rust Crates
```toml
# mail crate (business logic)
[dependencies]
reqwest = { version = "0.12", features = ["json"] }
oauth2 = "4.4"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
chrono = { version = "0.4", features = ["serde"] }
base64 = "0.22"
tokio = { version = "1", features = ["full"] }
anyhow = "1.0"
thiserror = "1.0"

# orion crate (binary with UI)
[dependencies]
gpui = "0.2.2"
gpui-component = "0.5.0"
mail = { path = "../../mail" }
```

### Gmail API Endpoints (v1)
- **Auth**: `https://oauth2.googleapis.com/token`
- **List messages**: `GET https://gmail.googleapis.com/gmail/v1/users/me/messages?maxResults=100`
- **Get message**: `GET https://gmail.googleapis.com/gmail/v1/users/me/messages/{id}?format=full`

**Scopes Required**:
- `https://www.googleapis.com/auth/gmail.readonly`

---

## 9. Success Metrics

### Functional Requirements
- [ ] User can authenticate with Gmail
- [ ] App displays at least 100 messages from inbox
- [ ] Threads group correctly (all messages with same thread_id)
- [ ] Thread view shows messages in chronological order
- [ ] Refresh fetches new messages
- [ ] No crashes during normal operation

### Non-Functional Requirements
- [ ] Sync 100 messages in < 10 seconds (network dependent)
- [ ] UI is responsive (no blocking on main thread)
- [ ] Memory usage reasonable (< 100MB for 1000 messages)
- [ ] `mail` crate is fully testable without UI
- [ ] Code coverage > 80%

### Quality Gates
- [ ] All tests pass
- [ ] Zero Clippy warnings
- [ ] Code formatted with `cargo fmt`
- [ ] README complete with setup instructions
- [ ] No TODO comments in production code

---

## 10. Future Considerations (Post-Phase 1)

These are explicitly OUT OF SCOPE but should inform architectural decisions:

1. **Incremental Sync**
   - Store sync cursor (history ID)
   - Only fetch new messages since last sync
   - *Architectural prep*: Design sync API to accept optional cursor

2. **Multiple Accounts**
   - Support >1 Gmail account
   - *Architectural prep*: Store account ID in all models

3. **Labels & Folders**
   - Beyond just inbox
   - *Architectural prep*: Add labels field to Message model

4. **Read/Unread State**
   - Track which messages user has read
   - *Architectural prep*: Add metadata table/edges

5. **Attachments**
   - Download and display attachments
   - *Architectural prep*: Add attachments list to Message model

6. **UniFFI Bindings**
   - Expose `mail` crate to Swift/Kotlin for mobile
   - *Architectural prep*: Keep `mail` crate pure Rust, no platform deps

7. **Background Sync**
   - Daemon process for continuous sync
   - *Architectural prep*: Make sync engine stateless

---

## 11. Open Questions

1. **OAuth Flow**: Device flow vs localhost redirect?
   - **Recommendation**: Start with device flow (simpler, no web server)

2. **Token Storage**: Where to store access/refresh tokens?
   - **Recommendation**: Encrypted JSON file in `~/.orion/tokens.json` (use `keyring` crate later)

3. **Error Handling**: Toast notifications vs status bar?
   - **Recommendation**: Status bar for persistent errors, logs for transient ones

4. **Testing Account**: Should we create a shared test Gmail account?
   - **Recommendation**: Yes, pre-populate with varied test data

5. **Cosmos Integration**: Stub or implement real cosmos-storage?
   - **Recommendation**: Stub for Phase 1 (in-memory HashMap), integrate in Phase 2

---

## 12. Definition of Done (Phase 1)

Phase 1 is complete when:

- [ ] All work packages (A-F) are done
- [ ] User can authenticate with Gmail via OAuth
- [ ] User can view their inbox (list of threads)
- [ ] User can click a thread to view messages
- [ ] User can refresh to fetch new messages
- [ ] Sync is idempotent (can run multiple times safely)
- [ ] No crashes or panics during normal operation
- [ ] `mail` crate has 80%+ test coverage
- [ ] All integration tests pass
- [ ] Manual testing checklist complete
- [ ] README documents setup and usage
- [ ] Code passes `cargo clippy` and `cargo fmt`
- [ ] Demo video recorded (optional but recommended)

---

## 13. Next Steps After Phase 1

Once Phase 1 is complete and validated:

1. **Immediate Next Phase Options**:
   - Phase 2A: Incremental sync (use Gmail history API)
   - Phase 2B: Read/unread state + labels
   - Phase 2C: Search functionality
   - Phase 2D: Replace cosmos stubs with real graph storage

2. **Technical Debt to Address**:
   - Token storage security (use OS keychain)
   - Error handling polish
   - UI styling improvements
   - Performance profiling & optimization

3. **Documentation**:
   - Architecture decision records (ADRs)
   - API documentation (rustdoc)
   - User guide
   - Contributor guide

---

## Appendix A: Example Code Skeletons

### mail/src/lib.rs
```rust
pub mod models;
pub mod gmail;
pub mod storage;
pub mod sync;
pub mod query;

pub use models::{Thread, ThreadId, Message, MessageId, EmailAddress};
pub use gmail::{GmailClient, GmailAuth};
pub use storage::MailStore;
pub use sync::sync_inbox;
pub use query::{list_threads, get_thread_detail};
```

### orion/src/app.rs
```rust
use gpui::*;
use mail::{sync_inbox, list_threads, MailStore, GmailClient, ThreadId};
use std::sync::Arc;

pub struct OrionApp {
    current_view: View,
    mail_store: Arc<dyn MailStore>,
    gmail_client: Arc<GmailClient>,
}

enum View {
    Inbox,
    Thread(ThreadId),
}

impl Render for OrionApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        match self.current_view {
            View::Inbox => self.render_inbox(cx),
            View::Thread(ref id) => self.render_thread(id, cx),
        }
    }
}
```

---

## Appendix B: Gmail API Response Example

```json
{
  "id": "18d1e2f3a4b5c6d7",
  "threadId": "18d1e2f3a4b5c6d7",
  "labelIds": ["INBOX", "UNREAD"],
  "snippet": "Hey, can we sync up tomorrow?",
  "internalDate": "1640000000000",
  "payload": {
    "headers": [
      {"name": "From", "value": "Alice <alice@example.com>"},
      {"name": "To", "value": "bob@example.com"},
      {"name": "Subject", "value": "Quick sync"},
      {"name": "Date", "value": "Mon, 20 Dec 2021 10:00:00 -0800"}
    ],
    "body": {
      "size": 1234,
      "data": "SGV5LCBjYW4gd2Ugc3luYyB1cCB0b21vcnJvdz8="
    }
  }
}
```

---

## Appendix C: Useful Commands

```bash
# Build everything
cargo build --workspace

# Run tests
cargo test --workspace

# Run orion
cargo run -p orion

# Check without building
cargo check --workspace

# Lint
cargo clippy --workspace -- -D warnings

# Format
cargo fmt --all

# Generate docs
cargo doc --workspace --open

# Clean build artifacts
cargo clean
```

---

**Document Owner**: Engineering Team
**Approval Required**: Tech Lead, Product
**Estimated Completion**: Q1 2025
