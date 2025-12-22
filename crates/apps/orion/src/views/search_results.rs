//! Search results view - displays search results with virtual scrolling

use gpui::prelude::*;
use gpui::*;
use gpui_component::scroll::Scrollbar;
use gpui_component::spinner::Spinner;
use gpui_component::{v_virtual_list, ActiveTheme, Sizable, Size as ComponentSize, VirtualListScrollHandle};
use log::{error, info};
use mail::{search_threads, MailStore, SearchIndex, SearchResult, parse_query};
use std::rc::Rc;
use std::sync::Arc;

use crate::app::OrionApp;
use crate::components::SearchResultItem;

/// Height of each search result item
const RESULT_ITEM_HEIGHT: f32 = 100.0;

/// View for displaying search results
pub struct SearchResultsView {
    store: Arc<dyn MailStore>,
    index: Arc<SearchIndex>,
    query: String,
    results: Vec<SearchResult>,
    selected_index: usize,
    is_searching: bool,
    error_message: Option<String>,
    app: Option<Entity<OrionApp>>,
    scroll_handle: VirtualListScrollHandle,
    item_sizes: Rc<Vec<Size<Pixels>>>,
    focus_handle: FocusHandle,
}

impl SearchResultsView {
    pub fn new(
        store: Arc<dyn MailStore>,
        index: Arc<SearchIndex>,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            store,
            index,
            query: String::new(),
            results: Vec::new(),
            selected_index: 0,
            is_searching: false,
            error_message: None,
            app: None,
            scroll_handle: VirtualListScrollHandle::new(),
            item_sizes: Rc::new(Vec::new()),
            focus_handle: cx.focus_handle(),
        }
    }

    pub fn set_app(&mut self, app: Entity<OrionApp>) {
        self.app = Some(app);
    }

    /// Focus the search results view (preserves current selection)
    pub fn focus(&self, window: &mut Window, _cx: &mut Context<Self>) {
        self.focus_handle.focus(window);
    }

    /// Execute search with the given query
    pub fn search(&mut self, query: String, cx: &mut Context<Self>) {
        self.query = query.clone();
        self.is_searching = true;
        self.error_message = None;
        self.selected_index = 0;
        cx.notify();

        // Run search on background thread
        let store = self.store.clone();
        let index = self.index.clone();
        let background = cx.background_executor().clone();

        cx.spawn(async move |this, cx| {
            let result = background
                .spawn(async move { search_threads(&index, store.as_ref(), &query, 100) })
                .await;

            let _ = cx.update(|cx| {
                let _ = this.update(cx, |view, cx| {
                    view.is_searching = false;
                    match result {
                        Ok(results) => {
                            info!("Search returned {} results", results.len());
                            view.item_sizes = Rc::new(
                                results
                                    .iter()
                                    .map(|_| size(px(10000.), px(RESULT_ITEM_HEIGHT)))
                                    .collect(),
                            );
                            view.results = results;
                        }
                        Err(e) => {
                            error!("Search failed: {}", e);
                            view.error_message = Some(format!("Search failed: {}", e));
                            view.results.clear();
                        }
                    }
                    cx.notify();
                });
            });
        })
        .detach();
    }

    /// Move selection up
    pub fn select_prev(&mut self, cx: &mut Context<Self>) {
        if self.results.is_empty() {
            return;
        }
        // Clamp to valid range (results may have changed)
        let max_index = self.results.len().saturating_sub(1);
        self.selected_index = self.selected_index.min(max_index);
        if self.selected_index > 0 {
            self.selected_index -= 1;
            cx.notify();
        }
    }

    /// Move selection down
    pub fn select_next(&mut self, cx: &mut Context<Self>) {
        if self.results.is_empty() {
            return;
        }
        // Clamp to valid range (results may have changed)
        let max_index = self.results.len().saturating_sub(1);
        self.selected_index = self.selected_index.min(max_index);
        if self.selected_index < max_index {
            self.selected_index += 1;
            cx.notify();
        }
    }

    /// Open the selected result
    pub fn open_selected(&mut self, cx: &mut Context<Self>) {
        if let Some(result) = self.results.get(self.selected_index) {
            let thread_id = result.thread_id.clone();
            if let Some(app) = &self.app {
                app.update(cx, |app, cx| {
                    app.show_thread(thread_id, cx);
                });
            }
        }
    }

    /// Parse query terms for highlighting
    fn query_terms(&self) -> Vec<String> {
        let parsed = parse_query(&self.query);
        parsed.terms
    }

    fn render_header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let result_count = self.results.len();

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
                    .child(format!("{} results for \"{}\"", result_count, self.query)),
            )
    }

    fn render_empty(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();

        div()
            .flex_1()
            .flex()
            .justify_center()
            .items_center()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .child("No results found"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child("Try different search terms"),
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
                    .child("Searching..."),
            )
    }

    fn render_results(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let query_terms = self.query_terms();

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
                    "search-results",
                    self.item_sizes.clone(),
                    move |view, visible_range, _window, cx| {
                        let terms = query_terms.clone();
                        visible_range
                            .map(|ix| {
                                let result = view.results[ix].clone();
                                let is_selected = ix == view.selected_index;

                                div()
                                    .id(ElementId::Name(format!("result-{}", ix).into()))
                                    .h(px(RESULT_ITEM_HEIGHT))
                                    .w_full()
                                    .cursor_pointer()
                                    .on_click(cx.listener(move |view, _, _, cx| {
                                        view.selected_index = ix;
                                        view.open_selected(cx);
                                    }))
                                    .child(SearchResultItem::new(result, is_selected, terms.clone()))
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

impl Render for SearchResultsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();

        div()
            .track_focus(&self.focus_handle)
            .key_context("SearchResultsView")
            .on_action(cx.listener(Self::handle_select_prev))
            .on_action(cx.listener(Self::handle_select_next))
            .on_action(cx.listener(Self::handle_open_selected))
            .flex()
            .flex_col()
            .size_full()
            .bg(theme.background)
            .child(self.render_header(cx))
            .child(if self.is_searching {
                self.render_loading(cx).into_any_element()
            } else if self.results.is_empty() {
                self.render_empty(cx).into_any_element()
            } else {
                self.render_results(cx).into_any_element()
            })
    }
}

// Actions for keyboard navigation
actions!(search_results, [SelectPrev, SelectNext, OpenSelected]);

impl SearchResultsView {
    fn handle_select_prev(&mut self, _: &SelectPrev, _window: &mut Window, cx: &mut Context<Self>) {
        self.select_prev(cx);
    }

    fn handle_select_next(&mut self, _: &SelectNext, _window: &mut Window, cx: &mut Context<Self>) {
        self.select_next(cx);
    }

    fn handle_open_selected(&mut self, _: &OpenSelected, _window: &mut Window, cx: &mut Context<Self>) {
        self.open_selected(cx);
    }
}
