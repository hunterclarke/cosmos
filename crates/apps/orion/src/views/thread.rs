//! Thread view - displays messages within a thread

use gpui::prelude::*;
use gpui::*;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::{ActiveTheme, Icon, IconName, Sizable, Size as ComponentSize};

use crate::app::OrionApp;
use crate::assets::icons::{Archive, MailOpen};
use crate::input::{self, ToggleRead, ToggleStar, Trash};
use mail::{get_thread_detail, MailStore, ThreadDetail, ThreadId};
use std::sync::Arc;

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
    focus_handle: FocusHandle,
}

impl ThreadView {
    pub fn new(store: Arc<dyn MailStore>, thread_id: ThreadId, cx: &mut Context<Self>) -> Self {
        Self {
            store,
            thread_id,
            detail: None,
            is_loading: false,
            error_message: None,
            app: None,
            focus_handle: cx.focus_handle(),
        }
    }

    /// Focus this view for keyboard input
    pub fn focus(&self, window: &mut Window, _cx: &mut Context<Self>) {
        window.focus(&self.focus_handle);
    }

    /// Set the parent app entity for navigation
    pub fn set_app(&mut self, app: Entity<OrionApp>) {
        self.app = Some(app);
    }

    /// Dismiss this thread view (back button click)
    fn dismiss(&mut self, cx: &mut Context<Self>) {
        if let Some(app) = &self.app {
            app.update(cx, |app, cx| {
                app.dismiss(cx);
            });
        }
    }

    // Action handlers for keyboard shortcuts
    fn handle_archive(&mut self, _: &input::Archive, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(app) = &self.app {
            app.update(cx, |app, cx| {
                app.archive_current_thread(cx);
            });
        }
    }

    fn handle_toggle_star(&mut self, _: &ToggleStar, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(app) = &self.app {
            app.update(cx, |app, cx| {
                app.toggle_star_current_thread(cx);
            });
        }
    }

    fn handle_toggle_read(&mut self, _: &ToggleRead, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(app) = &self.app {
            app.update(cx, |app, cx| {
                app.toggle_read_current_thread(cx);
            });
        }
    }

    fn handle_trash(&mut self, _: &Trash, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(app) = &self.app {
            app.update(cx, |app, cx| {
                app.trash_current_thread(cx);
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
                        view.dismiss(cx);
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
                    // Archive button
                    .child(
                        Button::new("archive-button")
                            .icon(
                                Icon::new(Archive)
                                    .with_size(ComponentSize::Small)
                                    .text_color(theme.muted_foreground),
                            )
                            .ghost()
                            .cursor_pointer()
                            .on_click(cx.listener(|view, _event, _window, cx| {
                                if let Some(app) = &view.app {
                                    app.update(cx, |app, cx| {
                                        app.archive_current_thread(cx);
                                    });
                                }
                            })),
                    )
                    // Star button
                    .child(
                        Button::new("star-button")
                            .icon(
                                Icon::new(IconName::Star)
                                    .with_size(ComponentSize::Small)
                                    .text_color(theme.muted_foreground),
                            )
                            .ghost()
                            .cursor_pointer()
                            .on_click(cx.listener(|view, _event, _window, cx| {
                                if let Some(app) = &view.app {
                                    app.update(cx, |app, cx| {
                                        app.toggle_star_current_thread(cx);
                                    });
                                }
                            })),
                    )
                    // Read/Unread button
                    .child(
                        Button::new("read-button")
                            .icon(
                                Icon::new(MailOpen)
                                    .with_size(ComponentSize::Small)
                                    .text_color(theme.muted_foreground),
                            )
                            .ghost()
                            .cursor_pointer()
                            .on_click(cx.listener(|view, _event, _window, cx| {
                                if let Some(app) = &view.app {
                                    app.update(cx, |app, cx| {
                                        app.toggle_read_current_thread(cx);
                                    });
                                }
                            })),
                    )
                    // Delete/Trash button
                    .child(
                        Button::new("delete-button")
                            .icon(
                                Icon::new(IconName::Delete)
                                    .with_size(ComponentSize::Small)
                                    .text_color(theme.muted_foreground),
                            )
                            .ghost()
                            .cursor_pointer()
                            .on_click(cx.listener(|view, _event, _window, cx| {
                                if let Some(app) = &view.app {
                                    app.update(cx, |app, cx| {
                                        app.trash_current_thread(cx);
                                    });
                                }
                            })),
                    ),
            )
    }
}

impl Render for ThreadView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // ThreadView only renders the header; the app manages the WebView for message content
        // Wrap in a div with key context for keyboard shortcuts
        // Note: Escape is handled at OrionApp level via Dismiss action
        div()
            .key_context("ThreadView")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::handle_archive))
            .on_action(cx.listener(Self::handle_toggle_star))
            .on_action(cx.listener(Self::handle_toggle_read))
            .on_action(cx.listener(Self::handle_trash))
            .child(self.render_header(cx))
    }
}
