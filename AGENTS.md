# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Cosmos is a Rust workspace containing desktop applications built with GPUI. Currently contains:
- **Orion** - A mail application with read-only Gmail integration (Phase 2: full library sync + persistence + sidebar navigation)
- **mail** - Shared mail business logic library (UniFFI-ready, platform-independent)

See `docs/phase_1.md` and `docs/phase_2.md` for implementation plans.

## Build System

This is a Cargo workspace project. The workspace root is at `/Users/hclarke/Projects/cosmos/` with member crates under `crates/`.

**Build commands:**
```bash
# Build the entire workspace
cargo build

# Build specific packages
cargo build -p orion
cargo build -p mail

# Build in release mode
cargo build --release

# Verify mail crate has no UI dependencies
cargo build -p mail
```

**Run commands:**
```bash
# Run orion application
cargo run -p orion

# Run with release optimizations
cargo run -p orion --release
```

**Test commands:**
```bash
# Run all tests in workspace
cargo test

# Run tests for specific package
cargo test -p orion
cargo test -p mail

# Run a single test
cargo test -p mail test_name

# Run integration tests only
cargo test -p mail --test integration_tests
```

**Other useful commands:**
```bash
# Check code without building
cargo check

# Format code
cargo fmt

# Run clippy linter
cargo clippy

# Clean build artifacts
cargo clean
```

## Architecture

### Workspace Structure

```
cosmos/
├── crates/
│   ├── apps/
│   │   └── orion/          # Mail app UI (GPUI-based)
│   ├── config/             # Shared configuration utilities
│   └── mail/               # Mail business logic (no UI deps)
├── docs/                   # Documentation
└── cosmos-stubs/           # Temporary stubs for cosmos-* crates
```

- Workspace uses Cargo resolver version 3

### Architectural Principles

**Separation of Concerns:**
- **mail crate**: Pure Rust business logic, zero UI dependencies
  - Must be UniFFI-ready for future mobile support
  - Side-effect free (effects through traits)
  - Deterministic and fully testable without UI
  - Contains: domain models, Gmail adapter, sync engine, storage traits, query API
- **orion crate**: UI-only code using GPUI
  - Contains zero business logic
  - Delegates all decisions to mail crate
  - Contains: views, components, rendering, user input handling

### Config Crate

The `config` crate provides shared configuration utilities for all Cosmos apps:

```rust
use config::{config_path, load_json, save_json, init};

// Bootstrap config directory on app startup
config::init()?;

// Load/save JSON config files from ~/.config/cosmos/
let settings: MySettings = config::load_json("settings.json")?;
config::save_json("settings.json", &settings)?;
```

Config directory: `~/.config/cosmos/`

### Mail Crate

The `mail` crate provides platform-independent mail functionality:

```rust
// Example usage in orion UI code
use mail::{
    sync_gmail, sync_inbox, list_threads, MailStore,
    GmailClient, GmailCredentials, RedbMailStore, SyncOptions, SyncState
};
```

**Key modules:**
- `models/` - Domain types (Thread, Message, EmailAddress, SyncState, Label)
- `gmail/` - Gmail API client, OAuth, History API, and Labels API (uses `ureq` for sync HTTP)
- `storage/` - Storage trait abstractions with InMemoryMailStore and RedbMailStore
- `sync/` - Idempotent sync engine with incremental sync support
- `query/` - Query API for UI consumption
- `search/` - Full-text search using Tantivy (Phase 3)
- `config` - Gmail credential loading

**Storage implementations:**
- `InMemoryMailStore` - For testing and development
- `RedbMailStore` - Persistent storage using redb (Phase 2)

**Sync modes (Phase 2):**
- Initial sync: Full fetch of entire mailbox (all labels, not just inbox)
- Incremental sync: Uses Gmail History API to fetch only new messages
- Automatic fallback: Falls back to initial sync if history ID expires
- Messages include label_ids for filtering by folder (Inbox, Sent, etc.)

**Search (Phase 3):**
- Uses Tantivy for full-text search (pure Rust, embedded)
- Index stored at `~/.config/cosmos/mail.search.idx/`
- Gmail-style operators: `from:`, `to:`, `subject:`, `is:unread`, `in:inbox`, `before:`, `after:`
- Messages indexed during sync

