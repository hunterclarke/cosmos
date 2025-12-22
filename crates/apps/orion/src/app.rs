//! Root application component for Orion mail app

use chrono::{DateTime, Local, Utc};
use gpui::prelude::*;
use gpui::*;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::webview::WebView;
use gpui_component::{ActiveTheme, Sizable};
use log::{debug, error, info, warn};
use mail::{
    ActionHandler, GmailAuth, GmailClient, HeedMailStore, Label, LabelId, MailStore, SearchIndex,
    SyncOptions, ThreadId, sync_gmail,
};
use std::sync::Arc;

use crate::components::{SearchBox, SearchBoxEvent};
use wry::WebViewBuilder;

use crate::components::Sidebar;
use crate::templates;
use crate::views::{SearchResultsView, ThreadListView, ThreadView};

// Global actions for keyboard shortcuts
actions!(orion, [FocusSearch]);

/// Current view in the application
#[derive(Clone)]
pub enum View {
    Inbox,
    Thread {
        /// Pre-generated HTML for the thread (generated on navigation, not during render)
        html: String,
        /// Thread ID being viewed
        thread_id: ThreadId,
    },
    Search,
}

/// Root application state
pub struct OrionApp {
    current_view: View,
    store: Arc<dyn MailStore>,
    gmail_client: Option<Arc<GmailClient>>,
    /// Action handler for email operations (archive, star, read/unread)
    action_handler: Option<Arc<ActionHandler>>,
    is_syncing: bool,
    sync_error: Option<String>,
    /// Last successful sync timestamp
    last_sync_at: Option<DateTime<Utc>>,
    pub thread_list_view: Option<Entity<ThreadListView>>,
    thread_view: Option<Entity<ThreadView>>,
    /// Available mailbox labels/folders
    labels: Vec<Label>,
    /// Currently selected label (defaults to INBOX)
    selected_label: String,
    /// Shared WebView for HTML email rendering (created lazily)
    webview: Option<Entity<WebView>>,
    /// Currently loaded WebView content (to avoid reloading on every render)
    webview_loaded_html: Option<String>,
    /// Search index for full-text search
    search_index: Option<Arc<SearchIndex>>,
    /// Search box component
    search_box: Option<Entity<SearchBox>>,
    /// Search results view
    search_results_view: Option<Entity<SearchResultsView>>,
    /// Flag to focus search results on next render (after submit)
    pending_focus_results: bool,
}

impl OrionApp {
    pub fn new(cx: &mut Context<Self>) -> Self {
        use std::time::Instant;
        let new_start = Instant::now();

        // Start with in-memory store for instant startup
        let store: Arc<dyn MailStore> = Arc::new(mail::InMemoryMailStore::new());
        debug!("[BOOT]   InMemoryMailStore created: {:?}", new_start.elapsed());

        // Create thread list view with empty store (will be populated after DB loads)
        let store_clone = store.clone();
        let thread_list_view = cx.new(|_| ThreadListView::new(store_clone));
        debug!("[BOOT]   ThreadListView created: {:?}", new_start.elapsed());

        Self {
            current_view: View::Inbox,
            store,
            gmail_client: None,
            action_handler: None,
            is_syncing: false,
            sync_error: None,
            last_sync_at: None,
            thread_list_view: Some(thread_list_view),
            thread_view: None,
            labels: Sidebar::default_labels(),
            selected_label: LabelId::INBOX.to_string(),
            webview: None,
            webview_loaded_html: None,
            search_index: None,
            search_box: None,
            search_results_view: None,
            pending_focus_results: false,
        }
    }

