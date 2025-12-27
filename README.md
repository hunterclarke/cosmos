# Cosmos

A cross-platform mail application workspace built with Rust. Cosmos provides a modern, keyboard-driven email experience with Gmail integration.

## Features

- **Full Gmail Sync** - Incremental sync with Gmail History API for fast updates
- **Multi-Account Support** - Manage multiple Gmail accounts with unified inbox view
- **Full-Text Search** - Gmail-style search with operators (`from:`, `to:`, `subject:`, `is:unread`, etc.)
- **Keyboard-First** - Vim-style navigation (`j/k`) and Gmail shortcuts (`e` archive, `s` star, etc.)
- **Cross-Platform** - GPUI app for macOS/Linux/Windows, SwiftUI app for macOS/iOS
- **Email Actions** - Archive, delete, star, mark read/unread
- **Offline-First** - SQLite storage with full local persistence

## Apps

| App | Platform | UI Framework | Status |
|-----|----------|--------------|--------|
| **Orion (GPUI)** | macOS, Linux, Windows | [GPUI](https://gpui.rs) | Active |
| **Orion (SwiftUI)** | macOS, iOS | SwiftUI + UniFFI | Active |

Both apps share the same `mail` crate for business logic, ensuring feature parity.

## Quick Start

### Prerequisites

- [Rust](https://rustup.rs/) (latest stable)
- [Xcode](https://developer.apple.com/xcode/) (for SwiftUI builds)
- Gmail OAuth credentials from [Google Cloud Console](https://console.cloud.google.com)

### Setup

```bash
# Clone the repository
git clone https://github.com/your-org/cosmos.git
cd cosmos

# Set up OAuth credentials
cp secrets/google-credentials.json.template secrets/google-credentials.json
# Edit with your OAuth client_id and client_secret from Google Cloud Console

# Configure credentials for both apps
./script/setup-credentials
```

### Running the Apps

**GPUI App (macOS/Linux/Windows):**
```bash
cargo run -p orion
# Or with release optimizations
cargo run -p orion --release
```

**SwiftUI App (macOS/iOS):**
```bash
# Build UniFFI XCFramework first
./script/build-xcframework

# Open in Xcode
./script/run-macos    # macOS
./script/run-ios      # iOS Simulator
```

## Project Structure

```
cosmos/
├── crates/
│   ├── apps/
│   │   └── orion/          # GPUI mail application
│   ├── config/             # Shared configuration utilities
│   ├── mail/               # Business logic (UniFFI-enabled)
│   │   └── src/
│   │       ├── models/     # Domain types (Thread, Message, Account)
│   │       ├── gmail/      # Gmail API client + OAuth
│   │       ├── storage/    # SQLite storage layer
│   │       ├── sync/       # Incremental sync engine
│   │       ├── query/      # Query API for UI
│   │       ├── search/     # Tantivy full-text search
│   │       ├── actions/    # Email actions (archive, star, etc.)
│   │       └── ffi/        # UniFFI bindings for Swift/Kotlin
│   └── mail-ffi/           # XCFramework generation
├── apple/
│   └── Orion/              # SwiftUI app (macOS + iOS)
├── docs/                   # Documentation
│   ├── phase_1.md          # Phase 1: Read-only inbox (complete)
│   ├── phase_2.md          # Phase 2: Full sync + persistence (complete)
│   ├── phase_3.md          # Phase 3: Full-text search (complete)
│   ├── ui.md               # UI design specification
│   ├── async.md            # Async architecture guide
│   └── workflows.md        # Development workflows
├── script/                 # Build and run scripts
└── secrets/                # OAuth credentials (gitignored)
```

## Architecture

### Separation of Concerns

```
┌─────────────────────────────────────────────────────┐
│  Orion Apps (GPUI / SwiftUI)                        │
│  - Views, components, rendering                     │
│  - User input handling                              │
│  - Zero business logic                              │
└────────────────────────┬────────────────────────────┘
                         │ depends on
                         ▼
              ┌──────────────────────┐
              │  mail crate          │
              │  (library)           │
              │                      │
              │  - Domain models     │
              │  - Gmail adapter     │
              │  - Sync engine       │
              │  - Storage (SQLite)  │
              │  - Search (Tantivy)  │
              │  - Actions           │
              └──────────────────────┘
```

**Key principles:**
- `mail` crate: Pure Rust, no UI dependencies, UniFFI-ready
- `orion` apps: UI only, delegates all logic to `mail` crate
- Sync I/O: Uses `ureq` (sync HTTP) to be executor-agnostic

### Data Storage

| File | Purpose |
|------|---------|
| `~/Library/Application Support/cosmos/mail.db` | SQLite database |
| `~/Library/Application Support/cosmos/mail.blobs/` | Message body storage |
| `~/Library/Application Support/cosmos/mail.search.idx/` | Tantivy search index |
| `~/Library/Application Support/cosmos/gmail-tokens-{email}.json` | OAuth tokens |

## Development

### Build Commands

```bash
# Build entire workspace
cargo build

# Build specific crate
cargo build -p mail

# Run tests
cargo test

# Format and lint
cargo fmt && cargo clippy
```

### SwiftUI Development

After modifying Rust code:
```bash
# Rebuild XCFramework
./script/build-xcframework

# Then rebuild in Xcode
```

### Release Builds

**GPUI App (with embedded credentials):**
```bash
./script/build-gpui-release
# Output: target/release/orion
```

**SwiftUI App:**
Build in Xcode with Release configuration (credentials embedded via xcconfig).

## Keyboard Shortcuts

### Navigation
| Key | Action |
|-----|--------|
| `j` / `↓` | Next thread |
| `k` / `↑` | Previous thread |
| `Enter` | Open thread |
| `Escape` | Go back |
| `g i` | Go to Inbox |
| `g s` | Go to Sent |

### Actions
| Key | Action |
|-----|--------|
| `e` | Archive |
| `s` | Toggle star |
| `#` | Delete |
| `Shift+I` | Mark read |
| `Shift+U` | Mark unread |
| `r` | Sync |

### Search
| Key | Action |
|-----|--------|
| `/` | Focus search |
| `Escape` | Clear search |

### Help
| Key | Action |
|-----|--------|
| `?` | Show shortcuts |

## Search Operators

| Operator | Example | Description |
|----------|---------|-------------|
| `from:` | `from:alice@example.com` | Sender filter |
| `to:` | `to:team@company.com` | Recipient filter |
| `subject:` | `subject:meeting` | Subject line filter |
| `in:` | `in:inbox`, `in:sent` | Label/folder filter |
| `is:unread` | `is:unread` | Unread messages |
| `is:starred` | `is:starred` | Starred messages |
| `before:` | `before:2024/12/01` | Date range |
| `after:` | `after:2024/01/01` | Date range |

## Roadmap

- [x] Read-only Gmail inbox (Phase 1)
- [x] Full library sync + persistence (Phase 2)
- [x] Full-text search with Tantivy (Phase 3)
- [x] Email actions (archive, delete, star, mark read)
- [x] Multi-account support
- [x] SwiftUI app for macOS/iOS
- [ ] Compose and reply to emails
- [ ] Attachments support
- [ ] OS notifications integration
- [ ] Command palette (`Cmd+K`)

## Tech Stack

- **Rust** - Core language
- **GPUI** - GPU-accelerated UI framework (desktop)
- **SwiftUI** - Native Apple UI (macOS/iOS)
- **UniFFI** - Rust-to-Swift bindings
- **SQLite** - Local storage
- **Tantivy** - Full-text search engine
- **ureq** - Sync HTTP client (executor-agnostic)

## Contributing

See [CLAUDE.md](CLAUDE.md) for detailed development guidelines and architecture documentation.
