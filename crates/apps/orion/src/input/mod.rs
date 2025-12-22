//! Input handling module for keyboard shortcuts
//!
//! Provides Gmail/Superhuman-style keybindings with context-aware dispatch.

pub mod actions;
pub mod keymap;

pub use actions::*;
pub use keymap::{bindings, shortcuts_help, ShortcutCategory};
