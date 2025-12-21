//! HTML templates for WebView rendering
//!
//! This module consolidates all HTML generation for the WebView,
//! ensuring consistent theming and structure across the application.

use gpui_component::theme::Theme;
use log::debug;
use mail::Message;

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

/// Simple HTML escape for user-generated content
pub fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// Theme colors extracted for CSS usage
struct ThemeColors {
    background: String,
    foreground: String,
    secondary: String,
    border: String,
    muted_foreground: String,
    link: String,
    danger: String,
    danger_foreground: String,
}

impl ThemeColors {
    fn from_theme(theme: &Theme) -> Self {
        Self {
            background: hsla_to_hex(theme.background),
            foreground: hsla_to_hex(theme.foreground),
            secondary: hsla_to_hex(theme.secondary),
            border: hsla_to_hex(theme.border),
            muted_foreground: hsla_to_hex(theme.muted_foreground),
            link: hsla_to_hex(theme.link),
            danger: hsla_to_hex(theme.danger),
            danger_foreground: hsla_to_hex(theme.danger_foreground),
        }
    }
}

/// Generate base CSS styles for the WebView
fn base_styles(colors: &ThemeColors) -> String {
    format!(
        r#"* {{ box-sizing: border-box; margin: 0; padding: 0; }}
html, body {{
    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Oxygen, Ubuntu, sans-serif;
    background: {bg} !important;
    color: {fg};
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
    background: {bg};
}}
::-webkit-scrollbar-thumb {{
    background: {border};
    border-radius: 4px;
}}
::-webkit-scrollbar-thumb:hover {{
    background: {muted};
}}"#,
        bg = colors.background,
        fg = colors.foreground,
        border = colors.border,
        muted = colors.muted_foreground,
    )
}

/// Generate CSS styles for message cards
fn message_styles(colors: &ThemeColors) -> String {
    format!(
        r#".message {{
    background: {card_bg};
    border: 1px solid {border};
    border-radius: 8px;
    margin-bottom: 12px;
    overflow: hidden;
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
    border-bottom: 1px solid {border};
}}
.sender {{ font-weight: 600; color: {fg}; }}
.email {{ font-size: 12px; color: {muted}; margin-top: 2px; }}
.date {{ font-size: 12px; color: {muted}; }}
.recipients {{ font-size: 12px; color: {muted}; margin-bottom: 12px; }}
.body {{
    color: {fg};
    overflow: hidden;
    border-radius: 0 0 6px 6px;
}}
.body img {{ max-width: 100%; height: auto; }}
.body a {{ color: {link}; }}
.body > * {{ border-radius: inherit; overflow: hidden; }}
.body blockquote {{
    border-left: 3px solid {border};
    padding-left: 12px;
    margin: 8px 0;
    color: {muted};
}}
/* Plain text body styling - matches header padding with reduced font size */
.body-text {{
    padding: 16px;
    font-size: 13px;
    line-height: 1.6;
}}"#,
        card_bg = colors.secondary,
        border = colors.border,
        fg = colors.foreground,
        muted = colors.muted_foreground,
        link = colors.link,
    )
}

/// Generate CSS styles for error display
fn error_styles(colors: &ThemeColors) -> String {
    format!(
        r#".error {{
    background: {danger_bg};
    color: {danger_fg};
    padding: 16px;
    border-radius: 8px;
    font-size: 14px;
}}"#,
        danger_bg = colors.danger,
        danger_fg = colors.danger_foreground,
    )
}

/// Generate HTML for a single message
fn render_message(message: &Message) -> String {
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

    // Determine if we have HTML body or need to use plain text
    let has_html = message
        .body_html
        .as_ref()
        .is_some_and(|h| !h.is_empty());

    let html_len = message.body_html.as_ref().map(|h| h.len()).unwrap_or(0);
    let text_len = message.body_text.as_ref().map(|t| t.len()).unwrap_or(0);
    debug!(
        "Message from {}: has_html={}, html_len={}, text_len={}, preview_len={}",
        sender_email,
        has_html,
        html_len,
        text_len,
        message.body_preview.len()
    );

    let body_content = if has_html {
        message.body_html.as_ref().unwrap().clone()
    } else {
        // Escape HTML in plain text and convert newlines
        let text = message
            .body_text
            .as_ref()
            .filter(|t| !t.is_empty())
            .unwrap_or(&message.body_preview);
        html_escape(text).replace('\n', "<br>")
    };

    // Use different class for plain text vs HTML bodies
    let body_class = if has_html { "body" } else { "body body-text" };

    let mut html = format!(
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
    );

    if !recipients_str.is_empty() {
        html.push_str(&format!(
            r#"<div class="recipients">To: {}</div>
"#,
            html_escape(&recipients_str)
        ));
    }

    html.push_str(&format!(
        r#"</div>
<div class="{}">{}</div>
</div>
"#,
        body_class, body_content
    ));

    html
}

/// Generate combined HTML for all messages in a thread with theme colors
///
/// This is called by OrionApp before navigation to generate HTML content
/// that will be loaded into the shared WebView.
pub fn thread_html(messages: &[Message], theme: &Theme) -> String {
    let colors = ThemeColors::from_theme(theme);

    let mut html = format!(
        r#"<!DOCTYPE html>
<html>
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<style>
{}
{}
</style>
</head>
<body>
"#,
        base_styles(&colors),
        message_styles(&colors),
    );

    for message in messages {
        html.push_str(&render_message(message));
    }

    html.push_str("</body></html>");
    html
}

/// Generate an error HTML page for WebView display
pub fn error_html(message: &str, theme: &Theme) -> String {
    let colors = ThemeColors::from_theme(theme);

    format!(
        r#"<!DOCTYPE html>
<html>
<head>
<meta charset="UTF-8">
<style>
{}
{}
</style>
</head>
<body>
<div class="error">{}</div>
</body>
</html>"#,
        base_styles(&colors),
        error_styles(&colors),
        html_escape(message)
    )
}
