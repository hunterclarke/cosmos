# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Cosmos is a Rust workspace containing desktop applications built with GPUI. Currently contains:
- **Orion** - A mail application with read-only Gmail integration (Phase 1)
- **mail** - Shared mail business logic library (UniFFI-ready, platform-independent)

See `phase_1.md` for the detailed implementation plan for Phase 1.

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
│   └── mail/               # Mail business logic (no UI deps)
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

### Mail Crate

The `mail` crate provides platform-independent mail functionality:

```rust
// Example usage in orion UI code
use mail::{sync_inbox, list_threads, MailStore, GmailClient};
```

**Key modules:**
- `models/` - Domain types (Thread, Message, EmailAddress)
- `gmail/` - Gmail API client and OAuth
- `storage/` - Storage trait abstractions
- `sync/` - Idempotent sync engine
- `query/` - Query API for UI consumption

### Orion Application

Orion is a mail application built with GPUI (v0.2.2), a GPU-accelerated UI framework for Rust desktop applications.

**Key dependencies:**
- `gpui` (0.2.2) - Core GPUI framework
- `gpui-component` (0.5.0) - GPUI component utilities
- `mail` - Business logic for mail operations

**Application structure:**
- Entry point: `src/main.rs` - Application bootstrap
- `src/app.rs` - Root app component
- `src/views/` - GPUI view components (inbox, thread)
- `src/components/` - Reusable UI components
- Uses GPUI's `Application` and `Window` APIs
- Implements the `Render` trait for UI components
- UI is built using a declarative builder pattern with methods like `div()`, `flex()`, `bg()`, etc.

**GPUI patterns used:**
- Component rendering via `Render` trait
- Window management with `WindowOptions`
- Element composition using method chaining (builder pattern)
- Styling with inline methods (colors via `rgb()`, sizing via `px()`, etc.)

**Using gpui-component:**

The `gpui-component` crate provides reusable UI components and utilities for GPUI applications. To use components from this crate:

```rust
use gpui_component::prelude::*;
// or import specific components:
use gpui_component::{Button, Input, Modal, etc.};
```

Common patterns with gpui-component:
- Import from `gpui_component::prelude::*` for commonly used component traits and utilities
- Components follow the same `Render` trait pattern as core GPUI
- Components can be composed together using the same `.child()` method chaining
- Use gpui-component for higher-level UI patterns (buttons, inputs, modals, etc.) while core GPUI provides primitives (div, text, etc.)

## Cosmos Integration

The `mail` crate depends on Cosmos OS abstractions for persistence and graph operations:
- `cosmos-storage` - Storage layer abstraction
- `cosmos-graph` - Graph database operations
- `cosmos-query` - Query API for data retrieval

**Phase 1 Strategy**: These dependencies are stubbed with in-memory implementations in `cosmos-stubs/`. The `mail` crate uses trait abstractions (e.g., `MailStore` trait) to allow swapping between stub and real implementations.

**Future**: Real Cosmos implementations will replace stubs in Phase 2+.

## Rust Edition

The project uses Rust edition 2024 (as specified in orion's Cargo.toml).

## Development Guidelines

When working on this codebase:
- **NEVER** import GPUI or UI code in the `mail` crate
- Keep business logic in `mail`, UI code in `orion`
- Use trait abstractions for side effects (storage, network, etc.)
- Ensure `mail` crate remains testable without UI
- Follow idempotent sync patterns (operations safe to retry)
