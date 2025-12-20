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

        let bg_color = if self.is_selected {
            theme.list_active
        } else {
            theme.list
        };

        let border_color = if self.is_selected {
            theme.list_active_border
        } else {
            theme.border
        };

        let date_str = self.format_date();
        let message_count = self.thread.message_count;
        let subject = self.thread.subject.clone();
        let snippet = self.thread.snippet.clone();

        div()
            .w_full()
            .px_4()
            .py_3()
            .bg(bg_color)
            .border_b_1()
            .border_color(border_color)
            .cursor_pointer()
            .hover(|style| style.bg(theme.list_hover))
            .child(
                div()
                    .flex()
                    .justify_between()
                    .items_center()
                    .child(
                        // Subject and snippet
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .overflow_hidden()
                            .flex_1()
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(theme.foreground)
                                    .text_ellipsis()
                                    .child(subject),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .text_ellipsis()
                                    .child(snippet),
                            ),
                    )
                    .child(
                        // Date and message count
                        div()
                            .flex()
                            .flex_col()
                            .items_end()
                            .gap_1()
                            .flex_shrink_0()
                            .ml_4()
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
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
            )
    }
}
