//! Thread list item component - displays a single thread row in the inbox
//! Uses Gmail-style single-line layout: Sender | Subject - preview | Date

use gpui::prelude::*;
use gpui::*;
use gpui_component::ActiveTheme;
use mail::ThreadSummary;

/// Props for ThreadListItem
#[derive(IntoElement)]
pub struct ThreadListItem {
    thread: ThreadSummary,
    is_selected: bool,
    /// Account email to show in unified view (None = single account, no need to show)
    account_email: Option<String>,
}

impl ThreadListItem {
    pub fn new(thread: ThreadSummary, is_selected: bool) -> Self {
        Self {
            thread,
            is_selected,
            account_email: None,
        }
    }

    /// Set the account email to display (for unified view)
    pub fn with_account(mut self, email: Option<String>) -> Self {
        self.account_email = email;
        self
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

        // Sender display: name or email
        let sender_display = self
            .thread
            .sender_name
            .clone()
            .unwrap_or_else(|| self.thread.sender_email.clone());

        // Text styling based on unread status
        let text_weight = if is_unread {
            FontWeight::SEMIBOLD
        } else {
            FontWeight::NORMAL
        };

        div()
            .w_full()
            .h_full()
            .bg(bg_color)
            .border_b_1()
            .border_color(theme.border)
            .cursor_pointer()
            .hover(|style| style.bg(theme.list_hover))
            .child(
                // Single row layout
                div()
                    .px_3()
                    .h_full()
                    .flex()
                    .items_center()
                    .gap_2()
                    // Unread indicator dot
                    .child(
                        div()
                            .w(px(6.))
                            .h(px(6.))
                            .rounded_full()
                            .flex_shrink_0()
                            .when(is_unread, |el| el.bg(theme.primary)),
                    )
                    // Column 1: Sender with message count
                    .child(
                        div()
                            .w(px(180.))
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .gap_1()
                            .overflow_hidden()
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(text_weight)
                                    .text_color(theme.foreground)
                                    .text_ellipsis()
                                    .child(sender_display),
                            )
                            .when(message_count > 1, |el| {
                                el.child(
                                    div()
                                        .text_xs()
                                        .text_color(theme.muted_foreground)
                                        .flex_shrink_0()
                                        .child(format!("({})", message_count)),
                                )
                            }),
                    )
                    // Column 2: Subject - preview (fills remaining space)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .items_center()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(text_weight)
                                    .text_color(theme.foreground)
                                    .flex_shrink_0()
                                    .child(subject),
                            )
                            .when(!snippet.is_empty(), |el| {
                                el.child(
                                    div()
                                        .text_sm()
                                        .text_color(theme.muted_foreground)
                                        .ml_1()
                                        .text_ellipsis()
                                        .child(format!("- {}", snippet)),
                                )
                            }),
                    )
                    // Column 3: Account email (unified view only)
                    .when_some(self.account_email, |el, email| {
                        el.child(
                            div()
                                .w(px(140.))
                                .flex_shrink_0()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .text_ellipsis()
                                .overflow_hidden()
                                .child(email),
                        )
                    })
                    // Column 4: Date (right-aligned)
                    .child(
                        div()
                            .flex_shrink_0()
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
                    ),
            )
    }
}
