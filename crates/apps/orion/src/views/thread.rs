//! Thread view - displays messages within a thread

use gpui::prelude::*;
use gpui::*;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::scroll::ScrollableElement;
use gpui_component::spinner::Spinner;
use gpui_component::theme::Theme;
use gpui_component::{ActiveTheme, Icon, IconName, Sizable, Size as ComponentSize};
use mail::{get_thread_detail, MailStore, Message, ThreadDetail, ThreadId};
use std::sync::Arc;

use crate::app::OrionApp;
use crate::components::MessageCard;

/// Convert HSLA color to CSS hex string
fn hsla_to_hex(color: gpui::Hsla) -> String {
    let rgba = color.to_rgb();
    format!(
        "#{:02x}{:02x}{:02x}",
        (rgba.r * 255.0) as u8,
        (rgba.g * 255.0) as u8,
        (rgba.b * 255.0) as u8
    )
}

/// Generate combined HTML for all messages in a thread with theme colors
///
/// This is called by OrionApp before navigation to generate HTML content
/// that will be loaded into the shared WebView.
pub fn generate_thread_html(messages: &[Message], theme: &Theme) -> String {
    // Convert theme colors to CSS hex strings
    let bg_color = hsla_to_hex(theme.background);
    let card_bg = hsla_to_hex(theme.secondary);
    let border_color = hsla_to_hex(theme.border);
    let fg_color = hsla_to_hex(theme.foreground);
    let muted_color = hsla_to_hex(theme.muted_foreground);
    let link_color = hsla_to_hex(theme.link);

    let mut html = format!(
        r#"<!DOCTYPE html>
<html>
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<style>
* {{ box-sizing: border-box; margin: 0; padding: 0; }}
html, body {{
    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Oxygen, Ubuntu, sans-serif;
    background: {bg_color};
    color: {fg_color};
    padding: 0;
    margin: 0;
    line-height: 1.5;
    min-height: 100%;
}}
/* Dark scrollbar styling */
::-webkit-scrollbar {{
    width: 8px;
    height: 8px;
}}
::-webkit-scrollbar-track {{
    background: {bg_color};
}}
::-webkit-scrollbar-thumb {{
    background: {border_color};
    border-radius: 4px;
}}
::-webkit-scrollbar-thumb:hover {{
    background: {muted_color};
}}
.message {{
    background: {card_bg};
    border: 1px solid {border_color};
    border-radius: 8px;
    margin-bottom: 12px;
    overflow: hidden; /* Clip content to rounded corners */
}}
.message-inner {{
    padding: 16px;
}}
.header {{
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    margin-bottom: 12px;
    padding-bottom: 12px;
    border-bottom: 1px solid {border_color};
}}
.sender {{ font-weight: 600; color: {fg_color}; }}
.email {{ font-size: 12px; color: {muted_color}; margin-top: 2px; }}
.date {{ font-size: 12px; color: {muted_color}; }}
.recipients {{ font-size: 12px; color: {muted_color}; margin-bottom: 12px; }}
.body {{
    color: {fg_color};
    overflow: hidden;
    border-radius: 0 0 6px 6px; /* Round bottom corners to match parent */
}}
.body img {{ max-width: 100%; height: auto; }}
.body a {{ color: {link_color}; }}
.body > * {{ border-radius: inherit; overflow: hidden; }}
.body blockquote {{
    border-left: 3px solid {border_color};
    padding-left: 12px;
    margin: 8px 0;
    color: {muted_color};
}}
</style>
</head>
<body>
"#
    );

    for message in messages {
        let sender_name = message
            .from
            .name
            .as_ref()
            .unwrap_or(&message.from.email)
            .clone();
        let sender_email = &message.from.email;
        let date = {
            use chrono::Local;
            let local = message.received_at.with_timezone(&Local);
            local.format("%b %d, %Y at %H:%M").to_string()
        };
        let recipients: Vec<&str> = message.to.iter().map(|a| a.email.as_str()).collect();
        let recipients_str = recipients.join(", ");

        // Use HTML body if available, otherwise plain text
        let body_content = message
            .body_html
            .as_ref()
            .filter(|h| !h.is_empty())
            .cloned()
            .unwrap_or_else(|| {
                // Escape HTML in plain text and convert newlines
                let text = message
                    .body_text
                    .as_ref()
                    .filter(|t| !t.is_empty())
                    .unwrap_or(&message.body_preview);
                html_escape(text).replace('\n', "<br>")
            });

        html.push_str(&format!(
            r#"<div class="message">
<div class="message-inner">
<div class="header">
<div>
<div class="sender">{}</div>
<div class="email">{}</div>
</div>
<div class="date">{}</div>
</div>
"#,
            html_escape(&sender_name),
            html_escape(sender_email),
            html_escape(&date)
        ));

        if !recipients_str.is_empty() {
            html.push_str(&format!(
                r#"<div class="recipients">To: {}</div>
"#,
                html_escape(&recipients_str)
            ));
        }

        html.push_str(&format!(
            r#"</div>
<div class="body">{}</div>
</div>
"#,
            body_content
        ));
    }

    html.push_str("</body></html>");
    html
}

