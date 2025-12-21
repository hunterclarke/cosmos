//! Root application component for Orion mail app

use chrono::{DateTime, Local, Utc};
use gpui::prelude::*;
use gpui::*;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::ActiveTheme;
use log::{error, info, warn};
use mail::{
    sync_gmail, GmailAuth, GmailClient, Label, LabelId, MailStore, RedbMailStore, SyncOptions,
    ThreadId,
};
use std::sync::Arc;

use crate::components::Sidebar;
use crate::views::{ThreadListView, ThreadView};

/// Current view in the application
#[derive(Clone)]
pub enum View {
    Inbox,
    Thread,
}

/// Root application state
pub struct OrionApp {
    current_view: View,
    store: Arc<dyn MailStore>,
    gmail_client: Option<Arc<GmailClient>>,
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
}

impl OrionApp {
    pub fn new(cx: &mut Context<Self>) -> Self {
        // Create persistent storage in the config directory
        let store: Arc<dyn MailStore> = match Self::create_persistent_store() {
            Ok(store) => Arc::new(store),
            Err(e) => {
                warn!("Failed to create persistent storage: {}, using in-memory", e);
                Arc::new(mail::InMemoryMailStore::new())
            }
        };

        // Load last sync timestamp from sync state
        let last_sync_at = store
            .get_sync_state("default")
            .ok()
            .flatten()
            .map(|state| state.last_sync_at);

        // Create thread list view - we'll set the app handle after construction
        let store_clone = store.clone();
        let thread_list_view = cx.new(|_| ThreadListView::new(store_clone));

        Self {
            current_view: View::Inbox,
            store,
            gmail_client: None,
            is_syncing: false,
            sync_error: None,
            last_sync_at,
            thread_list_view: Some(thread_list_view),
            thread_view: None,
            labels: Sidebar::default_labels(),
            selected_label: LabelId::INBOX.to_string(),
        }
    }

    /// Create persistent storage in the config directory
    fn create_persistent_store() -> anyhow::Result<RedbMailStore> {
        // Ensure config directory exists
        config::init()?;

        // Get path for mail database
        let db_path = config::config_path("mail.redb")
            .ok_or_else(|| anyhow::anyhow!("Could not determine config directory"))?;

        RedbMailStore::new(&db_path)
    }

    /// Wire up navigation by setting app handle on child views
    pub fn wire_navigation(&mut self, app_handle: Entity<Self>, cx: &mut Context<Self>) {
        if let Some(thread_list) = &self.thread_list_view {
            thread_list.update(cx, |view, _| view.set_app(app_handle.clone()));
        }
    }

    /// Initialize Gmail client with credentials
    pub fn init_gmail(&mut self, client_id: String, client_secret: String) -> anyhow::Result<()> {
        let auth = GmailAuth::new(client_id, client_secret)?;
        let client = GmailClient::new(auth);
        self.gmail_client = Some(Arc::new(client));
        Ok(())
    }

    /// Navigate to thread list view
    pub fn show_inbox(&mut self, cx: &mut Context<Self>) {
        if self.thread_list_view.is_none() {
            let store = self.store.clone();
            self.thread_list_view = Some(cx.new(|_| ThreadListView::new(store)));
        }
        self.thread_view = None;
        self.current_view = View::Inbox;
        cx.notify();
    }

    /// Navigate to thread view
    pub fn show_thread(&mut self, thread_id: ThreadId, cx: &mut Context<Self>) {
        let app_handle = cx.entity().clone();
        let store = self.store.clone();
        self.thread_view = Some(cx.new(|cx| {
            let mut view = ThreadView::new(store, thread_id.clone());
            view.set_app(app_handle);
            view.load_thread(cx);
            view
        }));
        self.current_view = View::Thread;
        cx.notify();
    }

