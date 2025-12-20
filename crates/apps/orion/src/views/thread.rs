//! Thread view - displays messages within a thread

use gpui::prelude::*;
use gpui::*;
use mail::{Message, ThreadDetail, ThreadId, get_thread_detail, storage::InMemoryMailStore};
use std::sync::Arc;

use crate::components::MessageCard;

/// Thread view showing messages in a conversation
#[allow(dead_code)]
pub struct ThreadView {
    store: Arc<InMemoryMailStore>,
    thread_id: ThreadId,
    detail: Option<ThreadDetail>,
    is_loading: bool,
    error_message: Option<String>,
}

impl ThreadView {
    #[allow(dead_code)]
    pub fn new(store: Arc<InMemoryMailStore>, thread_id: ThreadId) -> Self {
        Self {
            store,
            thread_id,
            detail: None,
            is_loading: false,
            error_message: None,
        }
    }

    #[allow(dead_code)]
    pub fn load_thread(&mut self, _cx: &mut Context<Self>) {
        self.is_loading = true;
        self.error_message = None;

        match get_thread_detail(self.store.as_ref(), &self.thread_id) {
            Ok(Some(detail)) => {
                self.detail = Some(detail);
                self.is_loading = false;
            }
            Ok(None) => {
                self.error_message = Some("Thread not found".to_string());
                self.is_loading = false;
            }
            Err(e) => {
                self.error_message = Some(format!("Failed to load thread: {}", e));
                self.is_loading = false;
            }
        }
    }

    fn render_header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let subject = self
            .detail
            .as_ref()
            .map(|d| d.thread.subject.clone())
            .unwrap_or_else(|| "Loading...".to_string());

        let message_count = self.detail.as_ref().map(|d| d.messages.len()).unwrap_or(0);

        div()
            .w_full()
            .px_4()
            .py_3()
            .bg(rgb(0x1a1a2a))
            .border_b_1()
            .border_color(rgb(0x404050))
            .flex()
            .items_center()
            .gap_4()
            .child(
                div()
                    .id("back-button")
                    .px_3()
                    .py_2()
                    .rounded_md()
                    .bg(rgb(0x2a2a3a))
                    .cursor_pointer()
                    .hover(|style| style.bg(rgb(0x3a3a4a)))
                    .on_click(cx.listener(|_view, _event, _window, _cx| {
                        // Navigation handled by parent
                    }))
                    .child(div().text_sm().text_color(rgb(0xccccdd)).child("← Back")),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .overflow_hidden()
                    .child(
                        div()
                            .text_lg()
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(0xffffff))
                            .text_ellipsis()
                            .child(subject),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(0x888899))
                            .child(format!("{} messages", message_count)),
                    ),
            )
    }

    fn render_loading(&self) -> impl IntoElement {
        div().flex().flex_1().justify_center().items_center().child(
            div()
                .text_sm()
                .text_color(rgb(0x888899))
                .child("Loading thread..."),
        )
    }

    fn render_error(&self, message: &str) -> impl IntoElement {
        div()
            .flex()
            .flex_1()
            .justify_center()
            .items_center()
            .p_4()
            .child(
                div()
                    .p_4()
                    .bg(rgb(0x4a2a2a))
                    .rounded_lg()
                    .border_1()
                    .border_color(rgb(0x6a3a3a))
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(0xffaaaa))
                            .child(message.to_string()),
                    ),
            )
    }

    fn render_messages(&self, messages: &[Message]) -> impl IntoElement {
        let messages = messages.to_vec();

        div()
            .flex()
            .flex_col()
            .flex_1()
            .p_4()
            .overflow_hidden()
            .children(messages.into_iter().map(MessageCard::new))
    }
}

impl Render for ThreadView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(0x1a1a2a))
            .child(self.render_header(cx))
            .child(if self.is_loading {
                self.render_loading().into_any_element()
            } else if let Some(ref error) = self.error_message {
                self.render_error(error).into_any_element()
            } else if let Some(ref detail) = self.detail {
                self.render_messages(&detail.messages).into_any_element()
            } else {
                self.render_loading().into_any_element()
            })
    }
}
