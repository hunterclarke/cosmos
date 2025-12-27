# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Cosmos is a Rust workspace containing desktop applications built with GPUI. Currently contains:
- **Orion (GPUI)** - A mail application with read-only Gmail integration (cross-platform: macOS, Linux, Windows; Phase 2: full library sync + persistence + sidebar navigation)
- **Orion (SwiftUI)** - Universal SwiftUI mail app for macOS and iOS using UniFFI bindings (`apple/Orion/`)
- **mail** - Shared mail business logic library (UniFFI-enabled, platform-independent)
- **mail-ffi** - Thin UniFFI crate for generating XCFramework bindings

See `docs/phase_1.md`, `docs/phase_2.md`, and `docs/ui.md` for implementation plans.
See `docs/workflows.md` for development and release workflows.

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

**SwiftUI/UniFFI commands:**
```bash
# Install required cross-compilation targets (one-time setup)
rustup target add x86_64-apple-darwin aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios

# Build XCFramework for all Apple platforms
./script/build-xcframework

# Build mail-ffi crate only
cargo build -p mail-ffi

# Generate Swift bindings manually
cargo run -p mail-ffi --features bindgen -- generate \
    --library target/debug/libmail_ffi.dylib \
    --language swift \
    --out-dir generated
```

**Run scripts:**
```bash
# Run GPUI Orion app (cross-platform, via cargo)
./script/run-gpui
./script/run-gpui --release

# Run SwiftUI Orion app (opens Xcode)
./script/run-macos   # macOS (opens Xcode project)
./script/run-ios     # iOS Simulator (opens Xcode project)
```

## Architecture

### Workspace Structure

```
cosmos/
├── apple/                  # SwiftUI apps
│   └── Orion/              # Universal SwiftUI mail app (macOS + iOS)
│       ├── Orion.xcodeproj/    # Xcode project
│       ├── Package.swift       # Swift Package definition (for SPM builds)
│       ├── MailFFI.xcframework # Generated UniFFI bindings
│       ├── Sources/            # Swift source files
│       │   └── Generated/      # Generated Swift bindings (mail.swift)
│       └── Resources/          # Info.plist, entitlements
├── crates/
│   ├── apps/
│   │   └── orion/          # Mail app UI (GPUI-based, cross-platform)
│   ├── config/             # Shared configuration utilities
│   ├── mail/               # Mail business logic (UniFFI-enabled)
│   │   └── src/ffi/        # UniFFI facade module
│   └── mail-ffi/           # Thin crate for XCFramework generation
├── docs/                   # Documentation
├── script/                 # Build scripts
│   └── build-xcframework   # XCFramework builder for Apple platforms
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

// Load/save JSON config files from the Cosmos config directory
let settings: MySettings = config::load_json("settings.json")?;
config::save_json("settings.json", &settings)?;
```

Config directory (platform-specific via `dirs::config_dir()`):
- macOS: `~/Library/Application Support/cosmos/`
- Linux: `~/.config/cosmos/`

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
- `models/` - Domain types (Thread, Message, Account, EmailAddress, SyncState, Label)
- `gmail/` - Gmail API client, OAuth, History API, and Labels API (uses `ureq` for sync HTTP)
- `storage/` - Storage trait abstractions with InMemoryMailStore and SqliteMailStore
- `sync/` - Idempotent sync engine with incremental sync support
- `query/` - Query API for UI consumption
- `search/` - Full-text search using Tantivy (Phase 3)
- `actions/` - Email action handlers (archive, star, read, trash)
- `config` - Gmail credential loading

**Storage implementations:**
- `InMemoryMailStore` - For testing and development
- `SqliteMailStore` - Persistent storage using SQLite with zstd compression

**Sync modes (Phase 2):**
- Initial sync: Full fetch of entire mailbox (all labels, not just inbox)
- Incremental sync: Uses Gmail History API to fetch only new messages
- Automatic fallback: Falls back to initial sync if history ID expires
- Messages include label_ids for filtering by folder (Inbox, Sent, etc.)

**Search (Phase 3):**
- Uses Tantivy for full-text search (pure Rust, embedded)
- Index stored at `~/Library/Application Support/cosmos/mail.search.idx/` (macOS)
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

### UniFFI Bindings (FFI Module)

The `mail` crate includes a UniFFI facade module (`src/ffi/`) that exposes the mail functionality to Swift and Kotlin.

**FFI Structure:**
- `ffi/mod.rs` - Module exports and UniFFI setup
- `ffi/types.rs` - FFI-friendly type wrappers (converts DateTime→i64, ThreadId→String)
- `ffi/service.rs` - `MailService` facade object

