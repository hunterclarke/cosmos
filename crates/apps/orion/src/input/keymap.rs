//! Keyboard shortcut definitions and help text
//!
//! Defines Gmail/Superhuman-style keybindings with Zed-like context management.

use gpui::KeyBinding;

use super::actions::*;
use crate::app::FocusSearch;
use crate::components::search_box;
use crate::views::search_results;

/// A category of keyboard shortcuts for display in help modal
pub struct ShortcutCategory {
    pub name: &'static str,
    pub shortcuts: Vec<Shortcut>,
}

/// A single keyboard shortcut for display
pub struct Shortcut {
    pub keys: &'static str,
    pub description: &'static str,
}

/// Returns all keybindings to register with GPUI
pub fn bindings() -> Vec<KeyBinding> {
    vec![
        // ===== Global (OrionApp context) =====
        KeyBinding::new("?", ShowShortcuts, Some("OrionApp")),
        // Dismiss: closes overlays, or ascends view hierarchy (Thread → List → Inbox)
        KeyBinding::new("escape", Dismiss, Some("OrionApp")),
        KeyBinding::new("/", FocusSearch, Some("OrionApp")),
        KeyBinding::new("cmd-k", FocusSearch, Some("OrionApp")),
        // ===== Search box =====
        KeyBinding::new("escape", search_box::Escape, Some("SearchBox")),
        // ===== Search results =====
        KeyBinding::new("k", search_results::SelectPrev, Some("SearchResultsView")),
        KeyBinding::new("up", search_results::SelectPrev, Some("SearchResultsView")),
        KeyBinding::new("j", search_results::SelectNext, Some("SearchResultsView")),
        KeyBinding::new("down", search_results::SelectNext, Some("SearchResultsView")),
        KeyBinding::new(
            "enter",
            search_results::OpenSelected,
            Some("SearchResultsView"),
        ),
        // ===== Thread list (ThreadListView context) =====
        KeyBinding::new("j", MoveDown, Some("ThreadListView")),
        KeyBinding::new("down", MoveDown, Some("ThreadListView")),
        KeyBinding::new("k", MoveUp, Some("ThreadListView")),
        KeyBinding::new("up", MoveUp, Some("ThreadListView")),
        KeyBinding::new("enter", OpenSelected, Some("ThreadListView")),
        KeyBinding::new("e", Archive, Some("ThreadListView")),
        KeyBinding::new("s", ToggleStar, Some("ThreadListView")),
        KeyBinding::new("u", ToggleRead, Some("ThreadListView")),
        KeyBinding::new("shift-3", Trash, Some("ThreadListView")), // # key
        // ===== Thread detail (ThreadView context) =====
        KeyBinding::new("e", Archive, Some("ThreadView")),
        KeyBinding::new("s", ToggleStar, Some("ThreadView")),
        KeyBinding::new("u", ToggleRead, Some("ThreadView")),
        KeyBinding::new("shift-3", Trash, Some("ThreadView")), // # key
        // ===== Go-to folder shortcuts (G sequences) =====
        // These are handled via on_key_down in app.rs for multi-key sequences
    ]
}

/// Returns categorized shortcuts for the help modal
pub fn shortcuts_help() -> Vec<ShortcutCategory> {
    vec![
        ShortcutCategory {
            name: "Navigation",
            shortcuts: vec![
                Shortcut {
                    keys: "J / ↓",
                    description: "Move down / Next",
                },
                Shortcut {
                    keys: "K / ↑",
                    description: "Move up / Previous",
                },
                Shortcut {
                    keys: "Enter",
                    description: "Open selected",
                },
                Shortcut {
                    keys: "Escape",
                    description: "Go back / Close",
                },
            ],
        },
        ShortcutCategory {
            name: "Actions",
            shortcuts: vec![
                Shortcut {
                    keys: "E",
                    description: "Archive",
                },
                Shortcut {
                    keys: "S",
                    description: "Toggle star",
                },
                Shortcut {
                    keys: "U",
                    description: "Toggle read/unread",
                },
                Shortcut {
                    keys: "#",
                    description: "Move to trash",
                },
            ],
        },
        ShortcutCategory {
            name: "Go To",
            shortcuts: vec![
                Shortcut {
                    keys: "G I",
                    description: "Go to Inbox",
                },
                Shortcut {
                    keys: "G S",
                    description: "Go to Starred",
                },
                Shortcut {
                    keys: "G T",
                    description: "Go to Sent",
                },
                Shortcut {
                    keys: "G D",
                    description: "Go to Drafts",
                },
                Shortcut {
                    keys: "G #",
                    description: "Go to Trash",
                },
                Shortcut {
                    keys: "G A",
                    description: "Go to All Mail",
                },
            ],
        },
        ShortcutCategory {
            name: "Search",
            shortcuts: vec![
                Shortcut {
                    keys: "/ or ⌘K",
                    description: "Focus search",
                },
                Shortcut {
                    keys: "Escape",
                    description: "Clear search",
                },
            ],
        },
        ShortcutCategory {
            name: "Help",
            shortcuts: vec![Shortcut {
                keys: "?",
                description: "Show this help",
            }],
        },
    ]
}