    /// Load persistent storage in the background
    /// Call this after the UI is displayed for deferred loading
    pub fn load_persistent_storage(&mut self, cx: &mut Context<Self>) {
        let background = cx.background_executor().clone();

        cx.spawn(async move |this, cx| {
            // Load database and search index on background thread
            let result = background
                .spawn(async move {
                    use std::time::Instant;
                    let start = Instant::now();

                    let store: Arc<dyn MailStore> = match Self::create_persistent_store() {
                        Ok(store) => Arc::new(store),
                        Err(e) => {
                            warn!("Failed to create persistent storage: {}, using in-memory", e);
                            return Err(e);
                        }
                    };
                    debug!("[BOOT]   Database opened (background): {:?}", start.elapsed());

                    let last_sync_at = store
                        .get_sync_state("default")
                        .ok()
                        .flatten()
                        .map(|state| state.last_sync_at);

                    let search_index = match Self::create_search_index() {
                        Ok(index) => {
                            debug!("[BOOT]   SearchIndex opened (background): {:?}", start.elapsed());
                            Some(Arc::new(index))
                        }
                        Err(e) => {
                            warn!("Failed to create search index: {}", e);
                            None
                        }
                    };

                    Ok((store, last_sync_at, search_index))
                })
                .await;

            // Update app state on main thread
            if let Ok((store, last_sync_at, search_index)) = result {
                cx.update(|cx| {
                    this.update(cx, |app, cx| {
                        app.store = store.clone();
                        app.last_sync_at = last_sync_at;
                        app.search_index = search_index;

                        // Update action handler with the new store
                        if let Some(gmail_client) = &app.gmail_client {
                            app.action_handler = Some(Arc::new(ActionHandler::new(
                                gmail_client.clone(),
                                store.clone(),
                            )));
                        }

                        // Update thread list view with the real store
                        if let Some(thread_list) = &app.thread_list_view {
                            thread_list.update(cx, |view, cx| {
                                view.set_store(store);
                                view.load_threads(cx);
                            });
                        }

                        info!("Persistent storage loaded");
                        cx.notify();
                    })
                }).ok();
            }
        })
        .detach();
    }

    /// Create search index in the config directory
    fn create_search_index() -> anyhow::Result<SearchIndex> {
        // Ensure config directory exists
        config::init()?;

        // Get path for search index
        let index_path = config::config_path("mail.search.idx")
            .ok_or_else(|| anyhow::anyhow!("Could not determine config directory"))?;

        SearchIndex::open(&index_path)
    }