**MailService API:**
```rust
use mail::ffi::{MailService, MailError};

// Initialize with platform paths
let service = MailService::new(db_path, blob_path, search_path)?;

// Accounts
let accounts = service.list_accounts()?;
let account = service.register_account("user@gmail.com".into())?;

// Threads
let threads = service.list_threads(Some("INBOX"), None, 50, 0)?;
let detail = service.get_thread_detail(thread_id)?;
let (total, unread) = (service.count_threads(None, None)?, service.count_unread(None, None)?);

// Search
let results = service.search("from:alice is:unread", 50, None)?;

// Sync (pass token JSON from native OAuth)
let stats = service.sync_account(account_id, token_json, client_id, client_secret, callback)?;

// Actions
service.archive_thread(thread_id, token_json, client_id, client_secret)?;
service.set_read(thread_id, true, token_json, client_id, client_secret)?;
```

**Building XCFramework:**
```bash
# Build for all Apple platforms and generate Swift bindings
./script/build-xcframework

# Output: generated/MailFFI.xcframework + generated/mail_ffi.swift
```

The script builds for:
- macOS: arm64, x86_64
- iOS: arm64
- iOS Simulator: arm64, x86_64

### Unified Logging System

The project uses a cross-platform logging system that works across both GPUI and SwiftUI targets.

**Rust side:**
- Uses the `log` crate facade for all logging
- GPUI app: Uses `env_logger` (configured in `main.rs`)
- SwiftUI app: FFI callback routes logs to Swift via `LogCallback` trait

**Swift side:**
- Uses Apple's `os_log` / `Logger` for unified logging
- `OrionLogger` provides category-based logging (`mailBridge`, `auth`, `sync`, `ui`)
- Rust logs appear in Console.app via FFI callback

**Key files:**
- `crates/mail/src/ffi/logging.rs` - FFI log backend implementation
- `crates/mail/src/ffi/types.rs` - `LogCallback` trait and `FfiLogLevel` enum
- `apple/Orion/Sources/Services/RustLogger.swift` - Swift `LogCallback` implementation
- `apple/Orion/Sources/Services/OrionLogger.swift` - Swift logging helper

**Usage in Rust:**
```rust
log::info!("Sync completed: {} messages", count);
log::debug!("Token refreshed successfully");
log::error!("Failed to connect: {}", error);
```

**Usage in Swift:**
```swift
OrionLogger.sync.info("Sync completed")
OrionLogger.auth.error("Token expired: \(error)")
OrionLogger.ui.debug("Loading threads...")
```

**Initialization:**
```swift
// In OrionApp.swift init()
initializeRustLogging(debug: true)  // Enable debug logs
```

### SwiftUI Orion App

Located in `apple/Orion/`, this is a universal SwiftUI app (macOS + iOS) that uses the UniFFI bindings.

**Application Structure:**
```
apple/Orion/Sources/
├── App/
│   ├── OrionApp.swift      # Main entry point
│   └── ContentView.swift   # Root NavigationSplitView
├── Views/
│   ├── SidebarView.swift   # Accounts and labels navigation
│   ├── ThreadListView.swift
│   ├── ThreadDetailView.swift
│   └── SearchResultsView.swift
├── Components/
│   ├── SearchBox.swift
│   └── ShortcutsHelpView.swift
├── Services/
│   ├── MailBridge.swift    # Swift wrapper around Rust MailService
│   └── AuthService.swift   # Native OAuth via ASWebAuthenticationSession
└── Theme/
    └── OrionTheme.swift    # Colors and styling matching docs/ui.md
```

**Key Patterns:**

1. **OAuth**: Uses native `ASWebAuthenticationSession` with PKCE flow (not Rust device flow)
2. **Token Storage**: Keychain for secure OAuth token storage
3. **Threading**: Swift `async/await` bridges to sync Rust calls via background DispatchQueue
4. **Data Paths**:
   - macOS: `~/Library/Application Support/cosmos/` (shares data with GPUI app)
   - iOS: App sandbox `Application Support/cosmos/`

**Building the SwiftUI App:**
```bash
# First, build the XCFramework
./script/build-xcframework

# Then open in Xcode or build with SwiftPM
cd apple/Orion
swift build
```

### Orion Application (GPUI)

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
- `MailStore` trait - Abstract storage interface with multi-account support
- `InMemoryMailStore` - In-memory stub for testing
- `SqliteMailStore` - Persistent storage using SQLite

**Current Phase**: Uses SQLite for persistent local storage at `~/Library/Application Support/cosmos/mail.db` (macOS). The `MailStore` trait allows swapping implementations.

**Multi-Account Support**: The system supports multiple Gmail accounts with:
- Unified inbox view (all accounts combined)
- Per-account filtering via sidebar selection
- Account metadata stored in SQLite `accounts` table
- OAuth tokens can be stored per-account (in database or files)

**Future**: Real Cosmos graph storage implementations may replace SQLite in later phases.

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
- **Keep `docs/workflows.md` updated** when adding or changing build scripts, credential setup, or release processes.

## GPUI Async Runtime (CRITICAL)

