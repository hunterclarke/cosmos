//! Thread list view - displays list of email threads filtered by label

use gpui::prelude::*;
use gpui::*;
use gpui_component::scroll::Scrollbar;
use gpui_component::skeleton::Skeleton;
use gpui_component::{ActiveTheme, VirtualListScrollHandle, v_virtual_list};
use log::{debug, error, info};
use mail::{MailStore, ThreadId, ThreadSummary, list_threads, list_threads_by_label};
use std::rc::Rc;
use std::sync::Arc;

use crate::app::OrionApp;
use crate::components::ThreadListItem;
use crate::input::{Archive, MoveDown, MoveUp, OpenSelected, ToggleRead, ToggleStar, Trash};

/// Height of each thread list item (2 lines: subject, snippet + padding)
const THREAD_ITEM_HEIGHT: f32 = 56.0;

/// Thread list view showing threads filtered by label
pub struct ThreadListView {
    store: Arc<dyn MailStore>,
    threads: Vec<ThreadSummary>,
    selected_thread: Option<ThreadId>,
    /// Index of currently selected item for keyboard navigation
    selected_index: Option<usize>,
    is_loading: bool,
    /// True while waiting for persistent storage to load in background
    is_store_loading: bool,
    error_message: Option<String>,
    app: Option<Entity<OrionApp>>,
    scroll_handle: VirtualListScrollHandle,
    item_sizes: Rc<Vec<Size<Pixels>>>,
    /// Current label filter (e.g., "INBOX", "SENT", etc.)
    label_filter: Option<String>,
    /// Focus handle for keyboard input
    focus_handle: FocusHandle,
}

impl ThreadListView {
    pub fn new(store: Arc<dyn MailStore>, cx: &mut Context<Self>) -> Self {
        Self {
            store,
            threads: Vec::new(),
            selected_thread: None,
            selected_index: None,
            is_loading: false,
            is_store_loading: true, // Start in loading state until real store is set
            error_message: None,
            app: None,
            scroll_handle: VirtualListScrollHandle::new(),
            item_sizes: Rc::new(Vec::new()),
            label_filter: Some("INBOX".to_string()),
            focus_handle: cx.focus_handle(),
        }
    }

    /// Focus this view for keyboard input
    pub fn focus(&self, window: &mut Window, _cx: &mut Context<Self>) {
        window.focus(&self.focus_handle);
    }

    /// Move selection up (previous item)
    fn move_up(&mut self, cx: &mut Context<Self>) {
        if self.threads.is_empty() {
            return;
        }
        let max_index = self.threads.len() - 1;
        // Clamp current index to valid range (list may have changed)
        let current = self.selected_index.map(|i| i.min(max_index));
        let new_index = match current {
            Some(i) if i > 0 => i - 1,
            Some(_) => 0, // Already at top
            None => 0,    // Select first item
        };
        self.selected_index = Some(new_index);
        self.selected_thread = Some(self.threads[new_index].id.clone());
        cx.notify();
    }

    /// Move selection down (next item)
    fn move_down(&mut self, cx: &mut Context<Self>) {
        if self.threads.is_empty() {
            return;
        }
        let max_index = self.threads.len() - 1;
        // Clamp current index to valid range (list may have changed)
        let current = self.selected_index.map(|i| i.min(max_index));
        let new_index = match current {
            Some(i) if i < max_index => i + 1,
            Some(_) => max_index, // Already at bottom
            None => 0,            // Select first item
        };
        self.selected_index = Some(new_index);
        self.selected_thread = Some(self.threads[new_index].id.clone());
        cx.notify();
    }

    /// Open the currently selected thread
    fn open_selected(&mut self, cx: &mut Context<Self>) {
        if let Some(index) = self.selected_index {
            if let Some(thread) = self.threads.get(index) {
                let thread_id = thread.id.clone();
                if let Some(app) = &self.app {
                    app.update(cx, |app, cx| {
                        app.show_thread(thread_id, cx);
                    });
                }
            }
        }
    }

    /// Archive the selected thread
    fn archive_selected(&mut self, cx: &mut Context<Self>) {
        if let Some(app) = &self.app {
            if let Some(index) = self.selected_index {
                if let Some(thread) = self.threads.get(index) {
                    let thread_id = thread.id.clone();
                    app.update(cx, |app, cx| {
                        // Navigate to thread first so archive_current_thread works
                        app.show_thread(thread_id, cx);
                        app.archive_current_thread(cx);
                    });
                }
            }
        }
    }

    /// Toggle star on selected thread
    fn toggle_star_selected(&mut self, cx: &mut Context<Self>) {
        if let Some(app) = &self.app {
            if let Some(index) = self.selected_index {
                if let Some(thread) = self.threads.get(index) {
                    let thread_id = thread.id.clone();
                    app.update(cx, |app, cx| {
                        app.show_thread(thread_id, cx);
                        app.toggle_star_current_thread(cx);
                    });
                }
            }
        }
    }