    /// Get or create the shared WebView
    fn get_or_create_webview(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<WebView> {
        if let Some(ref webview) = self.webview {
            return webview.clone();
        }

        // Get theme background color for WebView
        let theme = cx.theme();
        let bg = theme.background.to_rgb();
        let bg_r = (bg.r * 255.0) as u8;
        let bg_g = (bg.g * 255.0) as u8;
        let bg_b = (bg.b * 255.0) as u8;

        // Create initial HTML with dark background
        let initial_html = format!(
            "<html><head><style>html,body{{margin:0;padding:0;background:rgb({},{},{});}}</style></head><body></body></html>",
            bg_r, bg_g, bg_b
        );

        // Create a new WebView with dark background
        let wry_webview = WebViewBuilder::new()
            .with_html(&initial_html)
            .with_background_color((bg_r, bg_g, bg_b, 255))
            .build_as_child(window)
            .expect("Failed to create WebView");

        let webview_entity = cx.new(|cx| WebView::new(wry_webview, window, cx));
        self.webview = Some(webview_entity.clone());
        webview_entity
    }

    /// Hide the shared WebView
    pub fn hide_webview(&mut self, cx: &mut Context<Self>) {
        if let Some(ref webview) = self.webview {
            webview.update(cx, |wv, _| {
                wv.hide();
            });
        }
        // Clear loaded content so it will reload when shown again
        self.webview_loaded_html = None;
    }

    /// Create persistent storage in the config directory
    fn create_persistent_store() -> anyhow::Result<HeedMailStore> {
        // Ensure config directory exists
        config::init()?;

        // Get path for mail database (LMDB uses a directory, not a file)
        let db_path = config::config_path("mail.lmdb")
            .ok_or_else(|| anyhow::anyhow!("Could not determine config directory"))?;

        HeedMailStore::new(&db_path)
    }

    /// Wire up navigation by setting app handle on child views
    pub fn wire_navigation(&mut self, app_handle: Entity<Self>, cx: &mut Context<Self>) {
        if let Some(thread_list) = &self.thread_list_view {
            thread_list.update(cx, |view, _| view.set_app(app_handle.clone()));
        }
    }

    /// Get or create the search box
    fn get_or_create_search_box(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<SearchBox> {
        if let Some(ref search_box) = self.search_box {
            return search_box.clone();
        }

        let search_box = cx.new(|cx| SearchBox::new(window, cx));

        // Subscribe to search box events
        cx.subscribe(&search_box, Self::handle_search_box_event)
            .detach();

        self.search_box = Some(search_box.clone());
        search_box
    }

    /// Handle events from the search box
    fn handle_search_box_event(
        &mut self,
        _: Entity<SearchBox>,
        event: &SearchBoxEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            SearchBoxEvent::QueryChanged(query) => {
                self.update_search(query.clone(), cx);
            }
            SearchBoxEvent::Submitted(query) => {
                self.update_search(query.clone(), cx);
                // Set flag to focus results on next render (when we have window access)
                self.pending_focus_results = true;
            }
            SearchBoxEvent::Cleared => {
                self.clear_search(cx);
            }
            SearchBoxEvent::Cancelled => {
                self.clear_search(cx);
            }
        }
    }

    /// Update search results with a new query
    fn update_search(&mut self, query: String, cx: &mut Context<Self>) {
        if query.is_empty() {
            self.clear_search(cx);
            return;
        }

        // Create search results view if needed
        if self.search_results_view.is_none() {
            if let Some(ref index) = self.search_index {
                let store = self.store.clone();
                let index = index.clone();
                let app_handle = cx.entity().clone();
                self.search_results_view = Some(cx.new(|cx| {
                    let mut view = SearchResultsView::new(store, index, cx);
                    view.set_app(app_handle);
                    view
                }));
            }
        }

        // Execute search
        if let Some(ref results_view) = self.search_results_view {
            results_view.update(cx, |view, cx| {
                view.search(query.clone(), cx);
            });
        }

        // Hide WebView and switch to search view
        self.hide_webview(cx);
        self.thread_view = None;
        self.current_view = View::Search;
        cx.notify();
    }

    /// Clear search and return to inbox
    fn clear_search(&mut self, cx: &mut Context<Self>) {
        self.search_results_view = None;
        self.show_inbox(cx);
    }

    /// Focus the search box
    pub fn focus_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let search_box = self.get_or_create_search_box(window, cx);
        search_box.update(cx, |view, cx| {
            view.focus(window, cx);
        });
    }

    /// Get the current thread ID if viewing a thread
    pub fn current_thread_id(&self) -> Option<&ThreadId> {
        match &self.current_view {
            View::Thread { thread_id, .. } => Some(thread_id),
            _ => None,
        }
    }

