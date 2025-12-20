//! Message card component - displays a single message in thread view

use gpui::prelude::*;
use gpui::*;
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
}

impl RenderOnce for MessageCard {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let date_str = self.format_date();
        let sender = self.sender_display();
        let from_email = self.message.from.email.clone();
        let body = self.message.body_preview.clone();
        let to_list: Vec<String> = self.message.to.iter().map(|a| a.email.clone()).collect();
        let has_recipients = !to_list.is_empty();
        let recipients = to_list.join(", ");

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
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child(date_str),
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
                    // Body preview
                    .child(
                        div()
                            .pt_3()
                            .border_t_1()
                            .border_color(theme.border)
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(theme.secondary_foreground)
                                    .line_height(px(22.))
                                    .child(body),
                            ),
                    ),
            )
    }
}
