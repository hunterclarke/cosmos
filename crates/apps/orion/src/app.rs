//! Root application component for Orion mail app

use gpui::prelude::*;
use gpui::*;
use mail::{GmailAuth, GmailClient, ThreadId, storage::InMemoryMailStore, sync_inbox};
use std::sync::Arc;

use crate::views::{InboxView, ThreadView};

/// Current view in the application
#[derive(Clone)]
#[allow(dead_code)]
pub enum View {
    Inbox,
    Thread(ThreadId),
}

/// Root application state
pub struct OrionApp {
    current_view: View,
    store: Arc<InMemoryMailStore>,
    gmail_client: Option<Arc<GmailClient>>,
    is_syncing: bool,
    sync_error: Option<String>,
    pub inbox_view: Option<Entity<InboxView>>,
    thread_view: Option<Entity<ThreadView>>,
}

impl OrionApp {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let store = Arc::new(InMemoryMailStore::new());
        let inbox_view = cx.new(|_| InboxView::new(store.clone()));

        Self {
            current_view: View::Inbox,
            store,
            gmail_client: None,
            is_syncing: false,
            sync_error: None,
            inbox_view: Some(inbox_view),
            thread_view: None,
        }
    }

    /// Initialize Gmail client with credentials
    pub fn init_gmail(&mut self, client_id: String, client_secret: String) -> anyhow::Result<()> {
        let auth = GmailAuth::new(client_id, client_secret)?;
        let client = GmailClient::new(auth);
        self.gmail_client = Some(Arc::new(client));
        Ok(())
    }

    /// Navigate to inbox view
    #[allow(dead_code)]
    pub fn show_inbox(&mut self, cx: &mut Context<Self>) {
        if self.inbox_view.is_none() {
            self.inbox_view = Some(cx.new(|_| InboxView::new(self.store.clone())));
        }
        self.thread_view = None;
        self.current_view = View::Inbox;
        cx.notify();
    }

    /// Navigate to thread view
    #[allow(dead_code)]
    pub fn show_thread(&mut self, thread_id: ThreadId, cx: &mut Context<Self>) {
        self.thread_view = Some(cx.new(|cx| {
            let mut view = ThreadView::new(self.store.clone(), thread_id.clone());
            view.load_thread(cx);
            view
        }));
        self.current_view = View::Thread(thread_id);
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

        // Run sync on background thread (it's blocking I/O)
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            // Execute sync on background thread pool
            let result = background
                .spawn(async move { sync_inbox(&client, store.as_ref(), 100) })
                .await;

            cx.update(|cx| {
                this.update(cx, |app, cx| {
                    app.is_syncing = false;
                    match result {
                        Ok(stats) => {
                            println!(
                                "Sync complete: {} fetched, {} stored, {} skipped",
                                stats.messages_fetched,
                                stats.messages_stored,
                                stats.messages_skipped
                            );
                            // Reload inbox
                            if let Some(inbox) = &app.inbox_view {
                                inbox.update(cx, |view, cx| view.load_threads(cx));
                            }
                        }
                        Err(e) => {
                            eprintln!("Sync failed: {}", e);
                            app.sync_error = Some(format!("{}", e));
                        }
                    }
                    cx.notify();
                })
            })
        })
        .detach();
    }

    fn render_header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let is_syncing = self.is_syncing;

        div()
            .w_full()
            .px_4()
            .py_2()
            .bg(rgb(0x0a0a1a))
            .border_b_1()
            .border_color(rgb(0x303040))
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
                            .text_color(rgb(0x6a8aff))
                            .child("Orion"),
                    )
                    .child(div().text_xs().text_color(rgb(0x666677)).child("Mail")),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .when_some(self.sync_error.clone(), |el, err| {
                        el.child(
                            div()
                                .text_xs()
                                .text_color(rgb(0xff6666))
                                .max_w(px(300.))
                                .text_ellipsis()
                                .child(err),
                        )
                    })
                    .child(
                        div()
                            .id("sync-button")
                            .px_3()
                            .py_1()
                            .rounded_md()
                            .bg(rgb(0x2a3a5a))
                            .cursor_pointer()
                            .hover(|style| style.bg(rgb(0x3a4a6a)))
                            .when(is_syncing, |el| el.opacity(0.5))
                            .on_click(cx.listener(|app, _event, _window, cx| {
                                app.sync(cx);
                            }))
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(rgb(0xccccff))
                                    .child(if is_syncing { "Syncing..." } else { "Sync" }),
                            ),
                    ),
            )
    }

    fn render_content(&mut self, _cx: &mut Context<Self>) -> impl IntoElement {
        match &self.current_view {
            View::Inbox => {
                if let Some(inbox) = &self.inbox_view {
                    inbox.clone().into_any_element()
                } else {
                    div().child("Loading...").into_any_element()
                }
            }
            View::Thread(_) => {
                if let Some(thread) = &self.thread_view {
                    thread.clone().into_any_element()
                } else {
                    div().child("Loading thread...").into_any_element()
                }
            }
        }
    }

    /// Handle thread selection from inbox
    #[allow(dead_code)]
    pub fn on_thread_selected(&mut self, thread_id: ThreadId, cx: &mut Context<Self>) {
        self.show_thread(thread_id, cx);
    }
}

impl Render for OrionApp {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(0x0a0a1a))
            .text_color(rgb(0xffffff))
            .child(self.render_header(cx))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .overflow_hidden()
                    .child(self.render_content(cx)),
            )
    }
}
