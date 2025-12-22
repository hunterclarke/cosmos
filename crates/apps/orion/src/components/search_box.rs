//! Search box component with debounced input

use gpui::prelude::*;
use gpui::*;
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::{ActiveTheme, Icon, IconName, Sizable};
use std::time::Duration;

/// Events emitted by the SearchBox
pub enum SearchBoxEvent {
    /// Query changed (debounced)
    QueryChanged(String),
    /// Enter pressed - submit search
    Submitted(String),
    /// Search cleared (reserved for future use)
    #[allow(dead_code)]
    Cleared,
    /// Escape pressed - cancel search
    Cancelled,
}

impl EventEmitter<SearchBoxEvent> for SearchBox {}

/// Search box component with debounced input
pub struct SearchBox {
    input_state: Entity<InputState>,
    focus_handle: FocusHandle,
    debounce_task: Option<Task<()>>,
    last_emitted_query: String,
    #[allow(dead_code)]
    input_subscription: Subscription,
}

impl SearchBox {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input_state = cx.new(|cx| InputState::new(window, cx).placeholder("Search mail..."));

        // Subscribe to input events
        let input_subscription = cx.subscribe(&input_state, Self::on_input_event);

        Self {
            input_state,
            focus_handle: cx.focus_handle(),
            debounce_task: None,
            last_emitted_query: String::new(),
            input_subscription,
        }
    }

    fn on_input_event(
        &mut self,
        _: Entity<InputState>,
        event: &InputEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            InputEvent::Change => {
                self.on_input_change(cx);
            }
            InputEvent::PressEnter { .. } => {
                let query = self.query(cx);
                cx.emit(SearchBoxEvent::Submitted(query));
            }
            _ => {}
        }
    }

    /// Get the current query text
    pub fn query(&self, cx: &App) -> String {
        self.input_state.read(cx).text().to_string()
    }

    /// Set the query text (reserved for future use)
    #[allow(dead_code)]
    pub fn set_query(&mut self, query: &str, window: &mut Window, cx: &mut Context<Self>) {
        let query_owned = query.to_string();
        self.input_state.update(cx, |state, cx| {
            state.set_value(query_owned, window, cx);
        });
        self.last_emitted_query = query.to_string();
    }

    /// Clear the search box (reserved for future use)
    #[allow(dead_code)]
    pub fn clear(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.input_state.update(cx, |state, cx| {
            state.set_value("", window, cx);
        });
        self.last_emitted_query.clear();
        self.debounce_task = None;
        cx.emit(SearchBoxEvent::Cleared);
    }

    /// Focus the search box
    pub fn focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.input_state.update(cx, |state, cx| {
            state.focus(window, cx);
        });
    }

    /// Handle input changes with debouncing
    fn on_input_change(&mut self, cx: &mut Context<Self>) {
        let query = self.query(cx);

        // Skip if query hasn't changed from last emitted
        if query == self.last_emitted_query {
            return;
        }

        // Cancel existing debounce task
        self.debounce_task = None;

        // Start new debounce task
        let query_clone = query.clone();
        self.debounce_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(150))
                .await;

            let _ = cx.update(|cx| {
                let _ = this.update(cx, |view, cx| {
                    view.last_emitted_query = query_clone.clone();
                    cx.emit(SearchBoxEvent::QueryChanged(query_clone));
                });
            });
        }));
    }
}

impl Render for SearchBox {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let query = self.query(cx);
        let has_text = !query.is_empty();

        div()
            .track_focus(&self.focus_handle)
            .key_context("SearchBox")
            .on_action(cx.listener(Self::handle_escape))
            .flex()
            .items_center()
            .w(px(280.))
            .gap_1()
            // Search icon
            .child(
                Icon::new(IconName::Search)
                    .small()
                    .text_color(theme.muted_foreground),
            )
            // Input field - using gpui-component Input
            .child(
                Input::new(&self.input_state)
                    .appearance(false)
                    .cleanable(true)
                    .w_full(),
            )
            // Keyboard shortcut hint (when empty)
            .when(!has_text, |el| {
                el.child(
                    div().px_1().py_px().rounded(px(4.)).bg(theme.border).child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child("/"),
                    ),
                )
            })
    }
}

// Actions for keyboard handling
actions!(search_box, [Escape]);

impl SearchBox {
    fn handle_escape(&mut self, _: &Escape, _window: &mut Window, cx: &mut Context<Self>) {
        cx.emit(SearchBoxEvent::Cancelled);
    }
}
