//! Thread list item component - displays a single thread row in the inbox

use gpui::prelude::*;
use gpui::*;
use mail::ThreadSummary;

/// Props for ThreadListItem
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

impl IntoElement for ThreadListItem {
    type Element = AnyElement;

    fn into_element(self) -> Self::Element {
        let bg_color = if self.is_selected {
            rgb(0x3a3a5a)
        } else {
            rgb(0x2a2a3a)
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
            .border_color(rgb(0x404050))
            .cursor_pointer()
            .hover(|style| style.bg(rgb(0x3a3a4a)))
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
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(rgb(0xffffff))
                                    .text_ellipsis()
                                    .child(subject),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(0x888899))
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
                            .child(div().text_xs().text_color(rgb(0x888899)).child(date_str))
                            .when(message_count > 1, |el| {
                                el.child(
                                    div()
                                        .px_2()
                                        .py_px()
                                        .bg(rgb(0x4a4a6a))
                                        .rounded_md()
                                        .text_xs()
                                        .text_color(rgb(0xccccdd))
                                        .child(format!("{}", message_count)),
                                )
                            }),
                    ),
            )
            .into_any_element()
    }
}
