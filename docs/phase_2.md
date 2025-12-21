# Phase 2: Full Library Sync + Persistence — Implementation Plan

**Project**: COSMOS / ORION v0.2
**Status**: Implemented
**Depends On**: Phase 1 Complete

---

## Executive Summary

Implement durable, incremental, resumable Gmail sync with persistent local storage. This phase transforms Orion from a demo into a real mail system by adding crash-safe sync state, incremental updates via Gmail's History API, persistent storage across restarts, and sidebar navigation for mailbox labels.

**Phase Goal**: Durable, incremental, resumable Gmail sync with persistent local storage and label navigation
**Phase Status**: Read-only (still no mutations)

---

## 1. Goals & Non-Goals

### In Scope
- ✅ **Full library sync** (all messages, not just inbox)
- ✅ Incremental Gmail sync (no re-fetching everything)
- ✅ Persistent storage of mail data (survives restarts)
- ✅ Restart-safe sync (crash/restart resumes correctly)
- ✅ Multi-session durability
- ✅ Efficient sync for large mailboxes
- ✅ Deterministic, idempotent behavior
- ✅ SyncState persistence with history cursors
- ✅ Gmail History API integration
- ✅ **Labels API** for mailbox folder navigation
- ✅ **Sidebar navigation** using gpui-component

### Out of Scope (Future Phases)
- ❌ Send / archive / delete / label mail
- ❌ Background daemon scheduling
- ❌ Attachments (still metadata only)
- ❌ Full-text search
- ❌ Agents / LLMs
- ❌ Multi-account support

---

## 2. Architectural Constraints (Reaffirmed)

### mail Crate (Functional Core)
- Owns all mail domain logic
- Stateless except via explicit state passed in
- No direct disk I/O (delegates to storage traits)
- UniFFI-safe APIs
- Zero UI dependencies

### Persistence Layer
- Cosmos storage layer is the source of truth
- Mail writes via abstract `MailStore` trait
- Sync state is persisted, not inferred
- All operations idempotent and crash-safe

---

## 3. New Concepts Introduced in Phase 2

### 3.1 Sync State

We now persist sync cursors so the mailbox can be incrementally updated. The `SyncState` tracks the Gmail `historyId` which allows us to fetch only changes since the last sync.

### 3.2 Durable Identity

Gmail message IDs and thread IDs are now treated as stable external IDs. All writes are keyed by these external IDs to ensure idempotent upserts.

### 3.3 Snapshots vs Deltas
- **Initial sync** = Snapshot (full fetch of mailbox messages)
- **Subsequent syncs** = Deltas (only fetch changes via History API)

### 3.4 Labels (Folders)

Messages now track their Gmail label IDs, enabling navigation between Inbox, Sent, Drafts, Trash, etc.

---

## 4. Data Model (Additions)

### 4.1 Sync State Schema

```rust
// mail/src/models/sync_state.rs

/// Tracks sync progress for a Gmail account
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SyncState {
    /// Gmail user or account identifier
    pub account_id: String,
    /// Gmail historyId for incremental sync
    pub history_id: String,
    /// When we last successfully synced
    pub last_sync_at: DateTime<Utc>,
    /// Schema version for migrations
    pub sync_version: u32,
}
```

### 4.2 Label Model

```rust
// mail/src/models/label.rs

/// Unique identifier for a label
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LabelId(pub String);

impl LabelId {
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

/// A mail label (folder)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Label {
    pub id: LabelId,
    pub name: String,
    pub is_system: bool,
    pub message_count: u32,
    pub unread_count: u32,
}
```

### 4.3 Message Model (Extended)

```rust
// mail/src/models/message.rs

pub struct Message {
    // ... existing fields ...

    /// Gmail label IDs (e.g., "INBOX", "SENT", "UNREAD")
    pub label_ids: Vec<String>,
}
```

---

## 5. Gmail Sync Strategy (Full Library)

### 5.1 Initial Sync (Cold Start)