```rust
use mail::{SearchIndex, search_threads, parse_query};

// Open or create index
let index = SearchIndex::open(&index_path)?;

// Search
let results = search_threads(&index, &store, "from:alice is:unread", 50)?;
```

**Important: The mail crate is fully synchronous.** It uses `ureq` (sync HTTP) and `std::fs` (sync file I/O) to be executor-agnostic. See `docs/async.md` for details.

### Orion Application

Orion is a mail application built with GPUI (v0.2.2), a GPU-accelerated UI framework for Rust desktop applications.

**Key dependencies:**
- `gpui` (0.2.2) - Core GPUI framework
- `gpui-component` (0.5.0) - GPUI component utilities
- `mail` - Business logic for mail operations

**Application structure:**
- Entry point: `src/main.rs` - Application bootstrap
- `src/app.rs` - Root app component with sidebar navigation
- `src/views/` - GPUI view components (inbox, thread)
- `src/components/` - Reusable UI components (sidebar, thread list, message card)
- Uses GPUI's `Application` and `Window` APIs
- Implements the `Render` trait for UI components
- UI is built using a declarative builder pattern with methods like `div()`, `flex()`, `bg()`, etc.
- Sidebar shows mailbox labels (Inbox, Sent, Drafts, etc.) using gpui-component theme variables

**GPUI patterns used:**
- Component rendering via `Render` trait
- Window management with `WindowOptions`
- Element composition using method chaining (builder pattern)
- Styling with inline methods (colors via `rgb()`, sizing via `px()`, etc.)

**Using gpui-component:**

The `gpui-component` crate provides reusable UI components, theming, and utilities for GPUI applications.

**Theme System:**
```rust
use gpui_component::ActiveTheme;

// In render methods, access theme colors via cx.theme()
fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme();

    div()
        .bg(theme.background)
        .text_color(theme.foreground)
        .border_color(theme.border)
        // ...
}
```

Available theme colors include:
- `background`, `foreground` - Base colors
- `muted_foreground` - Subdued text
- `border` - Border/divider color
- `primary`, `primary_foreground`, `primary_hover`, `primary_active` - Primary accent
- `secondary`, `secondary_foreground` - Secondary background/text
- `danger`, `danger_foreground` - Error states
- `list`, `list_active`, `list_hover`, `list_active_border` - List item states

**Components:**
```rust
use gpui_component::button::{Button, ButtonVariants};

// Button with variants
Button::new("my-button")
    .label("Click me")
    .primary()  // or .ghost(), .secondary(), .danger()
    .on_click(cx.listener(|this, _event, _window, cx| { ... }))
```

**Custom Components with RenderOnce:**
```rust
use gpui::prelude::*;
use gpui::*;
use gpui_component::ActiveTheme;

#[derive(IntoElement)]  // Required for RenderOnce to work with .child()
pub struct MyComponent { ... }

impl RenderOnce for MyComponent {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        div().bg(theme.list).text_color(theme.foreground).child("...")
    }
}
```

**Dark Theme Setup:**
```rust
// In main.rs or app initialization
use gpui_component::{Theme, ThemeMode};

gpui_component::init(cx);
Theme::change(ThemeMode::Dark, None, cx);
```

**IMPORTANT: Root Wrapper Requirement:**
The `gpui-component` Input component requires the window root to be a `Root` element.
Without this, the app will crash on startup. Wrap your app component:

```rust
use gpui_component::Root;

// In window content closure
cx.new(|cx| Root::new(app_entity, window, cx))
```

**Text Highlighting with StyledText:**
For highlighting text (e.g., search matches), use GPUI's `StyledText` API:

```rust
use gpui::{StyledText, HighlightStyle, hsla};

let highlight = HighlightStyle {
    background_color: Some(hsla(50./360., 0.9, 0.5, 0.4)),
    ..Default::default()
};

let highlights = vec![(0..5, highlight)];  // Highlight chars 0-5
StyledText::new("Hello World").with_highlights(highlights)
```

Do NOT use `TextRun` directly - it requires complex Font setup.

**Custom Assets Pipeline:**

Orion extends gpui-component's bundled assets with custom Lucide icons. The asset system uses `rust-embed` to embed SVG files at compile time.

**Directory structure:**
```
crates/apps/orion/
├── assets/
│   └── icons/           # Custom Lucide SVG icons
│       ├── archive.svg
│       ├── mail-open.svg
│       └── refresh-cw.svg
└── src/
    └── assets.rs        # Asset source + custom icon types
```