**GPUI does NOT use Tokio.** It has its own async executor based on platform-native dispatch (GCD on macOS, platform equivalents on Linux/Windows). Any code using `tokio::*` will panic at runtime.

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

OAuth credentials are managed in a single location and used by both GPUI and SwiftUI apps.

### 1. Get OAuth credentials

- Go to https://console.cloud.google.com
- Create/select a project
- Enable the Gmail API
- Create OAuth client ID (Desktop app type for GPUI, iOS for SwiftUI iOS)
- Download the JSON file

### 2. Configure credentials

```bash
# Copy the template
cp secrets/google-credentials.json.template secrets/google-credentials.json

# Edit with your credentials (or copy the downloaded JSON file content)
# The file should have this structure:
# {
#   "installed": {
#     "client_id": "YOUR_CLIENT_ID.apps.googleusercontent.com",
#     "client_secret": "YOUR_CLIENT_SECRET",
#     ...
#   }
# }

# Run the setup script to configure both apps
./script/setup-credentials
```

The setup script:
- Generates `apple/Orion/Config/Secrets.xcconfig` for SwiftUI builds
- Creates a symlink at `~/Library/Application Support/cosmos/google-credentials.json` for GPUI

### 3. Run and authenticate

**GPUI App:**
```bash
cargo run -p orion
# Click "Sync" button, follow device flow prompts in terminal
```

**SwiftUI App:**
```bash
./script/run-macos  # or ./script/run-ios
# Add Account → OAuth flow in browser
```

### 4. Production builds

For production releases, credentials are embedded at compile time so no external files are needed:

**GPUI App (compile-time embedding):**
```bash
# Option 1: Use the convenience script
./script/build-gpui-release

# Option 2: Manual build with environment variables
GOOGLE_CLIENT_ID='your-client-id' \
GOOGLE_CLIENT_SECRET='your-secret' \
cargo build -p orion --release
```

The binary at `target/release/orion` will have credentials baked in and requires no external configuration.

**SwiftUI App:**
Credentials are automatically embedded via xcconfig at build time. Just build in Xcode with Release configuration.

### Credential Files

| File | Purpose |
|------|---------|
| `secrets/google-credentials.json` | Single source of truth (gitignored) |
| `secrets/google-credentials.json.template` | Template for new setups |
| `apple/Orion/Config/Secrets.xcconfig` | Generated for SwiftUI (gitignored) |
| `~/Library/Application Support/cosmos/google-credentials.json` | Symlink for GPUI |

**Data files (macOS):**
- `~/Library/Application Support/cosmos/mail.db` - SQLite database (accounts, threads, messages, sync state)
- `~/Library/Application Support/cosmos/mail.blobs/` - Blob storage for message bodies
- `~/Library/Application Support/cosmos/mail.search.idx/` - Tantivy search index directory
- `~/Library/Application Support/cosmos/gmail-tokens-{email}.json` - Per-account OAuth tokens

## Cross-Platform Feature Parity

**CRITICAL: All user-facing functionality must be implemented across ALL app targets.**

When adding new features, ensure they are implemented in:
1. **GPUI Orion** (`crates/apps/orion/`) - Desktop app for macOS, Linux, Windows
2. **SwiftUI Orion** (`apple/Orion/`) - Native app for macOS and iOS

### Shared Business Logic

**All business logic MUST live in the `mail` crate.** Both GPUI and SwiftUI apps should:
- Call the same `mail` crate functions for all email operations
- Share data models, storage, sync engine, and actions
- Never duplicate business logic in UI code

**Pattern for new features:**
1. Implement core logic in `crates/mail/src/` (e.g., new action, query, or sync behavior)
2. If Swift needs access, expose via FFI in `crates/mail/src/ffi/`
3. Rebuild XCFramework: `./script/build-xcframework`
4. Add UI in both GPUI (`crates/apps/orion/`) and SwiftUI (`apple/Orion/`)
5. Ensure feature parity in both apps before merging

### Feature Parity Checklist

Before completing any user-facing feature, verify:
- [ ] Implemented in GPUI Orion
- [ ] Implemented in SwiftUI Orion (macOS)
- [ ] Implemented in SwiftUI Orion (iOS) if applicable
- [ ] Uses shared business logic from `mail` crate
- [ ] FFI bindings updated if needed
- [ ] XCFramework rebuilt and tested

### Current App Targets

| Target | Platform | UI Framework | Location |
|--------|----------|--------------|----------|
| Orion (GPUI) | macOS, Linux, Windows | GPUI | `crates/apps/orion/` |
| Orion (SwiftUI) | macOS | SwiftUI | `apple/Orion/` |
| Orion (SwiftUI) | iOS | SwiftUI | `apple/Orion/` |

All targets share the `mail` crate for business logic via:
- Direct Rust calls (GPUI)
- UniFFI bindings via `MailService` (SwiftUI)
