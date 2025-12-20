//! Inbox view - displays list of email threads

use gpui::prelude::*;
use gpui::*;
use gpui_component::ActiveTheme;
use mail::{list_threads, storage::InMemoryMailStore, ThreadId, ThreadSummary};
use std::sync::Arc;

use crate::app::OrionApp;
use crate::components::ThreadListItem;

/// Inbox view showing list of threads
pub struct InboxView {
    store: Arc<InMemoryMailStore>,
    threads: Vec<ThreadSummary>,
    selected_thread: Option<ThreadId>,
    is_loading: bool,
    error_message: Option<String>,
    app: Option<Entity<OrionApp>>,
}

impl InboxView {
    pub fn new(store: Arc<InMemoryMailStore>) -> Self {
        Self {
            store,
            threads: Vec::new(),
            selected_thread: None,
            is_loading: false,
            error_message: None,
            app: None,
        }
    }

    /// Set the parent app entity for navigation
    pub fn set_app(&mut self, app: Entity<OrionApp>) {
        self.app = Some(app);
    }

    pub fn load_threads(&mut self, _cx: &mut Context<Self>) {
        self.is_loading = true;
        self.error_message = None;

        match list_threads(self.store.as_ref(), 100, 0) {
            Ok(threads) => {
                self.threads = threads;
                self.is_loading = false;
            }
            Err(e) => {
                self.error_message = Some(format!("Failed to load threads: {}", e));
                self.is_loading = false;
            }
        }
    }

    pub fn select_thread(&mut self, thread_id: ThreadId, cx: &mut Context<Self>) {
        self.selected_thread = Some(thread_id.clone());
        // Navigate to thread view via parent app
        if let Some(app) = &self.app {
            app.update(cx, |app, cx| {
                app.show_thread(thread_id, cx);
            });
        }
    }

    fn render_header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();

        div()
            .w_full()
            .px_4()
            .py_3()
            .bg(theme.background)
            .border_b_1()
            .border_color(theme.border)
            .flex()
            .justify_between()
            .items_center()
            .child(
                div()
                    .text_lg()
                    .font_weight(FontWeight::BOLD)
                    .text_color(theme.foreground)
                    .child("Inbox"),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .child(format!("{} threads", self.threads.len())),
            )
    }

    fn render_loading(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();

        div().flex().flex_1().justify_center().items_center().child(
            div()
                .text_sm()
                .text_color(theme.muted_foreground)
                .child("Loading..."),
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

    fn render_empty(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();

        div().flex().flex_1().justify_center().items_center().child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .text_sm()
                        .text_color(theme.muted_foreground)
                        .child("No emails yet"),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child("Sync your inbox to get started"),
                ),
        )
    }

    fn render_thread_list(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let threads = self.threads.clone();
        let selected = self.selected_thread.clone();
        let theme = cx.theme();

        div()
            .flex()
            .flex_col()
            .flex_1()
            .overflow_hidden()
            .bg(theme.list)
            .children(threads.into_iter().map(|thread| {
                let is_selected = selected.as_ref().is_some_and(|s| s.0 == thread.id.0);
                let thread_id = thread.id.clone();

                div()
                    .id(ElementId::Name(thread_id.0.clone().into()))
                    .on_click(cx.listener(move |view, _event, _window, cx| {
                        view.select_thread(thread_id.clone(), cx);
                    }))
                    .child(ThreadListItem::new(thread, is_selected))
            }))
    }
}

impl Render for InboxView {
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
            } else if self.threads.is_empty() {
                self.render_empty(cx).into_any_element()
            } else {
                self.render_thread_list(cx).into_any_element()
            })
    }
}