**Adding a new custom icon:**

1. Download the SVG from [Lucide](https://lucide.dev/) and place it in `assets/icons/`
2. Add an icon struct in `src/assets.rs`:

```rust
// In src/assets.rs, inside the `icons` module
#[derive(Clone, Copy)]
pub struct MyIcon;

impl IconNamed for MyIcon {
    fn path(self) -> SharedString {
        "icons/my-icon.svg".into()
    }
}
```

3. Use the icon in views:

```rust
use crate::assets::icons::MyIcon;
use gpui_component::Icon;

Icon::new(MyIcon)
    .with_size(ComponentSize::Small)
    .text_color(theme.foreground)
```

**How it works:**

- `OrionAssets` implements `AssetSource` and is registered via `Application::new().with_assets(OrionAssets)`
- When loading assets, it first checks `CustomIcons` (rust-embed) for paths starting with `icons/`
- Falls back to `gpui_component_assets::Assets` for standard gpui-component icons
- Custom icon structs implement `IconNamed` trait, returning the asset path

**Available custom icons:**
- `Archive` - Box with down arrow (archive action)
- `MailOpen` - Open envelope (read/unread toggle)
- `RefreshCw` - Circular arrows (sync button)

## Cosmos Integration

The `mail` crate uses trait abstractions for storage:
- `MailStore` trait - Abstract storage interface
- `InMemoryMailStore` - In-memory stub for testing
- `RedbMailStore` - Persistent storage using redb (Phase 2)

**Phase 2**: Uses `redb` for persistent local storage at `~/.config/cosmos/mail.redb`. The `MailStore` trait allows swapping implementations.

**Future**: Real Cosmos graph storage implementations may replace redb in later phases.

## Rust Edition

The project uses Rust edition 2024 (as specified in orion's Cargo.toml).

## Development Guidelines

When working on this codebase:
- **NEVER** import GPUI or UI code in the `mail` crate
- Keep business logic in `mail`, UI code in `orion`
- Use trait abstractions for side effects (storage, network, etc.)
- Ensure `mail` crate remains testable without UI
- Follow idempotent sync patterns (operations safe to retry)
- New dependencies should be added via cargo add and should use the latest versions available.

## GPUI Async Runtime (CRITICAL)

**GPUI does NOT use Tokio.** It has its own async executor based on platform-native dispatch (GCD on macOS). Any code using `tokio::*` will panic at runtime.

**Forbidden in business logic crates:**
- `tokio` (any module)
- `reqwest` with default features (uses tokio internally via hyper)
- `async-std`

**Allowed alternatives:**
- `ureq` - Sync HTTP client
- `std::fs` - Sync file I/O
- `std::thread::sleep` - Sync delays
- `futures-timer` - If async sleep is needed

**Pattern for running blocking code from GPUI:**

```rust
// In orion UI code
let background = cx.background_executor().clone();
cx.spawn(async move |this, cx| {
    // Run blocking work on background thread pool
    let result = background.spawn(async move {
        sync_function()  // Sync call runs on background thread
    }).await;

    // Update UI on main thread
    cx.update(|cx| {
        this.update(cx, |app, cx| {
            app.data = result;
            cx.notify();
        })
    })
}).detach();
```

See `docs/async.md` for full documentation.

## Gmail Setup

To use Gmail integration:

1. **Get OAuth credentials:**
   - Go to https://console.cloud.google.com
   - Create/select a project
   - Enable the Gmail API
   - Create OAuth client ID (Desktop app type)
   - Download the JSON file

2. **Install credentials:**
   ```bash
   mkdir -p ~/.config/cosmos
   cp ~/Downloads/client_secret_*.json ~/.config/cosmos/google-credentials.json
   ```

3. **Run and authenticate:**
   - Run `cargo run -p orion`
   - Click "Sync" button
   - Follow device flow prompts in terminal
   - Token saved to `~/.config/cosmos/gmail-tokens.json`

**Data files:**
- `~/.config/cosmos/mail.redb` - Persistent mail storage (threads, messages, sync state)
- `~/.config/cosmos/mail.search.idx/` - Tantivy search index directory (Phase 3)
- `~/.config/cosmos/gmail-tokens.json` - OAuth tokens
- `~/.config/cosmos/google-credentials.json` - OAuth client credentials
