//! Thread list view - displays list of email threads filtered by label

use gpui::prelude::*;
use gpui::*;
use gpui_component::scroll::Scrollbar;
use gpui_component::spinner::Spinner;
use gpui_component::{v_virtual_list, ActiveTheme, Sizable, Size as ComponentSize, VirtualListScrollHandle};
use log::{debug, error, info};
use mail::{list_threads, list_threads_by_label, MailStore, ThreadId, ThreadSummary};
use std::rc::Rc;
use std::sync::Arc;

use crate::app::OrionApp;
use crate::components::ThreadListItem;

/// Height of each thread list item (2 lines: subject, snippet + padding)
const THREAD_ITEM_HEIGHT: f32 = 56.0;

/// Thread list view showing threads filtered by label
pub struct ThreadListView {
    store: Arc<dyn MailStore>,
    threads: Vec<ThreadSummary>,
    selected_thread: Option<ThreadId>,
    is_loading: bool,
    error_message: Option<String>,
    app: Option<Entity<OrionApp>>,
    scroll_handle: VirtualListScrollHandle,
    item_sizes: Rc<Vec<Size<Pixels>>>,
    /// Current label filter (e.g., "INBOX", "SENT", etc.)
    label_filter: Option<String>,
}

impl ThreadListView {
    pub fn new(store: Arc<dyn MailStore>) -> Self {
        Self {
            store,
            threads: Vec::new(),
            selected_thread: None,
            is_loading: false,
            error_message: None,
            app: None,
            scroll_handle: VirtualListScrollHandle::new(),
            item_sizes: Rc::new(Vec::new()),
            label_filter: Some("INBOX".to_string()),
        }
    }

    /// Set the parent app entity for navigation
    pub fn set_app(&mut self, app: Entity<OrionApp>) {
        self.app = Some(app);
    }

    /// Set the label filter and reload threads
    pub fn set_label_filter(&mut self, label: String, cx: &mut Context<Self>) {
        self.label_filter = Some(label);
        self.load_threads(cx);
        cx.notify();
    }

    /// Get the display name for the current label
    fn current_label_name(&self) -> &str {
        match self.label_filter.as_deref() {
            Some("INBOX") => "Inbox",
            Some("SENT") => "Sent",
            Some("DRAFT") => "Drafts",
            Some("TRASH") => "Trash",
            Some("SPAM") => "Spam",
            Some("STARRED") => "Starred",
            Some("IMPORTANT") => "Important",
            Some("ALL") => "All Mail",
            Some(other) => other,
            None => "All Mail",
        }
    }

    pub fn load_threads(&mut self, _cx: &mut Context<Self>) {
        self.is_loading = true;
        self.error_message = None;

        // Use storage-layer filtering for efficiency
        // "ALL" means all mail - no filtering
        let result = match self.label_filter.as_deref() {
            None | Some("ALL") => {
                debug!("Loading all threads (no filter)");
                list_threads(self.store.as_ref(), 500, 0)
            }
            Some(label) => {
                debug!("Loading threads with label filter: {}", label);
                list_threads_by_label(self.store.as_ref(), label, 500, 0)
            }
        };

        match result {
            Ok(threads) => {
                info!("Loaded {} threads", threads.len());

                // Update item sizes for virtual list
                self.item_sizes = Rc::new(
                    threads
                        .iter()
                        .map(|_| size(px(10000.), px(THREAD_ITEM_HEIGHT)))
                        .collect(),
                );
                self.threads = threads;
                self.is_loading = false;
            }
            Err(e) => {
                error!("Failed to load threads: {}", e);
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
        let label_name = self.current_label_name().to_string();

        // Count total threads and unread threads
        let total_count = self.threads.len();
        let unread_count = self.threads.iter().filter(|t| t.is_unread).count();

        let stats_text = if unread_count > 0 {
            format!("{} messages, {} unread", total_count, unread_count)
        } else {
            format!("{} messages", total_count)
        };

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
                    .child(label_name),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .child(stats_text),
            )
    }

    fn render_loading(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();

        div()
            .flex()
            .flex_1()
            .flex_col()
            .justify_center()
            .items_center()
            .gap_2()
            .child(Spinner::new().with_size(ComponentSize::Medium))
            .child(
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
        let theme = cx.theme();

        div()
            .relative()
            .flex()
            .flex_col()
            .flex_1()
            .overflow_hidden()
            .bg(theme.list)
            .child(
                v_virtual_list(
                    cx.entity().clone(),
                    "thread-list",
                    self.item_sizes.clone(),
                    |view, visible_range, _window, cx| {
                        visible_range
                            .map(|ix| {
                                let thread = view.threads[ix].clone();
                                let is_selected = view
                                    .selected_thread
                                    .as_ref()
                                    .is_some_and(|s| s.0 == thread.id.0);
                                let thread_id = thread.id.clone();

                                div()
                                    .id(ElementId::Name(thread_id.0.clone().into()))
                                    .h(px(THREAD_ITEM_HEIGHT))
                                    .w_full()
                                    .cursor_pointer()
                                    .on_click(cx.listener(move |view, _event, _window, cx| {
                                        view.select_thread(thread_id.clone(), cx);
                                    }))
                                    .child(ThreadListItem::new(thread, is_selected))
                            })
                            .collect()
                    },
                )
                .flex_1()
                .track_scroll(&self.scroll_handle),
            )
            .child(Scrollbar::vertical(&self.scroll_handle))
    }
}

impl Render for ThreadListView {
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
                self.render_error(error, cx).into_any_element()
            } else if self.threads.is_empty() {
                self.render_empty(cx).into_any_element()
            } else {
                self.render_thread_list(cx).into_any_element()
            })
    }
}
