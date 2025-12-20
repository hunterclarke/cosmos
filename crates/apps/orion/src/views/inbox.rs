//! Inbox view - displays list of email threads

use gpui::prelude::*;
use gpui::*;
use mail::{ThreadId, ThreadSummary, list_threads, storage::InMemoryMailStore};
use std::sync::Arc;

use crate::components::ThreadListItem;

/// Inbox view showing list of threads
pub struct InboxView {
    store: Arc<InMemoryMailStore>,
    threads: Vec<ThreadSummary>,
    selected_thread: Option<ThreadId>,
    is_loading: bool,
    error_message: Option<String>,
}

impl InboxView {
    pub fn new(store: Arc<InMemoryMailStore>) -> Self {
        Self {
            store,
            threads: Vec::new(),
            selected_thread: None,
            is_loading: false,
            error_message: None,
        }
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

    pub fn select_thread(&mut self, thread_id: ThreadId, _cx: &mut Context<Self>) {
        self.selected_thread = Some(thread_id);
    }

    #[allow(dead_code)]
    pub fn selected_thread(&self) -> Option<&ThreadId> {
        self.selected_thread.as_ref()
    }

    fn render_header(&self, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .w_full()
            .px_4()
            .py_3()
            .bg(rgb(0x1a1a2a))
            .border_b_1()
            .border_color(rgb(0x404050))
            .flex()
            .justify_between()
            .items_center()
            .child(
                div()
                    .text_lg()
                    .font_weight(FontWeight::BOLD)
                    .text_color(rgb(0xffffff))
                    .child("Inbox"),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(0x888899))
                    .child(format!("{} threads", self.threads.len())),
            )
    }

    fn render_loading(&self) -> impl IntoElement {
        div().flex().flex_1().justify_center().items_center().child(
            div()
                .text_sm()
                .text_color(rgb(0x888899))
                .child("Loading..."),
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

    fn render_empty(&self) -> impl IntoElement {
        div().flex().flex_1().justify_center().items_center().child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap_2()
                .child(div().text_2xl().child("📭"))
                .child(
                    div()
                        .text_sm()
                        .text_color(rgb(0x888899))
                        .child("No emails yet"),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(0x666677))
                        .child("Sync your inbox to get started"),
                ),
        )
    }

    fn render_thread_list(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let threads = self.threads.clone();
        let selected = self.selected_thread.clone();

        div()
            .flex()
            .flex_col()
            .flex_1()
            .overflow_hidden()
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
            } else if self.threads.is_empty() {
                self.render_empty().into_any_element()
            } else {
                self.render_thread_list(cx).into_any_element()
            })
    }
}
