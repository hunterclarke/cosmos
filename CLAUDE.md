# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Cosmos is a Rust workspace containing desktop applications built with GPUI. Currently contains the Orion application, a GPUI-based GUI application.

## Build System

This is a Cargo workspace project. The workspace root is at `/Users/hclarke/Projects/cosmos/` with member crates under `crates/`.

**Build commands:**
```bash
# Build the entire workspace
cargo build

# Build a specific app (e.g., orion)
cargo build -p orion

# Build in release mode
cargo build --release
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

# Run a single test
cargo test -p orion test_name
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

- `crates/apps/orion/` - The Orion desktop application
- Workspace uses Cargo resolver version 3

### Orion Application

Orion is built with GPUI (v0.2.2), a GPU-accelerated UI framework for Rust desktop applications.

**Key dependencies:**
- `gpui` (0.2.2) - Core GPUI framework
- `gpui-component` (0.5.0) - GPUI component utilities

**Application structure:**
- Entry point: `src/main.rs`
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

## Rust Edition

The project uses Rust edition 2024 (as specified in orion's Cargo.toml).