    /// Toggle read status on selected thread
    fn toggle_read_selected(&mut self, cx: &mut Context<Self>) {
        if let Some(app) = &self.app {
            if let Some(index) = self.selected_index {
                if let Some(thread) = self.threads.get(index) {
                    let thread_id = thread.id.clone();
                    app.update(cx, |app, cx| {
                        app.show_thread(thread_id, cx);
                        app.toggle_read_current_thread(cx);
                    });
                }
            }
        }
    }

    /// Trash the selected thread
    fn trash_selected(&mut self, cx: &mut Context<Self>) {
        if let Some(app) = &self.app {
            if let Some(index) = self.selected_index {
                if let Some(thread) = self.threads.get(index) {
                    let thread_id = thread.id.clone();
                    app.update(cx, |app, cx| {
                        app.show_thread(thread_id, cx);
                        app.trash_current_thread(cx);
                    });
                }
            }
        }
    }

    // Action handlers
    fn handle_move_up(&mut self, _: &MoveUp, _window: &mut Window, cx: &mut Context<Self>) {
        self.move_up(cx);
    }

    fn handle_move_down(&mut self, _: &MoveDown, _window: &mut Window, cx: &mut Context<Self>) {
        self.move_down(cx);
    }

    fn handle_open_selected(
        &mut self,
        _: &OpenSelected,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_selected(cx);
    }

    fn handle_archive(&mut self, _: &Archive, _window: &mut Window, cx: &mut Context<Self>) {
        self.archive_selected(cx);
    }

    fn handle_toggle_star(&mut self, _: &ToggleStar, _window: &mut Window, cx: &mut Context<Self>) {
        self.toggle_star_selected(cx);
    }

    fn handle_toggle_read(&mut self, _: &ToggleRead, _window: &mut Window, cx: &mut Context<Self>) {
        self.toggle_read_selected(cx);
    }

    fn handle_trash(&mut self, _: &Trash, _window: &mut Window, cx: &mut Context<Self>) {
        self.trash_selected(cx);
    }

    /// Set the parent app entity for navigation
    pub fn set_app(&mut self, app: Entity<OrionApp>) {
        self.app = Some(app);
    }

    /// Update the store (called when persistent storage finishes loading)
    pub fn set_store(&mut self, store: Arc<dyn MailStore>) {
        self.store = store;
        self.is_store_loading = false;
    }

    /// Set the label filter and reload threads
    pub fn set_label_filter(&mut self, label: String, cx: &mut Context<Self>) {
        self.label_filter = Some(label);
        self.load_threads(cx);
        // Reset selection to first item when changing label
        self.selected_index = if self.threads.is_empty() {
            None
        } else {
            Some(0)
        };
        self.selected_thread = self.threads.first().map(|t| t.id.clone());
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
                debug!("Loaded {} threads", threads.len());

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

    fn render_skeleton(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();

        // Render skeleton thread items
        div()
            .flex()
            .flex_col()
            .flex_1()
            .bg(theme.list)
            .children((0..8).map(|_| {
                div()
                    .h(px(THREAD_ITEM_HEIGHT))
                    .w_full()
                    .px_4()
                    .py_2()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .border_b_1()
                    .border_color(theme.border)
                    // Skeleton for sender + subject line
                    .child(
                        div()
                            .flex()
                            .gap_3()
                            .child(Skeleton::new().w(px(120.)).h(px(16.)))
                            .child(Skeleton::new().flex_1().h(px(16.))),
                    )
                    // Skeleton for snippet line
                    .child(Skeleton::new().w(px(280.)).h(px(14.)))
            }))
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
        let selected_index = self.selected_index;

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
                    move |view, visible_range, _window, cx| {
                        visible_range
                            .map(|ix| {
                                let thread = view.threads[ix].clone();
                                // Use selected_index for keyboard selection
                                let is_selected = selected_index == Some(ix);
                                let thread_id = thread.id.clone();

                                div()
                                    .id(ElementId::Name(thread_id.0.clone().into()))
                                    .h(px(THREAD_ITEM_HEIGHT))
                                    .w_full()
                                    .cursor_pointer()
                                    .on_click(cx.listener(move |view, _event, _window, cx| {
                                        view.selected_index = Some(ix);
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
            .key_context("ThreadListView")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::handle_move_up))
            .on_action(cx.listener(Self::handle_move_down))
            .on_action(cx.listener(Self::handle_open_selected))
            .on_action(cx.listener(Self::handle_archive))
            .on_action(cx.listener(Self::handle_toggle_star))
            .on_action(cx.listener(Self::handle_toggle_read))
            .on_action(cx.listener(Self::handle_trash))
            .flex()
            .flex_col()
            .size_full()
            .bg(theme.background)
            .child(self.render_header(cx))
            .child(if self.is_store_loading || self.is_loading {
                self.render_skeleton(cx).into_any_element()
            } else if let Some(ref error) = self.error_message.clone() {
                self.render_error(error, cx).into_any_element()
            } else if self.threads.is_empty() {
                self.render_empty(cx).into_any_element()
            } else {
                self.render_thread_list(cx).into_any_element()
            })
    }
}
