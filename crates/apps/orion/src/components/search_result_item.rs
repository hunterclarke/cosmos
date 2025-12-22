//! Search result item component - displays a single search result

use gpui::prelude::*;
use gpui::*;
use gpui_component::ActiveTheme;
use mail::SearchResult;

/// Component for rendering a single search result
#[derive(IntoElement)]
pub struct SearchResultItem {
    result: SearchResult,
    is_selected: bool,
    query_terms: Vec<String>,
}

impl SearchResultItem {
    pub fn new(result: SearchResult, is_selected: bool, query_terms: Vec<String>) -> Self {
        Self {
            result,
            is_selected,
            query_terms,
        }
    }

    fn format_date(&self) -> String {
        use chrono::{Local, Utc};
        let local = self.result.last_message_at.with_timezone(&Local);
        let now = Utc::now().with_timezone(&Local);

        if local.date_naive() == now.date_naive() {
            local.format("%H:%M").to_string()
        } else if (now - local).num_days() < 7 {
            local.format("%a").to_string()
        } else {
            local.format("%b %d").to_string()
        }
    }

    /// Find all ranges in the text that match query terms (case-insensitive)
    fn find_highlight_ranges(&self, text: &str) -> Vec<std::ops::Range<usize>> {
        let mut ranges = Vec::new();
        let text_lower = text.to_lowercase();

        for term in &self.query_terms {
            let term_lower = term.to_lowercase();
            let mut start = 0;
            while let Some(pos) = text_lower[start..].find(&term_lower) {
                let abs_start = start + pos;
                let abs_end = abs_start + term.len();
                ranges.push(abs_start..abs_end);
                start = abs_end;
            }
        }

        // Sort and merge overlapping ranges
        ranges.sort_by_key(|r| r.start);
        let mut merged: Vec<std::ops::Range<usize>> = Vec::new();
        for range in ranges {
            if let Some(last) = merged.last_mut() {
                if range.start <= last.end {
                    last.end = last.end.max(range.end);
                    continue;
                }
            }
            merged.push(range);
        }
        merged
    }

    /// Render text with highlighted matching terms using StyledText
    fn render_highlighted_text(&self, text: &str) -> StyledText {
        let ranges = self.find_highlight_ranges(text);

        if ranges.is_empty() {
            return StyledText::new(text.to_string());
        }

        // Yellow highlight background for matches
        let highlight_style = HighlightStyle {
            background_color: Some(hsla(50. / 360., 0.9, 0.5, 0.4)),
            ..Default::default()
        };

        let highlights: Vec<_> = ranges
            .into_iter()
            .map(|range| (range, highlight_style))
            .collect();

        StyledText::new(text.to_string()).with_highlights(highlights)
    }
}

impl RenderOnce for SearchResultItem {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let is_unread = self.result.is_unread;

        let bg_color = if self.is_selected {
            theme.list_active
        } else {
            theme.list
        };

        let sender_display = self
            .result
            .sender_name
            .clone()
            .unwrap_or_else(|| self.result.sender_email.clone());

        let date_str = self.format_date();
        let message_count = self.result.message_count;
        let subject = self.result.subject.clone();
        let snippet = self.result.snippet.clone();

        // Text styling based on unread status
        let subject_weight = if is_unread {
            FontWeight::SEMIBOLD
        } else {
            FontWeight::NORMAL
        };

        // Render highlighted text elements
        let subject_styled = self.render_highlighted_text(&subject);
        let snippet_styled = self.render_highlighted_text(&snippet);

        div()
            .w_full()
            .flex()
            .flex_col()
            // Divider at top (full width)
            .child(div().w_full().h_px().bg(theme.border))
            // Content row with background
            .child(
                div()
                    .w_full()
                    .bg(bg_color)
                    .cursor_pointer()
                    .hover(|style| style.bg(theme.list_hover))
                    .overflow_hidden()
                    .child(
                        // Inner content with horizontal padding
                        div()
                            .px_3()
                            .py_4()
                            .flex()
                            .items_start()
                            .gap_2()
                            // Unread indicator dot
                            .child(
                                div()
                                    .pt(px(5.)) // Align with first line of text
                                    .child(
                                        div()
                                            .w(px(6.))
                                            .h(px(6.))
                                            .rounded_full()
                                            .when(is_unread, |el| el.bg(theme.primary))
                                            .flex_shrink_0(),
                                    ),
                            )
                            // Content: sender, subject, snippet
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .overflow_hidden()
                                    .min_w_0()
                                    .flex_1()
                                    // Sender
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_weight(subject_weight)
                                            .text_color(theme.foreground)
                                            .overflow_hidden()
                                            .whitespace_nowrap()
                                            .child(sender_display),
                                    )
                                    // Subject with highlighting
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_weight(subject_weight)
                                            .text_color(theme.foreground)
                                            .overflow_hidden()
                                            .whitespace_nowrap()
                                            .child(subject_styled),
                                    )
                                    // Snippet with highlighting
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(theme.muted_foreground)
                                            .overflow_hidden()
                                            .whitespace_nowrap()
                                            .child(snippet_styled),
                                    ),
                            )
                            // Date and message count
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .items_end()
                                    .gap_1()
                                    .flex_shrink_0()
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(if is_unread {
                                                theme.foreground
                                            } else {
                                                theme.muted_foreground
                                            })
                                            .font_weight(if is_unread {
                                                FontWeight::MEDIUM
                                            } else {
                                                FontWeight::NORMAL
                                            })
                                            .child(date_str),
                                    )
                                    .when(message_count > 1, |el| {
                                        el.child(
                                            div()
                                                .text_xs()
                                                .text_color(theme.muted_foreground)
                                                .child(format!("({})", message_count)),
                                        )
                                    }),
                            ),
                    ),
            )
    }
}
