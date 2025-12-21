//! Sidebar navigation component for mailbox folders/labels

use gpui::prelude::*;
use gpui::*;
use gpui_component::ActiveTheme;
use mail::{Label, LabelId};

/// A single item in the sidebar navigation
#[derive(IntoElement)]
pub struct SidebarItem {
    label: Label,
    is_selected: bool,
}

impl SidebarItem {
    pub fn new(label: Label, is_selected: bool) -> Self {
        Self { label, is_selected }
    }

    /// Get the display name, prettifying system labels
    fn display_name(&self) -> &str {
        let id = self.label.id.as_str();
        match id {
            LabelId::INBOX => "Inbox",
            LabelId::SENT => "Sent",
            LabelId::DRAFTS => "Drafts",
            LabelId::TRASH => "Trash",
            LabelId::SPAM => "Spam",
            LabelId::STARRED => "Starred",
            LabelId::IMPORTANT => "Important",
            LabelId::ALL_MAIL => "All Mail",
            _ => &self.label.name,
        }
    }
}

impl RenderOnce for SidebarItem {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();

        // Use list_active colors for selected state
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

        let border_color = if self.is_selected {
            theme.list_active_border
        } else {
            theme.transparent
        };

        let display_name = self.display_name().to_string();
        let unread_count = self.label.unread_count;

        div()
            .w_full()
            .px_3()
            .py_2()
            .rounded_md()
            .bg(bg_color)
            .border_l_2()
            .border_color(border_color)
            .cursor_pointer()
            .hover(|style| style.bg(theme.list_hover))
            .flex()
            .justify_between()
            .items_center()
            .child(
                div()
                    .text_sm()
                    .text_color(text_color)
                    .font_weight(if self.is_selected {
                        FontWeight::MEDIUM
                    } else {
                        FontWeight::NORMAL
                    })
                    .child(display_name),
            )
            .when(unread_count > 0, |el| {
                el.child(
                    div()
                        .px_2()
                        .py_px()
                        .rounded_md()
                        .bg(theme.primary)
                        .text_xs()
                        .text_color(theme.primary_foreground)
                        .font_weight(FontWeight::MEDIUM)
                        .child(format!("{}", unread_count)),
                )
            })
    }
}

/// Sidebar utilities
pub struct Sidebar;

impl Sidebar {
    /// Get default system labels for when no labels are loaded
    pub fn default_labels() -> Vec<Label> {
        vec![
            Label::system(LabelId::INBOX, "Inbox"),
            Label::system(LabelId::STARRED, "Starred"),
            Label::system(LabelId::SENT, "Sent"),
            Label::system(LabelId::DRAFTS, "Drafts"),
            Label::system(LabelId::ALL_MAIL, "All Mail"),
            Label::system(LabelId::SPAM, "Spam"),
            Label::system(LabelId::TRASH, "Trash"),
        ]
    }
}
