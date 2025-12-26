//! Search result item component - displays a single search result
//! Uses Gmail-style single-line layout: Sender | Subject - preview | Date

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
        let text_weight = if is_unread {
            FontWeight::SEMIBOLD
        } else {
            FontWeight::NORMAL
        };

        // Render highlighted text elements
        let subject_styled = self.render_highlighted_text(&subject);
        let snippet_text = if snippet.is_empty() {
            String::new()
        } else {
            format!("- {}", snippet)
        };
        let snippet_styled = self.render_highlighted_text(&snippet_text);

        div()
            .w_full()
            .h_full()
            .bg(bg_color)
            .border_b_1()
            .border_color(theme.border)
            .cursor_pointer()
            .hover(|style| style.bg(theme.list_hover))
            .child(
                // Single row layout
                div()
                    .px_3()
                    .h_full()
                    .flex()
                    .items_center()
                    .gap_2()
                    // Unread indicator dot
                    .child(
                        div()
                            .w(px(6.))
                            .h(px(6.))
                            .rounded_full()
                            .flex_shrink_0()
                            .when(is_unread, |el| el.bg(theme.primary)),
                    )
                    // Column 1: Sender with message count
                    .child(
                        div()
                            .w(px(180.))
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .gap_1()
                            .overflow_hidden()
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(text_weight)
                                    .text_color(theme.foreground)
                                    .text_ellipsis()
                                    .child(sender_display),
                            )
                            .when(message_count > 1, |el| {
                                el.child(
                                    div()
                                        .text_xs()
                                        .text_color(theme.muted_foreground)
                                        .flex_shrink_0()
                                        .child(format!("({})", message_count)),
                                )
                            }),
                    )
                    // Column 2: Subject - preview with highlighting (fills remaining space)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .items_center()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(text_weight)
                                    .text_color(theme.foreground)
                                    .flex_shrink_0()
                                    .child(subject_styled),
                            )
                            .when(!snippet.is_empty(), |el| {
                                el.child(
                                    div()
                                        .text_sm()
                                        .text_color(theme.muted_foreground)
                                        .ml_1()
                                        .text_ellipsis()
                                        .child(snippet_styled),
                                )
                            }),
                    )
                    // Column 3: Date (right-aligned)
                    .child(
                        div()
                            .flex_shrink_0()
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
                    ),
            )
    }
}
