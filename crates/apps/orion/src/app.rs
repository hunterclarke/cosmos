//! Root application component for Orion mail app

use chrono::{DateTime, Local, Utc};
use gpui::prelude::*;
use gpui::*;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::webview::WebView;
use gpui_component::{ActiveTheme, Icon, IconName, Sizable, Size as ComponentSize};
use log::{debug, error, info, warn};
use mail::{
    Account, ActionHandler, FileBlobStore, GmailAuth, GmailClient, Label, LabelId, MailStore,
    SearchIndex, SqliteMailStore, SyncOptions, SyncState, SyncStats, ThreadId,
};
use std::collections::HashMap;
use std::sync::Arc;

use crate::components::{AccountItem, AllAccountsItem, SearchBox, SearchBoxEvent, ShortcutsHelp};
use crate::input::{
    Dismiss, GoToAllMail, GoToDrafts, GoToInbox, GoToSent, GoToStarred, GoToTrash, ShowShortcuts,
};
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

/// What view should receive focus on next render
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PendingFocus {
    ThreadList,
    ThreadView,
}

/// The list context from which a thread was opened.
/// Used to determine where Dismiss should return to.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ListContext {
    /// Thread was opened from the inbox/label thread list
    Inbox,
    /// Thread was opened from search results
    Search,
}

/// State for a single Gmail account
pub struct AccountState {
    /// The account record from the database
    pub account: Account,
    /// Gmail API client for this account
    pub gmail_client: Arc<GmailClient>,
    /// Action handler for email operations (used for per-account actions)
    #[allow(dead_code)]
    pub action_handler: Arc<ActionHandler>,
    /// Whether this account is currently syncing
    pub is_syncing: bool,
    /// Last successful sync timestamp
    pub last_sync_at: Option<DateTime<Utc>>,
    /// Last sync error message
    pub sync_error: Option<String>,
}

/// Root application state
pub struct OrionApp {
    current_view: View,
    store: Arc<dyn MailStore>,

    // === Multi-Account State ===
    /// Per-account state (Gmail client, action handler, sync status)
    accounts: HashMap<i64, AccountState>,
    /// Currently selected account for filtering (None = unified view, all accounts)
    selected_account: Option<i64>,
    /// Primary account ID (first registered, used for fallback)
    primary_account_id: Option<i64>,

    // === Primary Account Shortcuts ===
    // These fields mirror the primary account's state for convenient access.
    // They are automatically updated when loading/adding accounts.
    // Used by: sync(), action handlers (archive, star, read, trash), sidebar display.
    //
    // These are NOT legacy fields - they provide quick access to the primary account
    // without HashMap lookups, which is useful for the common single-account case.
    gmail_client: Option<Arc<GmailClient>>,
    action_handler: Option<Arc<ActionHandler>>,
    /// App-level syncing flag (true if legacy sync() is running)
    is_syncing: bool,
    sync_error: Option<String>,
    last_sync_at: Option<DateTime<Utc>>,
    profile_email: Option<String>,

    // === UI State ===
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
    /// What view should receive focus on next render
    pending_focus: Option<PendingFocus>,
    /// Whether to show keyboard shortcuts help overlay
    show_shortcuts_help: bool,
    /// Pending G-sequence (waiting for second key)
    pending_g_sequence: bool,
    /// The list context from which the current thread was opened
    thread_list_context: ListContext,

    // === Sync Configuration ===
    /// Minimum seconds between syncs (cooldown)
    sync_cooldown_secs: u64,
    /// Background polling interval in seconds
    poll_interval_secs: u64,
    /// Background polling task handle
    poll_task: Option<Task<()>>,
    /// Track window active state for foreground detection
    was_window_active: bool,

    // === OAuth Credentials ===
    /// OAuth client ID (stored for account discovery after storage loads)
    oauth_client_id: Option<String>,
    /// OAuth client secret
    oauth_client_secret: Option<String>,
}

impl OrionApp {
    pub fn new(cx: &mut Context<Self>) -> Self {
        use std::time::Instant;
        let new_start = Instant::now();

        // Start with in-memory store for instant startup
        let store: Arc<dyn MailStore> = Arc::new(mail::InMemoryMailStore::new());
        debug!(
            "[BOOT]   InMemoryMailStore created: {:?}",
            new_start.elapsed()
        );

        // Create thread list view with empty store (will be populated after DB loads)
        let store_clone = store.clone();
        let thread_list_view = cx.new(|cx| ThreadListView::new(store_clone, cx));
        debug!("[BOOT]   ThreadListView created: {:?}", new_start.elapsed());

        Self {
            current_view: View::Inbox,
            store,

            // Multi-account state
            accounts: HashMap::new(),
            selected_account: None, // Unified view by default
            primary_account_id: None,

            // Primary account shortcuts (set by load_accounts/add_account)
            gmail_client: None,
            action_handler: None,
            is_syncing: false,
            sync_error: None,
            last_sync_at: None,
            profile_email: None,

            // UI state
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
            pending_focus: Some(PendingFocus::ThreadList), // Focus thread list on launch
            show_shortcuts_help: false,
            pending_g_sequence: false,
            thread_list_context: ListContext::Inbox,

            // Sync config
            sync_cooldown_secs: 30,
            poll_interval_secs: 60,
            poll_task: None,
            was_window_active: true,

            // OAuth credentials (set later via set_credentials)
            oauth_client_id: None,
            oauth_client_secret: None,
        }
    }