/// Simple HTML escape for user-generated content
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// Thread view showing messages in a conversation
///
/// When HTML content is available, the parent OrionApp manages the WebView.
/// ThreadView renders just the header, and the app composes the WebView below it.
pub struct ThreadView {
    store: Arc<dyn MailStore>,
    thread_id: ThreadId,
    detail: Option<ThreadDetail>,
    is_loading: bool,
    error_message: Option<String>,
    app: Option<Entity<OrionApp>>,
    /// Whether this thread has HTML content (WebView is managed by app)
    has_html_content: bool,
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
            has_html_content: false,
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
                // Check if thread has HTML content
                self.has_html_content = detail
                    .messages
                    .iter()
                    .any(|m| m.body_html.as_ref().is_some_and(|h| !h.is_empty()));
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
                    .icon(Icon::new(IconName::ArrowLeft).with_size(ComponentSize::Small))
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
                            .icon(Icon::new(IconName::ArrowLeft).with_size(ComponentSize::Small))
                            .ghost()
                            .label("Reply"),
                    )
                    .child(
                        Button::new("archive-button")
                            .icon(Icon::new(IconName::Folder).with_size(ComponentSize::Small))
                            .ghost(),
                    )
                    .child(
                        Button::new("delete-button")
                            .icon(Icon::new(IconName::Delete).with_size(ComponentSize::Small))
                            .ghost(),
                    ),
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
                    .child("Loading thread..."),
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

    fn render_messages(&self, messages: &[Message], cx: &mut Context<Self>) -> impl IntoElement {
        let messages = messages.to_vec();
        let theme = cx.theme();

        div()
            .id("thread-messages")
            .flex()
            .flex_col()
            .flex_1()
            .bg(theme.background)
            .overflow_y_scrollbar()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .p_4()
                    .gap_3()
                    .children(messages.into_iter().map(MessageCard::new)),
            )
    }
}

impl Render for ThreadView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // When has_html_content is true, the app renders the WebView below us,
        // so we only render the header (no size_full, just natural height)
        if self.has_html_content {
            return self.render_header(cx).into_any_element();
        }

        // Build header first
        let header: AnyElement = self.render_header(cx).into_any_element();

        // Build content based on state
        let content: AnyElement = if self.is_loading {
            self.render_loading(cx).into_any_element()
        } else if let Some(ref error) = self.error_message.clone() {
            self.render_error(error, cx).into_any_element()
        } else if let Some(ref detail) = self.detail.clone() {
            // Plain text only - render native message cards
            self.render_messages(&detail.messages, cx).into_any_element()
        } else {
            self.render_loading(cx).into_any_element()
        };

        // Get theme after mutable borrows are done
        let bg = cx.theme().background;

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(bg)
            .child(header)
            .child(content)
            .into_any_element()
    }
}
