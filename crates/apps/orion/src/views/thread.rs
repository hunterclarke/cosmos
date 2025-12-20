//! Thread view - displays messages within a thread

use gpui::prelude::*;
use gpui::*;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::ActiveTheme;
use mail::{get_thread_detail, storage::InMemoryMailStore, Message, ThreadDetail, ThreadId};
use std::sync::Arc;

use crate::app::OrionApp;
use crate::components::MessageCard;

/// Thread view showing messages in a conversation
pub struct ThreadView {
    store: Arc<InMemoryMailStore>,
    thread_id: ThreadId,
    detail: Option<ThreadDetail>,
    is_loading: bool,
    error_message: Option<String>,
    app: Option<Entity<OrionApp>>,
}

impl ThreadView {
    pub fn new(store: Arc<InMemoryMailStore>, thread_id: ThreadId) -> Self {
        Self {
            store,
            thread_id,
            detail: None,
            is_loading: false,
            error_message: None,
            app: None,
        }
    }

    /// Set the parent app entity for navigation
    pub fn set_app(&mut self, app: Entity<OrionApp>) {
        self.app = Some(app);
    }

    fn go_back(&self, cx: &mut Context<Self>) {
        if let Some(app) = &self.app {
            app.update(cx, |app, cx| {
                app.show_inbox(cx);
            });
        }
    }

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
        let theme = cx.theme();
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
            .bg(theme.background)
            .border_b_1()
            .border_color(theme.border)
            .flex()
            .items_center()
            .gap_4()
            .child(
                Button::new("back-button")
                    .label("← Back")
                    .ghost()
                    .on_click(cx.listener(|view, _event, _window, cx| {
                        view.go_back(cx);
                    })),
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
                            .text_color(theme.foreground)
                            .text_ellipsis()
                            .child(subject),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(format!("{} messages", message_count)),
                    ),
            )
    }

    fn render_loading(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();

        div().flex().flex_1().justify_center().items_center().child(
            div()
                .text_sm()
                .text_color(theme.muted_foreground)
                .child("Loading thread..."),
        )
    }

    fn render_error(&self, message: &str, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();

        div()
            .flex()
            .flex_1()
            .justify_center()
            .items_center()
            .p_4()
            .child(
                div()
                    .p_4()
                    .bg(theme.danger)
                    .rounded_lg()
                    .border_1()
                    .border_color(theme.danger)
                    .child(
                        div()
                            .text_sm()
                            .text_color(theme.danger_foreground)
                            .child(message.to_string()),
                    ),
            )
    }

    fn render_messages(&self, messages: &[Message], cx: &mut Context<Self>) -> impl IntoElement {
        let messages = messages.to_vec();
        let theme = cx.theme();

        div()
            .flex()
            .flex_col()
            .flex_1()
            .p_4()
            .gap_3()
            .overflow_hidden()
            .bg(theme.background)
            .children(messages.into_iter().map(MessageCard::new))
    }
}

impl Render for ThreadView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(theme.background)
            .child(self.render_header(cx))
            .child(if self.is_loading {
                self.render_loading(cx).into_any_element()
            } else if let Some(ref error) = self.error_message.clone() {
                self.render_error(&error, cx).into_any_element()
            } else if let Some(ref detail) = self.detail.clone() {
                self.render_messages(&detail.messages, cx).into_any_element()
            } else {
                self.render_loading(cx).into_any_element()
            })
    }
}
