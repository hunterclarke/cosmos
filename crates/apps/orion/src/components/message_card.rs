//! Message card component - displays a single message in thread view

use gpui::prelude::*;
use gpui::*;
use mail::Message;

/// Props for MessageCard
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

impl IntoElement for MessageCard {
    type Element = AnyElement;

    fn into_element(self) -> Self::Element {
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
            .mb_3()
            .bg(rgb(0x2a2a3a))
            .rounded_lg()
            .border_1()
            .border_color(rgb(0x404050))
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
                                            .text_color(rgb(0xffffff))
                                            .child(sender),
                                    )
                                    .child(
                                        div().text_xs().text_color(rgb(0x888899)).child(from_email),
                                    ),
                            )
                            .child(div().text_xs().text_color(rgb(0x888899)).child(date_str)),
                    )
                    // Recipients
                    .when(has_recipients, |el| {
                        el.child(
                            div()
                                .text_xs()
                                .text_color(rgb(0x666677))
                                .child(format!("To: {}", recipients)),
                        )
                    })
                    // Body preview
                    .child(
                        div().pt_2().border_t_1().border_color(rgb(0x404050)).child(
                            div()
                                .text_sm()
                                .text_color(rgb(0xccccdd))
                                .line_height(px(22.))
                                .child(body),
                        ),
                    ),
            )
            .into_any_element()
    }
}