    /// Archive the current thread
    pub fn archive_current_thread(&mut self, cx: &mut Context<Self>) {
        let Some(thread_id) = self.current_thread_id().cloned() else {
            return;
        };
        let Some(action_handler) = self.action_handler.clone() else {
            warn!("Cannot archive: action handler not available");
            return;
        };

        info!("Archiving thread {}", thread_id.as_str());

        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = background
                .spawn(async move { action_handler.archive_thread(&thread_id) })
                .await;

            cx.update(|cx| {
                this.update(cx, |app, cx| {
                    match result {
                        Ok(()) => {
                            info!("Thread archived successfully");
                            // Go back to inbox after archiving
                            app.show_inbox(cx);
                            // Refresh thread list
                            if let Some(thread_list) = &app.thread_list_view {
                                thread_list.update(cx, |view, cx| view.load_threads(cx));
                            }
                        }
                        Err(e) => {
                            error!("Failed to archive thread: {}", e);
                        }
                    }
                    cx.notify();
                })
            })
            .ok();
        })
        .detach();
    }

    /// Toggle star on the current thread
    pub fn toggle_star_current_thread(&mut self, cx: &mut Context<Self>) {
        let Some(thread_id) = self.current_thread_id().cloned() else {
            return;
        };
        let Some(action_handler) = self.action_handler.clone() else {
            warn!("Cannot toggle star: action handler not available");
            return;
        };

        info!("Toggling star for thread {}", thread_id.as_str());

        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = background
                .spawn(async move { action_handler.toggle_star(&thread_id) })
                .await;

            cx.update(|cx| {
                this.update(cx, |app, cx| {
                    match result {
                        Ok(new_starred) => {
                            info!(
                                "Thread {} {}",
                                if new_starred { "starred" } else { "unstarred" },
                                app.current_thread_id()
                                    .map(|id| id.as_str())
                                    .unwrap_or("unknown")
                            );
                            // Refresh thread list to show updated star state
                            if let Some(thread_list) = &app.thread_list_view {
                                thread_list.update(cx, |view, cx| view.load_threads(cx));
                            }
                        }
                        Err(e) => {
                            error!("Failed to toggle star: {}", e);
                        }
                    }
                    cx.notify();
                })
            })
            .ok();
        })
        .detach();
    }

    /// Toggle read status on the current thread
    pub fn toggle_read_current_thread(&mut self, cx: &mut Context<Self>) {
        let Some(thread_id) = self.current_thread_id().cloned() else {
            return;
        };
        let Some(action_handler) = self.action_handler.clone() else {
            warn!("Cannot toggle read: action handler not available");
            return;
        };

        info!("Toggling read status for thread {}", thread_id.as_str());

        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = background
                .spawn(async move { action_handler.toggle_read(&thread_id) })
                .await;

            cx.update(|cx| {
                this.update(cx, |app, cx| {
                    match result {
                        Ok(new_is_read) => {
                            info!(
                                "Thread marked as {}",
                                if new_is_read { "read" } else { "unread" }
                            );
                            // Refresh thread list to show updated read state
                            if let Some(thread_list) = &app.thread_list_view {
                                thread_list.update(cx, |view, cx| view.load_threads(cx));
                            }
                        }
                        Err(e) => {
                            error!("Failed to toggle read status: {}", e);
                        }
                    }
                    cx.notify();
                })
            })
            .ok();
        })
        .detach();
    }

    /// Trash the current thread
    pub fn trash_current_thread(&mut self, cx: &mut Context<Self>) {
        let Some(thread_id) = self.current_thread_id().cloned() else {
            return;
        };
        let Some(action_handler) = self.action_handler.clone() else {
            warn!("Cannot trash: action handler not available");
            return;
        };

        info!("Trashing thread {}", thread_id.as_str());

        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = background
                .spawn(async move { action_handler.trash_thread(&thread_id) })
                .await;

            cx.update(|cx| {
                this.update(cx, |app, cx| {
                    match result {
                        Ok(()) => {
                            info!("Thread trashed successfully");
                            // Go back to inbox after trashing
                            app.show_inbox(cx);
                            // Refresh thread list
                            if let Some(thread_list) = &app.thread_list_view {
                                thread_list.update(cx, |view, cx| view.load_threads(cx));
                            }
                        }
                        Err(e) => {
                            error!("Failed to trash thread: {}", e);
                        }
                    }
                    cx.notify();
                })
            })
            .ok();
        })
        .detach();
    }

    /// Initialize Gmail client with credentials
    pub fn init_gmail(&mut self, client_id: String, client_secret: String) -> anyhow::Result<()> {
        let auth = GmailAuth::new(client_id, client_secret)?;
        let client = GmailClient::new(auth);
        let gmail_client = Arc::new(client);
        self.gmail_client = Some(gmail_client.clone());

        // Create action handler now that we have the Gmail client
        self.action_handler = Some(Arc::new(ActionHandler::new(
            gmail_client,
            self.store.clone(),
        )));

        Ok(())
    }

    /// Navigate to thread list view
    pub fn show_inbox(&mut self, cx: &mut Context<Self>) {
        if self.thread_list_view.is_none() {
            let store = self.store.clone();
            self.thread_list_view = Some(cx.new(|_| ThreadListView::new(store)));
        }
        // Hide the WebView when not viewing a thread
        self.hide_webview(cx);
        // Clean up thread view
        self.thread_view = None;
        self.current_view = View::Inbox;
        cx.notify();
    }

    /// Navigate to thread view
    pub fn show_thread(&mut self, thread_id: ThreadId, cx: &mut Context<Self>) {
        // Load thread data and generate HTML upfront (not during render)
        let store = self.store.clone();
        let theme = cx.theme();
        let thread_html = match mail::get_thread_detail(store.as_ref(), &thread_id) {
            Ok(Some(detail)) => {
                info!(
                    "Thread {} has {} messages",
                    thread_id.as_str(),
                    detail.messages.len()
                );
                let html = templates::thread_html(&detail.messages, &theme);
                info!("Generated HTML with {} bytes", html.len());
                html
            }
            Ok(None) => {
                warn!("Thread {} not found", thread_id.as_str());
                templates::error_html("Thread not found", &theme)
            }
            Err(e) => {
                error!("Failed to load thread {}: {}", thread_id.as_str(), e);
                templates::error_html(&format!("Failed to load thread: {}", e), &theme)
            }
        };

        let app_handle = cx.entity().clone();
        let thread_id_clone = thread_id.clone();
        self.thread_view = Some(cx.new(|cx| {
            let mut view = ThreadView::new(store, thread_id.clone());
            view.set_app(app_handle);
            view.load_thread(cx);
            view
        }));
        self.current_view = View::Thread {
            html: thread_html.clone(),
            thread_id: thread_id_clone.clone(),
        };
        cx.notify();

        // Mark thread as read in background
        if let Some(action_handler) = self.action_handler.clone() {
            let background = cx.background_executor().clone();
            cx.spawn(async move |this, cx| {
                let result = background
                    .spawn(async move { action_handler.set_read(&thread_id_clone, true) })
                    .await;

                if let Err(e) = result {
                    error!("Failed to mark thread as read: {}", e);
                }

                // Refresh thread list to show updated read state
                cx.update(|cx| {
                    this.update(cx, |app, cx| {
                        if let Some(thread_list) = &app.thread_list_view {
                            thread_list.update(cx, |view, cx| view.load_threads(cx));
                        }
                        cx.notify();
                    })
                })
                .ok();
            })
            .detach();
        }
    }

    /// Select a label/folder to view
    pub fn select_label(&mut self, label_id: String, cx: &mut Context<Self>) {
        self.selected_label = label_id.clone();
        self.current_view = View::Inbox;

        // Hide WebView and clean up thread view
        self.hide_webview(cx);
        self.thread_view = None;

        // Update thread list view with the new label filter
        if let Some(thread_list) = &self.thread_list_view {
            thread_list.update(cx, |view, cx| view.set_label_filter(label_id, cx));
        }
        cx.notify();
    }

    /// Trigger inbox sync
    pub fn sync(&mut self, cx: &mut Context<Self>) {
        if self.is_syncing {
            return;
        }

        let Some(client) = self.gmail_client.clone() else {
            self.sync_error = Some("Gmail client not configured".to_string());
            cx.notify();
            return;
        };

        self.is_syncing = true;
        self.sync_error = None;
        cx.notify();

        let store = self.store.clone();
        let search_index = self.search_index.clone();

        // Start periodic UI refresh while sync is running
        // This provides optimistic updates as messages are stored
        cx.spawn(async move |this, cx| {
            loop {
                // Wait 500ms between refreshes
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(500))
                    .await;

                // Check if still syncing
                let still_syncing = cx
                    .update(|cx| {
                        this.update(cx, |app, cx| {
                            if app.is_syncing {
                                // Refresh thread list with new data
                                if let Some(thread_list) = &app.thread_list_view {
                                    thread_list.update(cx, |view, cx| view.load_threads(cx));
                                }
                                cx.notify();
                                true
                            } else {
                                false
                            }
                        })
                        .unwrap_or(false)
                    })
                    .unwrap_or(false);

                if !still_syncing {
                    break;
                }
            }
        })
        .detach();

        // Run sync on background thread (it's blocking I/O)
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            // Execute sync on background thread pool
            let result = background
                .spawn(async move {
                    let options = SyncOptions {
                        max_messages: None,
                        full_resync: false,
                        search_index,
                        ..Default::default()
                    };
                    sync_gmail(&client, store.as_ref(), "default", options)
                })
                .await;

            cx.update(|cx| {
                this.update(cx, |app, cx| {
                    app.is_syncing = false;
                    match result {
                        Ok(stats) => {
                            let sync_type = if stats.was_incremental {
                                "incremental"
                            } else {
                                "initial"
                            };
                            info!(
                                "Sync complete ({}): {} fetched, {} created, {} skipped in {}ms",
                                sync_type,
                                stats.messages_fetched,
                                stats.messages_created,
                                stats.messages_skipped,
                                stats.duration_ms
                            );
                            // Update last sync timestamp
                            app.last_sync_at = Some(Utc::now());
                            // Final reload of thread list
                            if let Some(thread_list) = &app.thread_list_view {
                                thread_list.update(cx, |view, cx| view.load_threads(cx));
                            }
                        }
                        Err(e) => {
                            error!("Sync failed: {}", e);
                            app.sync_error = Some(format!("{}", e));
                        }
                    }
                    cx.notify();
                })
            })
        })
        .detach();
    }

    fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let theme = cx.theme();
        let labels = self.labels.clone();
        let selected = self.selected_label.clone();
        let is_syncing = self.is_syncing;
        let last_sync = self.last_sync_at;

        div()
            .flex()
            .flex_col()
            .h_full()
            // Sidebar header with app branding
            .child(
                div()
                    .pt_8() // Extra top padding for window controls
                    .pb_4()
                    .px_3()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .text_lg()
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(theme.foreground)
                                    .child("Orion"),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child("Mail"),
                            ),
                    ),
            )
            // Navigation labels - fills remaining space
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .overflow_y_hidden()
                    .px_2()
                    .py_1()
                    .children(labels.into_iter().map(|label| {
                        let label_id = label.id.0.clone();
                        let is_selected = label_id == selected;

                        div()
                            .id(ElementId::Name(format!("label-{}", label_id).into()))
                            .on_click(cx.listener(move |app, _event, _window, cx| {
                                app.select_label(label_id.clone(), cx);
                            }))
                            .child(crate::components::SidebarItem::new(label, is_selected))
                    })),
            )
            // Sidebar footer with sync and profile
            .child(
                div()
                    .flex()
                    .flex_col()
                    .border_t_1()
                    .border_color(theme.border)
                    // Sync row
                    .child(
                        div()
                            .px_3()
                            .py_2()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div().text_xs().text_color(theme.muted_foreground).child(
                                    last_sync
                                        .map(|ts| format_relative_time(ts))
                                        .unwrap_or_else(|| "Not synced".to_string()),
                                ),
                            )
                            .child(
                                Button::new("sync-button")
                                    .icon(gpui_component::Icon::new(
                                        crate::assets::icons::RefreshCw,
                                    ))
                                    .label(if is_syncing { "Syncing..." } else { "Sync" })
                                    .small()
                                    .ghost()
                                    .loading(is_syncing)
                                    .cursor_pointer()
                                    .on_click(cx.listener(|app, _event, _window, cx| {
                                        app.sync(cx);
                                    })),
                            ),
                    )
                    // Profile row
                    .child(
                        div()
                            .px_3()
                            .py_2()
                            .border_t_1()
                            .border_color(theme.border)
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .size_8()
                                    .rounded_full()
                                    .bg(theme.primary)
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_xs()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(theme.primary_foreground)
                                    .child("U"),
                            )
                            .child(
                                div().flex().flex_col().overflow_hidden().flex_1().child(
                                    div()
                                        .text_sm()
                                        .text_color(theme.foreground)
                                        .text_ellipsis()
                                        .child("user@gmail.com"),
                                ),
                            ),
                    ),
            )
    }

    fn render_content(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        // Extract theme colors upfront before any mutable borrows
        let theme = cx.theme();
        let bg = theme.background;
        let muted_fg = theme.muted_foreground;

        // Extract data from current_view before any mutable borrows
        let (html_content, thread_entity, is_search) = match &self.current_view {
            View::Inbox => (None, None, false),
            View::Thread { html, .. } => (Some(html.clone()), self.thread_view.clone(), false),
            View::Search => (None, None, true),
        };

        // Search results view
        if is_search {
            if let Some(search_results) = &self.search_results_view {
                return search_results.clone().into_any_element();
            } else {
                return div()
                    .text_color(muted_fg)
                    .child("Search not available")
                    .into_any_element();
            }
        }

        // Inbox view
        if html_content.is_none() {
            if let Some(thread_list) = &self.thread_list_view {
                return thread_list.clone().into_any_element();
            } else {
                return div()
                    .text_color(muted_fg)
                    .child("Loading...")
                    .into_any_element();
            }
        }

        // Thread view - always use WebView
        let html = html_content.unwrap();
        if let Some(thread) = thread_entity {
            let webview = self.get_or_create_webview(window, cx);

            // Only reload HTML if content has changed (avoids re-render on scroll)
            let needs_reload = self
                .webview_loaded_html
                .as_ref()
                .map(|loaded| loaded != &html)
                .unwrap_or(true);

            if needs_reload {
                info!("Loading HTML into WebView ({} bytes)", html.len());
                webview.update(cx, |wv, _| {
                    let _ = wv.load_html(&html);
                    wv.show();
                });
                self.webview_loaded_html = Some(html.clone());
            }

            // Render thread header + WebView container
            div()
                .flex()
                .flex_col()
                .size_full()
                .bg(bg)
                .child(thread) // ThreadView renders header only
                .child(
                    div()
                        .id("webview-container")
                        .flex_1()
                        .w_full()
                        .min_h_0()
                        .p_4() // Match native card padding
                        .child(webview),
                )
                .into_any_element()
        } else {
            div()
                .text_color(muted_fg)
                .child("Loading thread...")
                .into_any_element()
        }
    }
}

