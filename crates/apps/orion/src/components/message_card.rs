//! Message card component - displays a single message in thread view

use gpui::prelude::*;
use gpui::*;
use gpui_component::text::TextView;
use gpui_component::ActiveTheme;
use mail::Message;

/// Props for MessageCard
#[derive(IntoElement)]
pub struct MessageCard {
    message: Message,
}

impl MessageCard {
    pub fn new(message: Message) -> Self {
        Self { message }
    }

    fn format_date(&self) -> String {
        use chrono::Local;
        let local = self.message.received_at.with_timezone(&Local);
        local.format("%b %d, %Y at %H:%M").to_string()
    }

    fn sender_display(&self) -> String {
        self.message
            .from
            .name
            .clone()
            .unwrap_or_else(|| self.message.from.email.clone())
    }

    /// Get plain text body content for display
    fn plain_text_body(&self) -> String {
        // Prefer plain text body
        if let Some(ref text) = self.message.body_text {
            if !text.is_empty() {
                return text.clone();
            }
        }
        // Fall back to preview
        self.message.body_preview.clone()
    }

    /// Check if HTML content is available
    fn has_html(&self) -> bool {
        self.message
            .body_html
            .as_ref()
            .is_some_and(|h| !h.is_empty())
    }
}

impl RenderOnce for MessageCard {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let date_str = self.format_date();
        let sender = self.sender_display();
        let from_email = self.message.from.email.clone();
        let message_id = self.message.id.0.clone();
        let has_html = self.has_html();
        let plain_text = self.plain_text_body();
        let to_list: Vec<String> = self.message.to.iter().map(|a| a.email.clone()).collect();
        let has_recipients = !to_list.is_empty();
        let recipients = to_list.join(", ");

        // Render body content - use HTML if available, otherwise plain text
        let body_element: AnyElement = if let Some(ref html) = self.message.body_html {
            if !html.is_empty() {
                let id = SharedString::from(format!("msg-{}", message_id));
                TextView::html(id, html.clone(), window, cx)
                    .selectable(true)
                    .into_any_element()
            } else {
                let theme = cx.theme();
                div()
                    .text_sm()
                    .text_color(theme.secondary_foreground)
                    .line_height(px(22.))
                    .child(plain_text.clone())
                    .into_any_element()
            }
        } else {
            let theme = cx.theme();
            div()
                .text_sm()
                .text_color(theme.secondary_foreground)
                .line_height(px(22.))
                .child(plain_text)
                .into_any_element()
        };

        let theme = cx.theme();

        div()
            .w_full()
            .p_4()
            .bg(theme.secondary)
            .rounded_lg()
            .border_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    // Header: From and Date
                    .child(
                        div()
                            .flex()
                            .justify_between()
                            .items_center()
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(theme.foreground)
                                            .child(sender),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(theme.muted_foreground)
                                            .child(from_email),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    // HTML indicator
                                    .when(has_html, |el| {
                                        el.child(
                                            div()
                                                .px_2()
                                                .py_1()
                                                .rounded(px(4.))
                                                .bg(theme.primary)
                                                .text_xs()
                                                .text_color(theme.primary_foreground)
                                                .child("HTML"),
                                        )
                                    })
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(theme.muted_foreground)
                                            .child(date_str),
                                    ),
                            ),
                    )
                    // Recipients
                    .when(has_recipients, |el| {
                        el.child(
                            div()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child(format!("To: {}", recipients)),
                        )
                    })
                    // Body content
                    .child(
                        div()
                            .pt_3()
                            .border_t_1()
                            .border_color(theme.border)
                            .child(body_element),
                    ),
            )
    }
}