    /// Select a label/folder to view
    pub fn select_label(&mut self, label_id: String, cx: &mut Context<Self>) {
        self.selected_label = label_id.clone();
        self.current_view = View::Inbox;
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

        // Start periodic UI refresh while sync is running
        // This provides optimistic updates as messages are stored
        cx.spawn(async move |this, cx| {
            loop {
                // Wait 500ms between refreshes
                cx.background_executor().timer(std::time::Duration::from_millis(500)).await;

                // Check if still syncing
                let still_syncing = cx.update(|cx| {
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
                    }).unwrap_or(false)
                }).unwrap_or(false);

                if !still_syncing {
                    break;
                }
            }
        }).detach();

        // Run sync on background thread (it's blocking I/O)
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            // Execute sync on background thread pool
            let result = background
                .spawn(async move {
                    let options = SyncOptions {
                        max_messages: None, // Fetch all messages
                        full_resync: false,
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

    fn render_header(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let is_syncing = self.is_syncing;
        let theme = cx.theme();

        div()
            .w_full()
            .px_4()
            .pt_8() // Extra top padding for window controls
            .pb_2()
            .bg(theme.background)
            .border_b_1()
            .border_color(theme.border)
            .flex()
            .justify_between()
            .items_center()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .text_xl()
                            .font_weight(FontWeight::BOLD)
                            .text_color(theme.primary)
                            .child("Orion"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child("Mail"),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    // Show last sync timestamp
                    .when_some(self.last_sync_at, |el, ts| {
                        el.child(
                            div()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child(format_relative_time(ts)),
                        )
                    })
                    // Show error if any
                    .when_some(self.sync_error.clone(), |el, err| {
                        el.child(
                            div()
                                .text_xs()
                                .text_color(theme.danger)
                                .max_w(px(200.))
                                .text_ellipsis()
                                .child(err),
                        )
                    })
                    .child(
                        Button::new("sync-button")
                            .label(if is_syncing { "Syncing..." } else { "Sync" })
                            .primary()
                            .loading(is_syncing)
                            .on_click(cx.listener(|app, _event, _window, cx| {
                                app.sync(cx);
                            })),
                    ),
            )
    }

    fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let labels = self.labels.clone();
        let selected = self.selected_label.clone();

        div()
            .children(labels.into_iter().map(|label| {
                let label_id = label.id.0.clone();
                let is_selected = label_id == selected;

                div()
                    .id(ElementId::Name(format!("label-{}", label_id).into()))
                    .on_click(cx.listener(move |app, _event, _window, cx| {
                        app.select_label(label_id.clone(), cx);
                    }))
                    .child(crate::components::SidebarItem::new(label, is_selected))
            }))
    }

    fn render_content(&mut self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let theme = cx.theme();

        match &self.current_view {
            View::Inbox => {
                if let Some(thread_list) = &self.thread_list_view {
                    thread_list.clone().into_any_element()
                } else {
                    div()
                        .text_color(theme.muted_foreground)
                        .child("Loading...")
                        .into_any_element()
                }
            }
            View::Thread => {
                if let Some(thread) = &self.thread_view {
                    thread.clone().into_any_element()
                } else {
                    div()
                        .text_color(theme.muted_foreground)
                        .child("Loading thread...")
                        .into_any_element()
                }
            }
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

impl Render for OrionApp {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        // Clone theme colors upfront to avoid borrow conflicts
        let bg = theme.background;
        let fg = theme.foreground;
        let secondary_bg = theme.secondary;
        let border = theme.border;

        let header = self.render_header(cx);
        let sidebar = self.render_sidebar(cx);
        let content = self.render_content(cx);

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(bg)
            .text_color(fg)
            .child(header)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_1()
                    .overflow_hidden()
                    // Sidebar
                    .child(
                        div()
                            .w(px(220.))
                            .h_full()
                            .bg(secondary_bg)
                            .border_r_1()
                            .border_color(border)
                            .flex()
                            .flex_col()
                            .p_2()
                            .child(sidebar),
                    )
                    // Main content
                    .child(
                        div()
                            .flex()
                            .flex_1()
                            .overflow_hidden()
                            .child(content),
                    ),
            )
    }
}
