//! Keyboard shortcuts help modal
//!
//! Displays a modal overlay showing all available keyboard shortcuts.

use gpui::prelude::*;
use gpui::*;
use gpui_component::ActiveTheme;

use crate::input::{shortcuts_help, ShortcutCategory};

/// Shortcuts help modal component
#[derive(IntoElement)]
pub struct ShortcutsHelp {
    categories: Vec<ShortcutCategory>,
}

impl ShortcutsHelp {
    pub fn new() -> Self {
        Self {
            categories: shortcuts_help(),
        }
    }
}

impl RenderOnce for ShortcutsHelp {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();

        // Full-screen overlay with centered modal
        div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            // Semi-transparent backdrop
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .bg(hsla(0., 0., 0., 0.5)),
            )
            // Modal content - use a two-column layout to fit all content
            .child(
                div()
                    .relative()
                    .bg(theme.background)
                    .border_1()
                    .border_color(theme.border)
                    .rounded_lg()
                    .shadow_lg()
                    .p_4()
                    // Header
                    .child(
                        div()
                            .pb_3()
                            .mb_3()
                            .border_b_1()
                            .border_color(theme.border)
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_lg()
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(theme.foreground)
                                    .child("Keyboard Shortcuts"),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(theme.muted_foreground)
                                    .child("Press Escape or ? to close"),
                            ),
                    )
                    // Content - grid layout for shortcuts
                    .child(
                        div()
                            .flex()
                            .gap_8()
                            // Left column
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_4()
                                    .children(
                                        self.categories
                                            .iter()
                                            .take(3)
                                            .map(|cat| render_category(cat, &theme)),
                                    ),
                            )
                            // Right column
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_4()
                                    .children(
                                        self.categories
                                            .iter()
                                            .skip(3)
                                            .map(|cat| render_category(cat, &theme)),
                                    ),
                            ),
                    ),
            )
    }
}

fn render_category(category: &ShortcutCategory, theme: &gpui_component::theme::Theme) -> impl IntoElement {
    div()
        .min_w(px(200.))
        .flex()
        .flex_col()
        .gap_1()
        // Category name
        .child(
            div()
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme.foreground)
                .pb_1()
                .mb_1()
                .border_b_1()
                .border_color(theme.border)
                .child(category.name),
        )
        // Shortcuts
        .children(category.shortcuts.iter().map(|shortcut| {
            div()
                .flex()
                .items_center()
                .gap_3()
                // Key combination
                .child(
                    div()
                        .min_w(px(70.))
                        .px_2()
                        .py_px()
                        .bg(theme.secondary)
                        .rounded(px(4.))
                        .text_xs()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme.secondary_foreground)
                        .child(shortcut.keys),
                )
                // Description
                .child(
                    div()
                        .text_sm()
                        .text_color(theme.muted_foreground)
                        .child(shortcut.description),
                )
        }))
}