**Trigger**: No existing `SyncState` for account

**Steps**:
1. Fetch messages via `users.messages.list`
   - **No query filter** (syncs all messages, not just inbox)
   - Limit: configurable (default 500 per page)
2. For each message:
   - Fetch full message via `users.messages.get`
   - Extract label_ids from response
   - Normalize → Orion models
   - Upsert Thread
   - Upsert Message
3. Record:
   - `history_id` from Gmail response
   - `last_sync_at = now`
4. Persist `SyncState`

### 5.2 Incremental Sync (Warm Start)

**Trigger**: Existing `SyncState` with valid `history_id`

**API Used**: `users.history.list`

**Steps**:
1. Call History API with stored `history_id`
   - **No labelId filter** (receives changes for all labels)
2. For each history record:
   - If `messagesAdded`:
     - Fetch message via `users.messages.get`
     - Normalize → Orion models
     - Upsert Thread + Message
   - Ignore `messagesDeleted` (Phase 2 is read-only)
3. Update:
   - `history_id` from response
   - `last_sync_at = now`
4. Persist `SyncState`

### 5.3 History ID Expired Fallback

If Gmail returns `404 historyId` (too old):
1. Log warning
2. Clear existing mail data (mail namespace only)
3. Delete existing `SyncState`
4. Re-run initial sync from scratch

---

## 6. Gmail API Extensions

### 6.1 Labels API

```rust
// mail/src/gmail/client.rs

impl GmailClient {
    /// List all labels (folders) in the user's mailbox
    pub fn list_labels(&self) -> Result<ListLabelsResponse>;
}

// mail/src/gmail/api.rs

/// Response from Gmail Labels API
#[derive(Debug, Deserialize)]
pub struct ListLabelsResponse {
    pub labels: Option<Vec<GmailLabel>>,
}

/// A Gmail label (folder)
#[derive(Debug, Clone, Deserialize)]
pub struct GmailLabel {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub label_type: Option<String>,
    pub messages_total: Option<u32>,
    pub messages_unread: Option<u32>,
}
```

### 6.2 Gmail API Endpoints

| Endpoint | Use Case |
|----------|----------|
| `GET /gmail/v1/users/me/messages` | Initial sync (all messages) |
| `GET /gmail/v1/users/me/messages/{id}` | Fetch full message |
| `GET /gmail/v1/users/me/history` | Incremental sync (all changes) |
| `GET /gmail/v1/users/me/labels` | List mailbox labels |

**Messages API**: No `q=in:inbox` filter - syncs full library.

**History API Query Parameters**:
- `startHistoryId`: Required, from previous sync
- `historyTypes`: `messageAdded` (we ignore deletes in Phase 2)
- **No `labelId` filter** - receives all changes

---

## 7. UI: Sidebar Navigation

### 7.1 Sidebar Component

Using gpui-component theme variables for consistent styling:

```rust
// orion/src/components/sidebar.rs

#[derive(IntoElement)]
pub struct SidebarItem {
    label: Label,
    is_selected: bool,
}

impl RenderOnce for SidebarItem {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();

        // Use gpui-component theme colors
        let bg_color = if self.is_selected {
            theme.list_active
        } else {
            theme.transparent
        };

        let text_color = if self.is_selected {
            theme.foreground
        } else {
            theme.muted_foreground
        };

        div()
            .px_3()
            .py_2()
            .rounded_md()
            .bg(bg_color)
            .border_l_2()
            .border_color(if self.is_selected {
                theme.list_active_border
            } else {
                theme.transparent
            })
            .cursor_pointer()
            .hover(|style| style.bg(theme.list_hover))
            .child(/* ... */)
    }
}
```

### 7.2 Theme Variables Used

