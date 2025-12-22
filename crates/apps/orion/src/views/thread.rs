//! Thread view - displays messages within a thread

use gpui::prelude::*;
use gpui::*;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::{ActiveTheme, Icon, IconName, Sizable, Size as ComponentSize};
use mail::{MailStore, ThreadDetail, ThreadId, get_thread_detail};
use std::sync::Arc;

use crate::app::OrionApp;

/// Thread view showing messages in a conversation
///
/// The parent OrionApp manages the WebView for message content.
/// ThreadView renders just the header, and the app composes the WebView below it.
pub struct ThreadView {
    store: Arc<dyn MailStore>,
    thread_id: ThreadId,
    detail: Option<ThreadDetail>,
    is_loading: bool,
    error_message: Option<String>,
    app: Option<Entity<OrionApp>>,
}

impl ThreadView {
    pub fn new(store: Arc<dyn MailStore>, thread_id: ThreadId) -> Self {
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

    fn go_back(&mut self, cx: &mut Context<Self>) {
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
        let message_count_text = if message_count == 1 {
            "1 message".to_string()
        } else {
            format!("{} messages", message_count)
        };

        div()
            .w_full()
            .px_4()
            .py_3()
            .bg(theme.background)
            .border_b_1()
            .border_color(theme.border)
            .flex()
            .items_center()
            .gap_3()
            // Back button with icon
            .child(
                Button::new("back-button")
                    .icon(
                        Icon::new(IconName::ArrowLeft)
                            .with_size(ComponentSize::Small)
                            .text_color(theme.foreground),
                    )
                    .ghost()
                    .cursor_pointer()
                    .on_click(cx.listener(|view, _event, _window, cx| {
                        view.go_back(cx);
                    })),
            )
            // Subject and message count
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
                            .child(message_count_text),
                    ),
            )
            // Action buttons
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(
                        Button::new("reply-button")
                            .icon(
                                Icon::new(IconName::ArrowLeft)
                                    .with_size(ComponentSize::Small)
                                    .text_color(theme.muted_foreground),
                            )
                            .ghost()
                            .label("Reply"),
                    )
                    .child(
                        Button::new("archive-button")
                            .icon(
                                Icon::new(IconName::Folder)
                                    .with_size(ComponentSize::Small)
                                    .text_color(theme.muted_foreground),
                            )
                            .ghost(),
                    )
                    .child(
                        Button::new("delete-button")
                            .icon(
                                Icon::new(IconName::Delete)
                                    .with_size(ComponentSize::Small)
                                    .text_color(theme.muted_foreground),
                            )
                            .ghost(),
                    ),
            )
    }
}

impl Render for ThreadView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // ThreadView only renders the header; the app manages the WebView for message content
        self.render_header(cx)
    }
}
