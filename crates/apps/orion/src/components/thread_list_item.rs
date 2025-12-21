//! Thread list item component - displays a single thread row in the inbox

use gpui::prelude::*;
use gpui::*;
use gpui_component::ActiveTheme;
use mail::ThreadSummary;

/// Props for ThreadListItem
#[derive(IntoElement)]
pub struct ThreadListItem {
    thread: ThreadSummary,
    is_selected: bool,
}

impl ThreadListItem {
    pub fn new(thread: ThreadSummary, is_selected: bool) -> Self {
        Self {
            thread,
            is_selected,
        }
    }

    fn format_date(&self) -> String {
        use chrono::{Local, Utc};
        let local = self.thread.last_message_at.with_timezone(&Local);
        let now = Utc::now().with_timezone(&Local);

        if local.date_naive() == now.date_naive() {
            // Today: show time
            local.format("%H:%M").to_string()
        } else if (now - local).num_days() < 7 {
            // This week: show day name
            local.format("%a").to_string()
        } else {
            // Older: show date
            local.format("%b %d").to_string()
        }
    }
}

impl RenderOnce for ThreadListItem {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let is_unread = self.thread.is_unread;

        let bg_color = if self.is_selected {
            theme.list_active
        } else {
            theme.list
        };

        let date_str = self.format_date();
        let message_count = self.thread.message_count;
        let subject = self.thread.subject.clone();
        let snippet = self.thread.snippet.clone();

        // Text styling based on unread status
        let subject_weight = if is_unread {
            FontWeight::SEMIBOLD
        } else {
            FontWeight::NORMAL
        };

        div()
            .w_full()
            .flex()
            .flex_col()
            // Divider at top (full width)
            .child(div().w_full().h_px().bg(theme.border))
            // Content row with background
            .child(
                div()
                    .w_full()
                    .bg(bg_color)
                    .cursor_pointer()
                    .hover(|style| style.bg(theme.list_hover))
                    .child(
                        // Inner content with horizontal padding
                        div()
                            .px_3()
                            .py_2()
                            .flex()
                            .items_start()
                            .gap_2()
                            // Unread indicator dot
                            .child(
                                div()
                                    .pt(px(5.)) // Align with first line of text
                                    .child(
                                        div()
                                            .w(px(6.))
                                            .h(px(6.))
                                            .rounded_full()
                                            .when(is_unread, |el| el.bg(theme.primary))
                                            .flex_shrink_0(),
                                    ),
                            )
                            // Content: subject and snippet
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_px()
                                    .overflow_hidden()
                                    .flex_1()
                                    // Subject
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_weight(subject_weight)
                                            .text_color(theme.foreground)
                                            .text_ellipsis()
                                            .child(subject),
                                    )
                                    // Snippet
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(theme.muted_foreground)
                                            .text_ellipsis()
                                            .child(snippet),
                                    ),
                            )
                            // Date and message count
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .items_end()
                                    .gap_1()
                                    .flex_shrink_0()
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(if is_unread {
                                                theme.foreground
                                            } else {
                                                theme.muted_foreground
                                            })
                                            .font_weight(if is_unread {
                                                FontWeight::MEDIUM
                                            } else {
                                                FontWeight::NORMAL
                                            })
                                            .child(date_str),
                                    )
                                    .when(message_count > 1, |el| {
                                        el.child(
                                            div()
                                                .px_2()
                                                .py_px()
                                                .bg(theme.primary)
                                                .rounded_md()
                                                .text_xs()
                                                .text_color(theme.primary_foreground)
                                                .child(format!("{}", message_count)),
                                        )
                                    }),
                            ),
                    ),
            )
    }
}