/// Format a timestamp as a relative time string (e.g., "5 minutes ago")
fn format_relative_time(ts: DateTime<Utc>) -> String {
    let now = Utc::now();
    let duration = now.signed_duration_since(ts);

    if duration.num_seconds() < 60 {
        "Just now".to_string()
    } else if duration.num_minutes() < 60 {
        let mins = duration.num_minutes();
        if mins == 1 {
            "1 minute ago".to_string()
        } else {
            format!("{} minutes ago", mins)
        }
    } else if duration.num_hours() < 24 {
        let hours = duration.num_hours();
        if hours == 1 {
            "1 hour ago".to_string()
        } else {
            format!("{} hours ago", hours)
        }
    } else {
        // Show as local date/time
        let local: DateTime<Local> = ts.into();
        local.format("%b %d at %H:%M").to_string()
    }
}

impl OrionApp {
    fn handle_focus_search(&mut self, _: &FocusSearch, window: &mut Window, cx: &mut Context<Self>) {
        self.focus_search(window, cx);
    }
}

impl Render for OrionApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Handle pending focus on search results (from Enter key in search box)
        if self.pending_focus_results {
            self.pending_focus_results = false;
            if let Some(ref results_view) = self.search_results_view {
                results_view.update(cx, |view, cx| {
                    view.focus(window, cx);
                });
            }
        }

        let theme = cx.theme();
        // Clone theme colors upfront to avoid borrow conflicts
        let bg = theme.background;
        let fg = theme.foreground;
        let secondary_bg = theme.secondary;
        let border = theme.border;

        let sidebar = self.render_sidebar(cx);
        let search_box = self.get_or_create_search_box(window, cx);
        let content = self.render_content(window, cx);

        div()
            .key_context("OrionApp")
            .on_action(cx.listener(Self::handle_focus_search))
            .flex()
            .flex_row()
            .size_full()
            .bg(bg)
            .text_color(fg)
            // Sidebar
            .child(
                div()
                    .w(px(240.))
                    .h_full()
                    .bg(secondary_bg)
                    .border_r_1()
                    .border_color(border)
                    .child(sidebar),
            )
            // Main content area with header
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .overflow_hidden()
                    // Header with search box (right-aligned)
                    .child(
                        div()
                            .w_full()
                            .px_4()
                            .py_2()
                            .border_b_1()
                            .border_color(border)
                            .flex()
                            .justify_end()
                            .items_center()
                            .child(search_box),
                    )
                    // Content area
                    .child(div().flex().flex_1().overflow_hidden().child(content)),
            )
    }
}