    /// Store OAuth credentials for later account discovery
    pub fn set_credentials(&mut self, client_id: String, client_secret: String) {
        self.oauth_client_id = Some(client_id);
        self.oauth_client_secret = Some(client_secret);
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
                            warn!(
                                "Failed to create persistent storage: {}, using in-memory",
                                e
                            );
                            return Err(e);
                        }
                    };
                    debug!(
                        "[BOOT]   Database opened (background): {:?}",
                        start.elapsed()
                    );

                    // During initial load, we don't know which account is primary yet.
                    // Account ID 1 is used because:
                    // 1. For new databases: first registered account gets ID 1
                    // 2. For existing databases: the original single account has ID 1
                    // After load_accounts() runs, we'll use the actual primary account.
                    let account_id: i64 = 1;
                    let sync_state = store.get_sync_state(account_id).ok().flatten();
                    let sync_info = mail::get_sync_state_info(sync_state.as_ref());
                    let last_sync_at = sync_info.last_sync_at;
                    let should_auto_sync = mail::should_auto_sync_on_startup(sync_state.as_ref());

                    let search_index = match Self::create_search_index() {
                        Ok(index) => {
                            debug!(
                                "[BOOT]   SearchIndex opened (background): {:?}",
                                start.elapsed()
                            );
                            Some(Arc::new(index))
                        }
                        Err(e) => {
                            warn!("Failed to create search index: {}", e);
                            None
                        }
                    };

                    Ok((store, last_sync_at, should_auto_sync, sync_info, search_index))
                })
                .await;

            // Update app state on main thread
            if let Ok((store, last_sync_at, should_auto_sync, sync_info, search_index)) = result {
                cx.update(|cx| {
                    this.update(cx, |app, cx| {
                        app.store = store.clone();
                        app.last_sync_at = last_sync_at;
                        app.search_index = search_index;

                        // Load accounts from database
                        if let (Some(client_id), Some(client_secret)) =
                            (app.oauth_client_id.clone(), app.oauth_client_secret.clone())
                        {
                            app.load_accounts(client_id, client_secret, cx);
                        }

                        // Update thread list view with the real store
                        if let Some(thread_list) = &app.thread_list_view {
                            thread_list.update(cx, |view, cx| {
                                view.set_store(store);
                                view.load_threads(cx);
                            });
                        }

                        // Update inbox unread count
                        app.refresh_inbox_unread_count();

                        info!("Persistent storage loaded");

                        // Auto-start sync if Gmail is configured (either via accounts or legacy)
                        let has_gmail = !app.accounts.is_empty() || app.gmail_client.is_some();
                        if has_gmail && should_auto_sync {
                            if sync_info.needs_resume {
                                if let Some(ref progress) = sync_info.resume_progress {
                                    info!(
                                        "Resuming incomplete sync (page_token={}, messages_listed={}, failed={})",
                                        progress.has_page_token,
                                        progress.messages_listed,
                                        progress.failed_message_count
                                    );
                                } else {
                                    info!("Resuming incomplete initial sync...");
                                }
                            } else {
                                info!("No previous sync found, starting initial sync...");
                            }
                            app.sync(cx);
                        }

                        // Start background polling if Gmail is configured
                        if has_gmail {
                            app.start_polling(cx);
                        }

                        cx.notify();
                    })
                })
                .ok();
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
    fn create_persistent_store() -> anyhow::Result<SqliteMailStore> {
        // Ensure config directory exists
        config::init()?;

        // Get paths for database and blob storage
        let db_path = config::config_path("mail.db")
            .ok_or_else(|| anyhow::anyhow!("Could not determine config directory"))?;
        let blob_path = config::config_path("blobs")
            .ok_or_else(|| anyhow::anyhow!("Could not determine config directory"))?;

        // Create blob store for message bodies
        let blob_store = Box::new(FileBlobStore::new(&blob_path)?);

        SqliteMailStore::new(&db_path, blob_store)
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

    /// Archive the current thread (navigates back to inbox after)
    pub fn archive_current_thread(&mut self, cx: &mut Context<Self>) {
        let Some(thread_id) = self.current_thread_id().cloned() else {
            return;
        };
        self.archive_thread(thread_id, true, cx);
    }

    /// Archive a specific thread
    /// If `navigate_to_inbox` is true, navigates back to inbox after archiving
    pub fn archive_thread(
        &mut self,
        thread_id: ThreadId,
        navigate_to_inbox: bool,
        cx: &mut Context<Self>,
    ) {
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
                            // Only navigate to inbox if requested (e.g., from thread view)
                            if navigate_to_inbox {
                                app.show_inbox(cx);
                            }
                            // Refresh thread list
                            if let Some(thread_list) = &app.thread_list_view {
                                thread_list.update(cx, |view, cx| view.load_threads(cx));
                            }
                            // Update inbox unread count
                            app.refresh_inbox_unread_count();
                            // Trigger sync to pick up any new messages
                            app.try_sync(cx);
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
        self.toggle_star_thread(thread_id, cx);
    }

    /// Toggle star on a specific thread
    pub fn toggle_star_thread(&mut self, thread_id: ThreadId, cx: &mut Context<Self>) {
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
                                "Thread {}",
                                if new_starred { "starred" } else { "unstarred" }
                            );
                            // Refresh thread list to show updated star state
                            if let Some(thread_list) = &app.thread_list_view {
                                thread_list.update(cx, |view, cx| view.load_threads(cx));
                            }
                            // Trigger sync to pick up any new messages
                            app.try_sync(cx);
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
        self.toggle_read_thread(thread_id, cx);
    }

    /// Toggle read status on a specific thread
    pub fn toggle_read_thread(&mut self, thread_id: ThreadId, cx: &mut Context<Self>) {
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
                            // Update inbox unread count
                            app.refresh_inbox_unread_count();
                            // Trigger sync to pick up any new messages
                            app.try_sync(cx);
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

    /// Trash the current thread (navigates back to inbox after)
    pub fn trash_current_thread(&mut self, cx: &mut Context<Self>) {
        let Some(thread_id) = self.current_thread_id().cloned() else {
            return;
        };
        self.trash_thread(thread_id, true, cx);
    }

    /// Trash a specific thread
    /// If `navigate_to_inbox` is true, navigates back to inbox after trashing
    pub fn trash_thread(
        &mut self,
        thread_id: ThreadId,
        navigate_to_inbox: bool,
        cx: &mut Context<Self>,
    ) {
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
                            // Only navigate to inbox if requested (e.g., from thread view)
                            if navigate_to_inbox {
                                app.show_inbox(cx);
                            }
                            // Refresh thread list
                            if let Some(thread_list) = &app.thread_list_view {
                                thread_list.update(cx, |view, cx| view.load_threads(cx));
                            }
                            // Update inbox unread count
                            app.refresh_inbox_unread_count();
                            // Trigger sync to pick up any new messages
                            app.try_sync(cx);
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

    /// Add a new Gmail account via OAuth flow
    ///
    /// This will:
    /// 1. Open browser for OAuth authentication (runs on background thread)
    /// 2. Get the user's email from Gmail API
    /// 3. Save account with token to database
    /// 4. Create AccountState
    pub fn add_account(&mut self, cx: &mut Context<Self>) {
        let (Some(client_id), Some(client_secret)) =
            (self.oauth_client_id.clone(), self.oauth_client_secret.clone())
        else {
            error!("No OAuth credentials configured");
            return;
        };

        let store = self.store.clone();
        let current_account_count = self.accounts.len();
        let background = cx.background_executor().clone();

        info!("Starting OAuth flow for new account...");

        cx.spawn(async move |this, cx| {
            // Run OAuth flow on background thread to avoid blocking UI
            let oauth_result = background
                .spawn(async move {
                    // Create auth with no existing token (will trigger OAuth flow)
                    let auth = GmailAuth::with_token_data(
                        client_id.clone(),
                        client_secret.clone(),
                        None,
                    );
                    let client = GmailClient::new(auth);

                    // Get access token (triggers OAuth flow in browser)
                    let profile = client.get_profile()?;
                    let email = profile.email_address;
                    let token_data = client.get_token_data();

                    // Check if account already exists
                    if let Ok(Some(_)) = store.get_account_by_email(&email) {
                        return Err(anyhow::anyhow!("Account {} already exists", email));
                    }

                    // Create and save account
                    let is_primary = current_account_count == 0;
                    let new_account = Account {
                        id: 0,
                        email: email.clone(),
                        display_name: None,
                        avatar_color: Self::generate_avatar_color(current_account_count),
                        is_primary,
                        added_at: chrono::Utc::now(),
                        token_data,
                    };

                    let account = store.register_account(new_account)?;
                    info!("Saved account: {} (id={})", email, account.id);

                    Ok((account, client_id, client_secret))
                })
                .await;

            // Update app state on main thread
            match oauth_result {
                Ok((account, client_id, client_secret)) => {
                    cx.update(|cx| {
                        this.update(cx, |app, cx| {
                            // Create AccountState
                            let auth = GmailAuth::with_token_data(
                                client_id,
                                client_secret,
                                account.token_data.clone(),
                            );
                            let gmail_client = Arc::new(GmailClient::new(auth));
                            let action_handler = Arc::new(ActionHandler::new(
                                gmail_client.clone(),
                                app.store.clone(),
                            ));

                            let account_state = AccountState {
                                account: account.clone(),
                                gmail_client: gmail_client.clone(),
                                action_handler: action_handler.clone(),
                                is_syncing: false,
                                last_sync_at: None,
                                sync_error: None,
                            };

                            if account.is_primary {
                                app.primary_account_id = Some(account.id);
                                app.gmail_client = Some(gmail_client);
                                app.action_handler = Some(action_handler);
                                app.profile_email = Some(account.email.clone());
                            }

                            let account_id = account.id;
                            app.accounts.insert(account_id, account_state);
                            cx.notify();

                            info!("Account {} added successfully", account.email);

                            // Start initial sync for the new account
                            app.sync_account(account_id, cx);
                        })
                    })
                    .ok();
                }
                Err(e) => {
                    error!("Failed to add account: {}", e);
                }
            }
        })
        .detach();
    }

    /// Load and initialize all accounts from database
    ///
    /// Loads accounts from SQLite, uses token_data field for auth,
    /// and creates AccountState for each.
    pub fn load_accounts(&mut self, client_id: String, client_secret: String, cx: &mut Context<Self>) {
        let accounts = match self.store.list_accounts() {
            Ok(accounts) => accounts,
            Err(e) => {
                warn!("Failed to load accounts: {}", e);
                return;
            }
        };

        if accounts.is_empty() {
            info!("No accounts in database, starting add account flow...");
            self.add_account(cx);
            return;
        }

        info!("Loading {} account(s) from database", accounts.len());

        for account in accounts {
            // Create GmailAuth with token_data from database
            let auth = GmailAuth::with_token_data(
                client_id.clone(),
                client_secret.clone(),
                account.token_data.clone(),
            );

            // Create Gmail client and action handler
            let gmail_client = Arc::new(GmailClient::new(auth));
            let action_handler = Arc::new(ActionHandler::new(
                gmail_client.clone(),
                self.store.clone(),
            ));

            // Create AccountState
            let account_state = AccountState {
                account: account.clone(),
                gmail_client: gmail_client.clone(),
                action_handler: action_handler.clone(),
                is_syncing: false,
                last_sync_at: None,
                sync_error: None,
            };

            // Set primary account fields
            if account.is_primary {
                self.primary_account_id = Some(account.id);
                self.gmail_client = Some(gmail_client);
                self.action_handler = Some(action_handler);
                self.profile_email = Some(account.email.clone());
            }

            self.accounts.insert(account.id, account_state);
            info!(
                "Loaded account: {} (id={}, primary={}, has_token={})",
                account.email,
                account.id,
                account.is_primary,
                account.token_data.is_some()
            );
        }

        cx.notify();
    }

    /// Generate a consistent avatar color based on account index
    fn generate_avatar_color(index: usize) -> String {
        // Use a set of pleasant, distinguishable colors
        let colors = [
            "hsl(210, 70%, 50%)", // Blue
            "hsl(150, 70%, 40%)", // Green
            "hsl(340, 70%, 50%)", // Pink
            "hsl(45, 80%, 50%)",  // Orange
            "hsl(270, 60%, 55%)", // Purple
            "hsl(180, 60%, 45%)", // Teal
        ];
        colors[index % colors.len()].to_string()
    }

    // === Multi-Account Management Methods ===

    /// Get the account ID to use for operations
    ///
    /// If an account is selected (filtered view), returns that account.
    /// Otherwise falls back to primary account.
    /// Returns None if no accounts are registered.
    pub fn current_account_id(&self) -> Option<i64> {
        self.selected_account.or(self.primary_account_id)
    }

    /// Get the account ID to use for operations, with a fallback for legacy code.
    ///
    /// This is used by sync operations that need an account ID but were written
    /// before multi-account support. It defaults to 1 for backwards compatibility
    /// with existing databases that have account_id=1.
    ///
    /// New code should use `current_account_id()` and handle the None case.
    fn current_account_id_or_default(&self) -> i64 {
        self.current_account_id().unwrap_or(1)
    }

    /// Set the account filter for views
    ///
    /// Pass `None` for unified view (all accounts), or `Some(id)` for single account.
    pub fn set_account_filter(&mut self, account_id: Option<i64>, cx: &mut Context<Self>) {
        self.selected_account = account_id;

        // Update thread list view with the new account filter
        if let Some(thread_list) = &self.thread_list_view {
            thread_list.update(cx, |view, cx| {
                view.set_account_filter(account_id, cx);
            });
        }

        cx.notify();
    }

    /// Check if we're in unified view (all accounts)
    #[allow(dead_code)]
    pub fn is_unified_view(&self) -> bool {
        self.selected_account.is_none()
    }

    /// Get all registered accounts
    #[allow(dead_code)]
    pub fn list_accounts(&self) -> Vec<&AccountState> {
        self.accounts.values().collect()
    }

    /// Sync all accounts (or just the selected account if filtered)
    ///
    /// This is called by the sync button in the sidebar.
    pub fn sync_all_accounts(&mut self, cx: &mut Context<Self>) {
        // If a specific account is selected, just sync that one
        if let Some(account_id) = self.selected_account {
            self.sync_account(account_id, cx);
            return;
        }

        // Otherwise sync all accounts
        let account_ids: Vec<i64> = self.accounts.keys().copied().collect();
        for account_id in account_ids {
            self.sync_account(account_id, cx);
        }
    }

    /// Navigate to thread list view
    pub fn show_inbox(&mut self, cx: &mut Context<Self>) {
        if self.thread_list_view.is_none() {
            let store = self.store.clone();
            self.thread_list_view = Some(cx.new(|cx| ThreadListView::new(store, cx)));
        }
        // Hide the WebView when not viewing a thread
        self.hide_webview(cx);
        // Clean up thread view
        self.thread_view = None;
        self.current_view = View::Inbox;
        self.thread_list_context = ListContext::Inbox;
        // Focus thread list on next render
        self.pending_focus = Some(PendingFocus::ThreadList);
        cx.notify();
    }

    /// Navigate to thread view
    pub fn show_thread(&mut self, thread_id: ThreadId, cx: &mut Context<Self>) {
        // Track which list context we're coming from
        self.thread_list_context = match self.current_view {
            View::Search => ListContext::Search,
            _ => ListContext::Inbox,
        };

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
            let mut view = ThreadView::new(store, thread_id.clone(), cx);
            view.set_app(app_handle);
            view.load_thread(cx);
            view
        }));
        self.current_view = View::Thread {
            html: thread_html.clone(),
            thread_id: thread_id_clone.clone(),
        };
        // Focus thread view on next render
        self.pending_focus = Some(PendingFocus::ThreadView);
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

    /// Refresh the unread count for the Inbox label from storage
    fn refresh_inbox_unread_count(&mut self) {
        let unread_count = self
            .store
            .count_unread_threads_by_label(LabelId::INBOX)
            .unwrap_or(0) as u32;

        // Update the Inbox label's unread count
        for label in &mut self.labels {
            if label.id.as_str() == LabelId::INBOX {
                label.unread_count = unread_count;
                break;
            }
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
        // Focus thread list on next render
        self.pending_focus = Some(PendingFocus::ThreadList);

        // Trigger sync when navigating to a different label
        self.try_sync(cx);

        cx.notify();
    }

    /// Check if enough time has passed since last sync to allow a new sync.
    ///
    /// Returns false if:
    /// - Already syncing
    /// - Gmail client not configured
    /// - Last sync was less than `sync_cooldown_secs` ago
    fn should_sync(&self) -> bool {
        if self.is_syncing || self.gmail_client.is_none() {
            return false;
        }
        mail::cooldown_elapsed(self.last_sync_at, self.sync_cooldown_secs)
    }

    /// Try to sync if cooldown has elapsed.
    ///
    /// This is the preferred way to trigger syncs from activity handlers
    /// (label navigation, actions completing, window focus, etc).
    fn try_sync(&mut self, cx: &mut Context<Self>) {
        if self.should_sync() {
            debug!("try_sync: cooldown elapsed, starting sync");
            self.sync(cx);
        } else {
            debug!("try_sync: skipping sync (cooldown not elapsed or already syncing)");
        }
    }

    /// Start background polling for new mail.
    ///
    /// Runs a loop that syncs every `poll_interval_secs` seconds.
    /// Polling stops if Gmail client is removed or app is dropped.
    fn start_polling(&mut self, cx: &mut Context<Self>) {
        use std::time::Duration;

        // Cancel existing poll task if any
        self.poll_task = None;

        let interval = Duration::from_secs(self.poll_interval_secs);
        info!(
            "Starting background sync polling (interval: {}s)",
            self.poll_interval_secs
        );

        self.poll_task = Some(cx.spawn(async move |this, cx| {
            loop {
                // Wait for the polling interval
                cx.background_executor().timer(interval).await;

                // Try to sync
                let should_continue = cx
                    .update(|cx| {
                        this.update(cx, |app, cx| {
                            app.try_sync(cx);
                            // Continue polling only if gmail is configured
                            app.gmail_client.is_some()
                        })
                        .unwrap_or(false)
                    })
                    .unwrap_or(false);

                if !should_continue {
                    info!("Stopping background sync polling (gmail client removed)");
                    break;
                }
            }
        }));
    }

    /// Trigger sync for a specific account
    ///
    /// This is the preferred way to sync individual accounts in multi-account mode.
    pub fn sync_account(&mut self, account_id: i64, cx: &mut Context<Self>) {
        use mail::{fetch_phase, process_pending_batch};
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::time::Duration;

        // Check if this account is already syncing
        let Some(account_state) = self.accounts.get_mut(&account_id) else {
            warn!("[SYNC] Account {} not found", account_id);
            return;
        };

        if account_state.is_syncing {
            debug!("[SYNC] Account {} already syncing", account_id);
            return;
        }

        let client = account_state.gmail_client.clone();
        let account_email = account_state.account.email.clone();

        // Mark account as syncing
        account_state.is_syncing = true;
        cx.notify();

        info!("[SYNC] Starting sync for account {} (id={})", account_email, account_id);

        let store = self.store.clone();
        let search_index = self.search_index.clone();
        let background = cx.background_executor().clone();

        cx.spawn(async move |this, cx| {
            let options = SyncOptions {
                search_index: search_index.clone(),
                ..Default::default()
            };

            // Get history_id for sync state
            let client_for_profile = client.clone();
            let profile_result = background
                .spawn(async move { client_for_profile.get_profile() })
                .await;

            let history_id = match profile_result {
                Ok(profile) => {
                    info!(
                        "[SYNC] Account {} history_id={}",
                        account_email, profile.history_id
                    );
                    Some(profile.history_id)
                }
                Err(e) => {
                    warn!("[SYNC] Failed to get profile for {}: {}", account_email, e);
                    None
                }
            };

            // Check for existing sync state
            let existing_sync_state = store.get_sync_state(account_id).ok().flatten();
            let sync_info = mail::get_sync_state_info(existing_sync_state.as_ref());
            let has_existing_sync = sync_info.last_sync_at.is_some();

            // First sync for this account - clear any stale data
            if !has_existing_sync {
                info!("[SYNC] First sync for account {} - clearing account data", account_email);
                if let Err(e) = store.clear_account_data(account_id) {
                    warn!("[SYNC] Failed to clear account data: {}", e);
                }
            }

            // Determine sync action
            let sync_action = mail::determine_sync_action(existing_sync_state.as_ref(), false);
            debug!("[SYNC] Account {} sync action: {:?}", account_email, sync_action);

            // Handle incremental sync (fast path)
            if let mail::SyncAction::IncrementalSync { history_id: _ } = &sync_action {
                if let Some(ref state) = existing_sync_state {
                    info!("[SYNC] Account {} performing incremental sync", account_email);

                    let store_for_sync = store.clone();
                    let client_for_sync = client.clone();
                    let options_for_sync = options.clone();
                    let state_clone = state.clone();

                    let sync_result = background
                        .spawn(async move {
                            mail::incremental_sync(
                                &client_for_sync,
                                store_for_sync.as_ref(),
                                &state_clone,
                                &options_for_sync,
                            )
                        })
                        .await;

                    match sync_result {
                        Ok(stats) => {
                            info!(
                                "[SYNC] Account {} incremental sync complete: {} created, {} updated",
                                account_email, stats.messages_created, stats.messages_updated
                            );

                            // Update account state
                            cx.update(|cx| {
                                this.update(cx, |app, cx| {
                                    if let Some(state) = app.accounts.get_mut(&account_id) {
                                        state.is_syncing = false;
                                        state.last_sync_at = Some(chrono::Utc::now());
                                        state.sync_error = None;
                                    }
                                    // Refresh thread list
                                    if let Some(thread_list) = &app.thread_list_view {
                                        thread_list.update(cx, |view, cx| view.load_threads(cx));
                                    }
                                    cx.notify();
                                })
                            })
                            .ok();
                            return;
                        }
                        Err(e) => {
                            if e.downcast_ref::<mail::HistoryExpiredError>().is_some() {
                                info!("[SYNC] Account {} history expired, will do full sync", account_email);
                                let _ = store.delete_sync_state(account_id);
                            } else {
                                error!("[SYNC] Account {} incremental sync failed: {}", account_email, e);
                                cx.update(|cx| {
                                    this.update(cx, |app, cx| {
                                        if let Some(state) = app.accounts.get_mut(&account_id) {
                                            state.is_syncing = false;
                                            state.sync_error = Some(format!("Sync failed: {}", e));
                                        }
                                        cx.notify();
                                    })
                                })
                                .ok();
                                return;
                            }
                        }
                    }
                }
            }

            // Full sync path
            // Save partial sync state
            if !sync_info.needs_resume {
                if let Some(ref history_id) = history_id {
                    let partial_state = SyncState::partial(account_id, history_id);
                    if let Err(e) = store.save_sync_state(partial_state) {
                        warn!("[SYNC] Failed to save partial sync state: {}", e);
                    }
                }
            }

            let mut stats = SyncStats::default();
            let fetch_done = Arc::new(AtomicBool::new(false));
            let fetch_error: Arc<std::sync::Mutex<Option<String>>> =
                Arc::new(std::sync::Mutex::new(None));

            // Fetch phase
            let store_for_fetch = store.clone();
            let client_clone = client.clone();
            let options_clone = options.clone();
            let fetch_done_clone = fetch_done.clone();
            let fetch_error_clone = fetch_error.clone();

            background
                .spawn(async move {
                    let mut fetch_stats = SyncStats::default();
                    match fetch_phase(
                        &client_clone,
                        store_for_fetch.as_ref(),
                        account_id,
                        &options_clone,
                        &mut fetch_stats,
                    ) {
                        Ok(_) => {
                            info!("[SYNC] Account {} fetch phase complete", account_id);
                        }
                        Err(e) => {
                            error!("[SYNC] Account {} fetch phase failed: {}", account_id, e);
                            *fetch_error_clone.lock().unwrap() = Some(e.to_string());
                        }
                    }
                    fetch_done_clone.store(true, Ordering::SeqCst);
                })
                .detach();

            // Process phase
            let batch_size = 50;
            let mut consecutive_empty = 0;

            loop {
                // Check for fetch errors
                if let Some(ref err) = *fetch_error.lock().unwrap() {
                    error!("[SYNC] Account {} stopping due to fetch error: {}", account_id, err);
                    cx.update(|cx| {
                        this.update(cx, |app, cx| {
                            if let Some(state) = app.accounts.get_mut(&account_id) {
                                state.is_syncing = false;
                                state.sync_error = Some(err.clone());
                            }
                            cx.notify();
                        })
                    })
                    .ok();
                    return;
                }

                let store_for_batch = store.clone();
                let options_clone = options.clone();
                let mut batch_stats = stats.clone();

                let batch_result = background
                    .spawn(async move {
                        process_pending_batch(
                            store_for_batch.as_ref(),
                            account_id,
                            &options_clone,
                            &mut batch_stats,
                            batch_size,
                        )
                        .map(|result| (result, batch_stats))
                    })
                    .await;

                match batch_result {
                    Ok((result, updated_stats)) => {
                        stats = updated_stats;
                        let processed = result.processed;
                        let remaining = result.remaining;

                        if processed > 0 {
                            consecutive_empty = 0;
                            cx.update(|cx| {
                                this.update(cx, |app, cx| {
                                    if let Some(thread_list) = &app.thread_list_view {
                                        thread_list.update(cx, |view, cx| view.load_threads(cx));
                                    }
                                    debug!(
                                        "[SYNC] Account {} processed {} messages, {} remaining",
                                        account_id, processed, remaining
                                    );
                                    cx.notify();
                                })
                            })
                            .ok();
                        } else {
                            consecutive_empty += 1;
                        }

                        let is_fetch_done = fetch_done.load(Ordering::SeqCst);
                        if !result.has_more && is_fetch_done {
                            let store_for_check = store.clone();
                            let final_pending = background
                                .spawn(async move {
                                    store_for_check
                                        .count_pending_messages(account_id, None)
                                        .unwrap_or(0)
                                })
                                .await;

                            if final_pending == 0 {
                                debug!("[SYNC] Account {} no more pending messages", account_id);
                                break;
                            }
                        }

                        if !result.has_more && !is_fetch_done {
                            std::thread::sleep(Duration::from_millis(50));
                        }

                        if consecutive_empty > 100 && is_fetch_done {
                            debug!("[SYNC] Account {} safety exit after {} empty batches", account_id, consecutive_empty);
                            break;
                        }
                    }
                    Err(e) => {
                        error!("[SYNC] Account {} process batch failed: {}", account_id, e);
                        cx.update(|cx| {
                            this.update(cx, |app, cx| {
                                if let Some(state) = app.accounts.get_mut(&account_id) {
                                    state.is_syncing = false;
                                    state.sync_error = Some(format!("Process failed: {}", e));
                                }
                                cx.notify();
                            })
                        })
                        .ok();
                        return;
                    }
                }
            }

            // Mark sync complete
            if let Some(ref history_id) = history_id {
                let complete_state = SyncState::new(account_id, history_id);
                if let Err(e) = store.save_sync_state(complete_state) {
                    error!("[SYNC] Account {} failed to mark sync complete: {}", account_id, e);
                } else {
                    info!("[SYNC] Account {} sync complete", account_id);
                }
            }

            // Update account state
            cx.update(|cx| {
                this.update(cx, |app, cx| {
                    if let Some(state) = app.accounts.get_mut(&account_id) {
                        state.is_syncing = false;
                        state.last_sync_at = Some(chrono::Utc::now());
                        state.sync_error = None;
                    }
                    // Also update legacy last_sync_at for UI
                    app.last_sync_at = Some(chrono::Utc::now());
                    // Refresh thread list
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

    /// Trigger inbox sync with event-driven UI updates
    ///
    /// Runs fetch and process phases in parallel for maximum throughput.
    /// SQLite handles concurrent access properly with WAL mode.
    ///
    /// When transitioning from unauthenticated to authenticated (first sync after OAuth),
    /// clears the database and search index to start fresh.
    pub fn sync(&mut self, cx: &mut Context<Self>) {
        use mail::{fetch_phase, process_pending_batch};
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::time::Duration;

        if self.is_syncing {
            return;
        }

        let Some(client) = self.gmail_client.clone() else {
            self.sync_error = Some("Gmail client not configured".to_string());
            cx.notify();
            return;
        };

        // Check if we have an existing sync state (meaning we've synced before)
        // This is different from is_authenticated() which checks token validity
        // Token can expire but we still have valid sync data
        let has_existing_sync = self.last_sync_at.is_some();
        debug!(
            "[SYNC] has_existing_sync: {}, is_authenticated: {}",
            has_existing_sync,
            client.is_authenticated()
        );

        self.is_syncing = true;
        self.sync_error = None;
        cx.notify();

        let store = self.store.clone();
        let search_index = self.search_index.clone();
        let background = cx.background_executor().clone();
        // Use primary account or fallback to 1 for legacy compatibility
        let account_id = self.current_account_id_or_default();

        cx.spawn(async move |this, cx| {
            let options = SyncOptions {
                search_index: search_index.clone(),
                ..Default::default()
            };

            // Capture history_id and profile at start of sync
            // history_id is needed for saving sync state (incremental sync)
            // email_address is for display in sidebar
            let client_for_profile = client.clone();
            let profile_result = background
                .spawn(async move { client_for_profile.get_profile() })
                .await;

            let (history_id, profile_email) = match profile_result {
                Ok(profile) => {
                    info!(
                        "[SYNC] Captured profile: email={}, history_id={}",
                        profile.email_address, profile.history_id
                    );
                    (Some(profile.history_id), Some(profile.email_address))
                }
                Err(e) => {
                    warn!("[SYNC] Failed to get profile: {}", e);
                    (None, None)
                }
            };

            // Update profile email immediately
            if let Some(email) = profile_email {
                cx.update(|cx| {
                    this.update(cx, |app, cx| {
                        app.profile_email = Some(email);
                        cx.notify();
                    })
                })
                .ok();
            }

            // Only clear data on truly first sync (no existing sync state)
            // NOT when token is just expired - we have valid data, just need refresh
            if !has_existing_sync {
                info!("[SYNC] First sync ever - clearing any stale data");

                // Clear store and search index on background thread
                let store_for_clear = store.clone();
                let search_index_for_clear = search_index.clone();
                let clear_result = background
                    .spawn(async move {
                        // Clear mail data (but not sync state yet - that's managed by sync_gmail)
                        store_for_clear.clear()?;
                        info!("[SYNC] Cleared mail store");

                        // Clear search index
                        if let Some(ref index) = search_index_for_clear {
                            index.clear()?;
                            info!("[SYNC] Cleared search index");
                        }
                        Ok::<(), anyhow::Error>(())
                    })
                    .await;

                if let Err(e) = clear_result {
                    error!("[SYNC] Failed to clear data: {}", e);
                    cx.update(|cx| {
                        this.update(cx, |app, cx| {
                            app.sync_error = Some(format!("Failed to clear data: {}", e));
                            app.is_syncing = false;
                            cx.notify();
                        })
                    })
                    .ok();
                    return;
                }
            }

            // Save partial sync state immediately with history_id
            // This ensures incremental sync works even if app crashes during sync
            // Must be after clear (which deletes sync_state) but before fetch starts
            //
            // IMPORTANT: Only save a NEW partial state if we don't already have one.
            // If we're resuming an incomplete sync, the existing state has the page_token
            // and failed_message_ids that we need to resume from.
            let existing_sync_state = store.get_sync_state(account_id).ok().flatten();
            let sync_info = mail::get_sync_state_info(existing_sync_state.as_ref());

            // Determine what sync action to take
            let sync_action = mail::determine_sync_action(existing_sync_state.as_ref(), false);
            debug!("[SYNC] Sync action: {:?}", sync_action);

            // Check if we should do an incremental sync (fast path using History API)
            if let mail::SyncAction::IncrementalSync { history_id: _ } = &sync_action {
                if let Some(ref state) = existing_sync_state {
                    info!("[SYNC] Performing incremental sync using History API");

                    let store_for_sync = store.clone();
                    let client_for_sync = client.clone();
                    let options_for_sync = options.clone();
                    let state_clone = state.clone();

                    let sync_result = background
                        .spawn(async move {
                            mail::incremental_sync(
                                &client_for_sync,
                                store_for_sync.as_ref(),
                                &state_clone,
                                &options_for_sync,
                            )
                        })
                        .await;

                    match sync_result {
                        Ok(stats) => {
                            info!(
                                "[SYNC] Incremental sync complete: {} created, {} updated",
                                stats.messages_created, stats.messages_updated
                            );

                            // Update the sync state with new history_id
                            if let Some(ref new_history_id) = history_id {
                                let updated_state = SyncState::new(account_id, new_history_id);
                                if let Err(e) = store.save_sync_state(updated_state) {
                                    warn!("[SYNC] Failed to update sync state: {}", e);
                                }
                            }

                            // Update UI
                            cx.update(|cx| {
                                this.update(cx, |app, cx| {
                                    app.is_syncing = false;
                                    app.last_sync_at = Some(Utc::now());

                                    // Reload thread list
                                    if let Some(thread_list) = &app.thread_list_view {
                                        thread_list.update(cx, |view, cx| view.load_threads(cx));
                                    }

                                    // Update inbox unread count
                                    app.refresh_inbox_unread_count();

                                    // Start background polling if not already running
                                    if app.poll_task.is_none() && app.gmail_client.is_some() {
                                        app.start_polling(cx);
                                    }

                                    cx.notify();
                                })
                            })
                            .ok();
                            return;
                        }
                        Err(e) => {
                            // Check if it's a history expired error - need full resync
                            if e.downcast_ref::<mail::HistoryExpiredError>().is_some() {
                                warn!("[SYNC] History ID expired, falling back to full sync");
                                // Clear data and continue with full sync below
                                if let Err(clear_err) = store.clear() {
                                    error!("[SYNC] Failed to clear store: {}", clear_err);
                                }
                                if let Some(ref index) = search_index {
                                    if let Err(clear_err) = index.clear() {
                                        error!("[SYNC] Failed to clear search index: {}", clear_err);
                                    }
                                }
                                // Delete sync state to trigger fresh initial sync
                                let _ = store.delete_sync_state(account_id);
                            } else {
                                // Other error - report and stop
                                error!("[SYNC] Incremental sync failed: {}", e);
                                cx.update(|cx| {
                                    this.update(cx, |app, cx| {
                                        app.sync_error = Some(format!("Sync failed: {}", e));
                                        app.is_syncing = false;
                                        cx.notify();
                                    })
                                })
                                .ok();
                                return;
                            }
                        }
                    }
                }
            }

            // Full sync path: Initial sync or resume incomplete sync
            if !sync_info.needs_resume {
                if let Some(ref history_id) = history_id {
                    let partial_state = SyncState::partial(account_id, history_id);
                    if let Err(e) = store.save_sync_state(partial_state) {
                        warn!("[SYNC] Failed to save partial sync state: {}", e);
                    } else {
                        info!(
                            "[SYNC] Saved partial sync state with history_id: {}",
                            history_id
                        );
                    }
                }
            } else if let Some(ref progress) = sync_info.resume_progress {
                info!(
                    "[SYNC] Resuming existing sync (page_token={}, messages_listed={}, failed_ids={})",
                    progress.has_page_token,
                    progress.messages_listed,
                    progress.failed_message_count
                );
            }

            // Track when fetch phase is done
            let fetch_done = Arc::new(AtomicBool::new(false));
            let fetch_error = Arc::new(std::sync::Mutex::new(None::<String>));

            // Start fetch phase in background (runs in parallel with process)
            debug!("[SYNC] Starting fetch phase (parallel)...");
            let store_for_fetch = store.clone();
            let client_clone = client.clone();
            let options_clone = options.clone();
            let fetch_done_clone = fetch_done.clone();
            let fetch_error_clone = fetch_error.clone();

            background
                .spawn(async move {
                    let mut fetch_stats = SyncStats::default();
                    match fetch_phase(
                        &client_clone,
                        store_for_fetch.as_ref(),
                        account_id,
                        &options_clone,
                        &mut fetch_stats,
                    ) {
                        Ok(stats) => {
                            debug!(
                                "[SYNC] Fetch phase complete: {} fetched, {} pending",
                                stats.fetched, stats.pending
                            );
                        }
                        Err(e) => {
                            error!("[SYNC] Fetch phase failed: {}", e);
                            *fetch_error_clone.lock().unwrap() =
                                Some(format!("Fetch failed: {}", e));
                        }
                    }
                    fetch_done_clone.store(true, Ordering::SeqCst);
                })
                .detach();

            // Process pending messages in batches (runs in parallel with fetch)
            debug!("[SYNC] Starting process phase (parallel)...");
            let batch_size = 100;
            let mut stats = SyncStats::default();
            let mut consecutive_empty = 0;

            loop {
                // Check for fetch errors
                if let Some(err_msg) = fetch_error.lock().unwrap().take() {
                    cx.update(|cx| {
                        this.update(cx, |app, cx| {
                            app.sync_error = Some(err_msg);
                            app.is_syncing = false;
                            cx.notify();
                        })
                    })
                    .ok();
                    return;
                }

                let store_for_batch = store.clone();
                let options_clone = options.clone();
                let mut batch_stats = stats.clone();

                // Process one batch on background thread
                let batch_result = background
                    .spawn(async move {
                        process_pending_batch(
                            store_for_batch.as_ref(),
                            account_id,
                            &options_clone,
                            &mut batch_stats,
                            batch_size,
                        )
                        .map(|result| (result, batch_stats))
                    })
                    .await;

                match batch_result {
                    Ok((result, updated_stats)) => {
                        stats = updated_stats;
                        let processed = result.processed;
                        let remaining = result.remaining;

                        if processed > 0 {
                            consecutive_empty = 0;
                            // Update UI with new data
                            cx.update(|cx| {
                                this.update(cx, |app, cx| {
                                    if let Some(thread_list) = &app.thread_list_view {
                                        thread_list.update(cx, |view, cx| view.load_threads(cx));
                                    }
                                    debug!(
                                        "[SYNC] Processed {} messages, {} remaining",
                                        processed, remaining
                                    );
                                    cx.notify();
                                })
                            })
                            .ok();
                        } else {
                            consecutive_empty += 1;
                        }

                        // Exit conditions:
                        // 1. No more messages AND fetch is done AND fresh count confirms empty
                        // 2. Multiple consecutive empty batches AND fetch is done (belt and suspenders)
                        let is_fetch_done = fetch_done.load(Ordering::SeqCst);
                        if !result.has_more && is_fetch_done {
                            // Race condition guard: fetch may have added messages after our
                            // count check but before setting fetch_done. Do a fresh check.
                            let store_for_check = store.clone();
                            let final_pending = background
                                .spawn(async move {
                                    store_for_check.count_pending_messages(account_id, None).unwrap_or(0)
                                })
                                .await;

                            if final_pending == 0 {
                                debug!("[SYNC] No more pending messages and fetch complete");
                                break;
                            } else {
                                debug!(
                                    "[SYNC] Found {} pending after fetch completed, continuing",
                                    final_pending
                                );
                                // Don't break - continue processing the remaining messages
                            }
                        }

                        // If no pending but fetch still running, wait a bit before polling again
                        if !result.has_more && !is_fetch_done {
                            std::thread::sleep(Duration::from_millis(50));
                        }

                        // Safety valve: if we've had many empty batches in a row
                        if consecutive_empty > 100 && is_fetch_done {
                            debug!(
                                "[SYNC] Safety exit after {} empty batches",
                                consecutive_empty
                            );
                            break;
                        }
                    }
                    Err(e) => {
                        error!("Process batch failed: {}", e);
                        cx.update(|cx| {
                            this.update(cx, |app, cx| {
                                app.sync_error = Some(format!("Process failed: {}", e));
                                cx.notify();
                            })
                        })
                        .ok();
                        break;
                    }
                }
            }

            // Mark sync state as complete
            // This updates the partial state saved at the start to indicate sync finished
            if let Some(ref history_id) = history_id {
                let complete_state = SyncState::new(account_id, history_id);
                if let Err(e) = store.save_sync_state(complete_state) {
                    error!("[SYNC] Failed to mark sync complete: {}", e);
                } else {
                    info!(
                        "[SYNC] Marked sync complete with history_id: {}",
                        history_id
                    );
                }
            }

            // Sync complete
            cx.update(|cx| {
                this.update(cx, |app, cx| {
                    app.is_syncing = false;
                    app.last_sync_at = Some(Utc::now());

                    info!(
                        "Sync complete: {} created, {} skipped",
                        stats.messages_created, stats.messages_skipped,
                    );

                    // Final reload
                    if let Some(thread_list) = &app.thread_list_view {
                        thread_list.update(cx, |view, cx| view.load_threads(cx));
                    }

                    // Update inbox unread count
                    app.refresh_inbox_unread_count();

                    // Start background polling if not already running
                    // (handles case where first sync was triggered manually after OAuth)
                    if app.poll_task.is_none() && app.gmail_client.is_some() {
                        app.start_polling(cx);
                    }

                    cx.notify();
                })
            })
            .ok();
        })
        .detach();
    }

    fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let theme = cx.theme();
        let labels = self.labels.clone();
        let selected = self.selected_label.clone();
        // Check if any account is syncing (or the app-level legacy flag)
        let is_syncing =
            self.is_syncing || self.accounts.values().any(|state| state.is_syncing);
        let last_sync = self.last_sync_at;

        // Gather accounts for the account section
        let accounts: Vec<_> = self.accounts.values().map(|s| s.account.clone()).collect();
        let selected_account = self.selected_account;
        let has_accounts = !accounts.is_empty();

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
            // Account section (always show - at minimum has Add Account button)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .px_2()
                    .pb_2()
                    .border_b_1()
                    .border_color(theme.border)
                    // Section header
                    .child(
                        div()
                            .px_1()
                            .py_1()
                            .text_xs()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.muted_foreground)
                            .child("ACCOUNTS"),
                    )
                    // All Accounts option (only when we have accounts)
                    .when(has_accounts, |el| {
                        el.child(
                            div()
                                .id("account-all")
                                .on_click(cx.listener(|app, _event, _window, cx| {
                                    app.set_account_filter(None, cx);
                                }))
                                .child(AllAccountsItem::new(selected_account.is_none())),
                        )
                    })
                        // Individual accounts
                        .children(accounts.into_iter().map(|account| {
                            let account_id = account.id;
                            let is_selected = selected_account == Some(account_id);
                            let is_account_syncing = self
                                .accounts
                                .get(&account_id)
                                .map(|s| s.is_syncing)
                                .unwrap_or(false);

                            div()
                                .id(ElementId::Name(format!("account-{}", account_id).into()))
                                .on_click(cx.listener(move |app, _event, _window, cx| {
                                    app.set_account_filter(Some(account_id), cx);
                                }))
                                .child(
                                    AccountItem::new(account, is_selected).syncing(is_account_syncing),
                                )
                        }))
                        // Add Account button
                        .child(
                            div()
                                .id("add-account")
                                .px_2()
                                .py_1()
                                .mx_1()
                                .mt_1()
                                .rounded_md()
                                .cursor_pointer()
                                .hover(|s| s.bg(theme.list_hover))
                                .on_click(cx.listener(|app, _event, _window, cx| {
                                    app.add_account(cx);
                                }))
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap_2()
                                        .text_sm()
                                        .text_color(theme.muted_foreground)
                                        .child(
                                            Icon::new(IconName::Plus)
                                                .with_size(ComponentSize::XSmall)
                                                .text_color(theme.muted_foreground),
                                        )
                                        .child("Add Account"),
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
                                        app.sync_all_accounts(cx);
                                    })),
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
    fn handle_focus_search(
        &mut self,
        _: &FocusSearch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus_search(window, cx);
    }

    fn handle_show_shortcuts(
        &mut self,
        _: &ShowShortcuts,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.show_shortcuts_help = !self.show_shortcuts_help;
        cx.notify();
    }

    /// Dismiss current context and ascend view hierarchy.
    /// Priority: Overlay → Thread → Search → Inbox (no-op)
    pub fn dismiss(&mut self, cx: &mut Context<Self>) {
        // First priority: close any overlay
        if self.show_shortcuts_help {
            self.show_shortcuts_help = false;
            cx.notify();
            return;
        }

        // Second: dismiss based on current view hierarchy
        match &self.current_view {
            View::Thread { .. } => {
                // Thread → List (based on where thread was opened from)
                self.hide_webview(cx);
                self.thread_view = None;
                match self.thread_list_context {
                    ListContext::Search => {
                        self.current_view = View::Search;
                        self.pending_focus_results = true;
                        cx.notify();
                    }
                    ListContext::Inbox => {
                        self.show_inbox(cx);
                    }
                }
            }
            View::Search => {
                // Search → Inbox
                self.search_results_view = None;
                self.show_inbox(cx);
            }
            View::Inbox => {
                // Already at top level, no-op
            }
        }
    }

    fn handle_dismiss(&mut self, _: &Dismiss, _window: &mut Window, cx: &mut Context<Self>) {
        self.dismiss(cx);
    }

    // Go-to folder handlers
    fn handle_go_to_inbox(&mut self, _: &GoToInbox, _window: &mut Window, cx: &mut Context<Self>) {
        self.select_label(LabelId::INBOX.to_string(), cx);
    }

    fn handle_go_to_starred(
        &mut self,
        _: &GoToStarred,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_label(LabelId::STARRED.to_string(), cx);
    }

    fn handle_go_to_sent(&mut self, _: &GoToSent, _window: &mut Window, cx: &mut Context<Self>) {
        self.select_label(LabelId::SENT.to_string(), cx);
    }

    fn handle_go_to_drafts(
        &mut self,
        _: &GoToDrafts,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_label(LabelId::DRAFTS.to_string(), cx);
    }

    fn handle_go_to_trash(&mut self, _: &GoToTrash, _window: &mut Window, cx: &mut Context<Self>) {
        self.select_label(LabelId::TRASH.to_string(), cx);
    }

    fn handle_go_to_all_mail(
        &mut self,
        _: &GoToAllMail,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_label("ALL".to_string(), cx);
    }

    /// Handle G-sequence key events
    fn handle_key_down(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // If waiting for G-sequence second key
        if self.pending_g_sequence {
            self.pending_g_sequence = false;

            match event.keystroke.key.as_str() {
                "i" => self.select_label(LabelId::INBOX.to_string(), cx),
                "s" => self.select_label(LabelId::STARRED.to_string(), cx),
                "t" => self.select_label(LabelId::SENT.to_string(), cx),
                "d" => self.select_label(LabelId::DRAFTS.to_string(), cx),
                "a" => self.select_label("ALL".to_string(), cx),
                "#" | "3" => self.select_label(LabelId::TRASH.to_string(), cx),
                _ => {} // Ignore other keys
            }
            cx.notify();
            return;
        }

        // Check for G key to start sequence (no modifiers)
        let mods = &event.keystroke.modifiers;
        let no_modifiers = !mods.shift && !mods.control && !mods.alt && !mods.platform;
        if event.keystroke.key == "g" && no_modifiers {
            self.pending_g_sequence = true;
            cx.notify();
        }
    }
}

impl Render for OrionApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Sync when window becomes active (foreground)
        let is_active = window.is_window_active();
        if is_active && !self.was_window_active {
            debug!("Window became active, triggering sync");
            self.try_sync(cx);
        }
        self.was_window_active = is_active;

        // Handle pending focus on search results (from Enter key in search box)
        if self.pending_focus_results {
            self.pending_focus_results = false;
            if let Some(ref results_view) = self.search_results_view {
                results_view.update(cx, |view, cx| {
                    view.focus(window, cx);
                });
            }
        }

        // Handle pending focus from navigation
        if let Some(pending_focus) = self.pending_focus.take() {
            match pending_focus {
                PendingFocus::ThreadList => {
                    if let Some(ref thread_list) = self.thread_list_view {
                        thread_list.update(cx, |view, cx| {
                            view.focus(window, cx);
                        });
                    }
                }
                PendingFocus::ThreadView => {
                    if let Some(ref thread_view) = self.thread_view {
                        thread_view.update(cx, |view, cx| {
                            view.focus(window, cx);
                        });
                    }
                }
            }
        }

        let theme = cx.theme();
        // Clone theme colors upfront to avoid borrow conflicts
        let bg = theme.background;
        let fg = theme.foreground;
        let secondary_bg = theme.secondary;
        let border = theme.border;
        let g_indicator_bg = theme.secondary;
        let g_indicator_fg = theme.foreground;

        let sidebar = self.render_sidebar(cx);
        let search_box = self.get_or_create_search_box(window, cx);
        let content = self.render_content(window, cx);

        // G-sequence indicator
        let g_sequence_indicator = if self.pending_g_sequence {
            Some(
                div()
                    .absolute()
                    .bottom_4()
                    .right_4()
                    .px_3()
                    .py_2()
                    .bg(g_indicator_bg)
                    .rounded_md()
                    .shadow_md()
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(g_indicator_fg)
                    .child("G..."),
            )
        } else {
            None
        };

        // Shortcuts help overlay - hide webview when showing overlay
        let shortcuts_overlay = if self.show_shortcuts_help {
            // Hide webview so it doesn't appear above the overlay
            if let Some(ref webview) = self.webview {
                webview.update(cx, |wv, _| wv.hide());
            }
            Some(ShortcutsHelp::new())
        } else {
            None
        };

        div()
            .key_context("OrionApp")
            .on_action(cx.listener(Self::handle_focus_search))
            .on_action(cx.listener(Self::handle_show_shortcuts))
            .on_action(cx.listener(Self::handle_dismiss))
            .on_action(cx.listener(Self::handle_go_to_inbox))
            .on_action(cx.listener(Self::handle_go_to_starred))
            .on_action(cx.listener(Self::handle_go_to_sent))
            .on_action(cx.listener(Self::handle_go_to_drafts))
            .on_action(cx.listener(Self::handle_go_to_trash))
            .on_action(cx.listener(Self::handle_go_to_all_mail))
            .on_key_down(cx.listener(Self::handle_key_down))
            .relative()
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
            // G-sequence indicator
            .children(g_sequence_indicator)
            // Shortcuts help overlay
            .children(shortcuts_overlay)
    }
}