| Variable | Usage |
|----------|-------|
| `theme.secondary` | Sidebar background |
| `theme.border` | Sidebar right border |
| `theme.list_active` | Selected item background |
| `theme.list_active_border` | Selected item left accent |
| `theme.list_hover` | Hover state |
| `theme.foreground` | Selected text |
| `theme.muted_foreground` | Unselected text |
| `theme.primary` | Unread count badge background |
| `theme.primary_foreground` | Badge text |

### 7.3 App Layout

```
┌──────────────────────────────────────────────────┐
│  Header (Orion | Sync Status | Sync Button)      │
├────────────┬─────────────────────────────────────┤
│            │                                     │
│  Sidebar   │       Main Content                  │
│  220px     │       (Inbox/Thread View)           │
│            │                                     │
│  - Inbox   │                                     │
│  - Starred │                                     │
│  - Sent    │                                     │
│  - Drafts  │                                     │
│  - All Mail│                                     │
│  - Spam    │                                     │
│  - Trash   │                                     │
│            │                                     │
│  Labels    │                                     │
│  - Custom1 │                                     │
│  - Custom2 │                                     │
│            │                                     │
└────────────┴─────────────────────────────────────┘
```

---

## 8. Storage Implementation

### 8.1 redb Backend

```rust
// mail/src/storage/persistent.rs

pub struct RedbMailStore {
    db: Arc<Database>,
}

// Tables
const THREADS: TableDefinition<&str, &[u8]> = TableDefinition::new("threads");
const MESSAGES: TableDefinition<&str, &[u8]> = TableDefinition::new("messages");
const SYNC_STATE: TableDefinition<&str, &[u8]> = TableDefinition::new("sync_state");
const THREAD_MESSAGES: TableDefinition<&str, &[u8]> = TableDefinition::new("thread_messages");
```

### 8.2 Data Files

All stored in `~/.config/cosmos/`:
- `mail.redb` - Persistent mail storage
- `gmail-tokens.json` - OAuth tokens
- `google-credentials.json` - OAuth client credentials

---

## 9. Recent Improvements

### 9.1 VirtualList for Thread Rendering

The thread list now uses `gpui-component`'s `v_virtual_list` for efficient rendering of large thread lists. Only visible items are rendered, significantly improving performance.

### 9.2 Storage-Layer Label Filtering

Label filtering is now performed at the storage layer via `list_threads_by_label()` rather than in-memory filtering. This is more efficient for large mailboxes.

### 9.3 Full Email Pagination

Initial sync now fetches **all** messages in the user's mailbox using pagination, not just the first 500. The `list_messages_all()` method handles automatic pagination through all pages.

### 9.4 Optimistic UI Updates

During sync, the UI is refreshed every 500ms to show new messages as they are stored, providing a more responsive experience rather than waiting for the full sync to complete.

---

## 10. Definition of Done (Phase 2)

Phase 2 is complete when:

- [x] **Full library sync** (not just inbox)
- [x] **Full email pagination** (fetches all messages, not just first page)
- [x] Messages include label_ids
- [x] Inbox persists across app restarts
- [x] Sync is incremental and fast (uses history API)
- [x] No duplicate messages
- [x] Crash-safe (can recover from interrupted sync)
- [x] History ID fallback works (full resync on expiration)
- [x] Sidebar navigation shows mailbox labels
- [x] **Label filtering at storage layer** (efficient querying)
- [x] **VirtualList for thread rendering** (efficient UI)
- [x] **Optimistic UI updates** (live refresh during sync)
- [x] Uses gpui-component theme variables
- [x] `mail` crate remains UI-agnostic
- [x] CLAUDE.md and docs updated

---

## 11. Future Phases (Preview)

| Phase | Focus |
|-------|-------|
| Phase 3 | Label filtering, read/unread state, message mutations |
| Phase 4 | Attachments + blob storage |
| Phase 5 | Full-text search (text + semantic) |
| Phase 6 | Agents (summaries, task extraction) |
| Phase 7 | Multi-account support |

---

**Document Owner**: Engineering Team
**Last Updated**: Phase 2 Implementation Complete
