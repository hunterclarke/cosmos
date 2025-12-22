//! GPUI action definitions for keyboard shortcuts
//!
//! Actions are organized by context where they apply.

use gpui::actions;

// Navigation actions (thread list and thread detail)
actions!(
    orion,
    [
        MoveUp,       // K or Up arrow - select previous item
        MoveDown,     // J or Down arrow - select next item
        OpenSelected, // Enter - open selected thread
    ]
);

// Email actions (work in both list and detail views)
actions!(
    orion,
    [
        Archive,    // E - archive thread
        ToggleStar, // S - toggle star
        ToggleRead, // U - toggle read/unread
        Trash,      // # - move to trash
    ]
);

// Go-to folder actions (G sequences)
actions!(
    orion,
    [
        GoToInbox,   // G I - go to inbox
        GoToStarred, // G S - go to starred
        GoToSent,    // G T - go to sent
        GoToDrafts,  // G D - go to drafts
        GoToTrash,   // G # - go to trash
        GoToAllMail, // G A - go to all mail
    ]
);

// Utility actions
actions!(
    orion,
    [
        ShowShortcuts, // ? - show keyboard shortcuts help
        /// Dismiss current context and ascend to parent view.
        /// Hierarchy: Thread → List (search/inbox) → Inbox
        /// Also closes overlays (shortcuts modal).
        Dismiss,
    ]
);
