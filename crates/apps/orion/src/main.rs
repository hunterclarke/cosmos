//! Orion - A read-only Gmail inbox viewer
//!
//! This is the main entry point for the Orion mail application.

use std::time::Instant;

use gpui::prelude::*;
use gpui::{px, size, Application, KeyBinding, WindowOptions};
use gpui_component::{Root, Theme, ThemeMode, TitleBar};
use log::{debug, error, info, warn};
use mail::GmailCredentials;

mod app;
mod assets;
mod components;
mod templates;
mod views;

use app::{FocusSearch, OrionApp};
use assets::OrionAssets;
use components::search_box;
use views::search_results;

fn main() {
    let startup_start = Instant::now();

    // Initialize logging
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_millis()
        .init();

    debug!("[BOOT] Logger initialized: {:?}", startup_start.elapsed());

    // Bootstrap config directory
    if let Err(e) = config::init() {
        error!("Failed to initialize config directory: {}", e);
    }
    debug!("[BOOT] Config init: {:?}", startup_start.elapsed());

    Application::new()
        .with_assets(OrionAssets)
        .run(move |cx| {
        debug!("[BOOT] GPUI Application created: {:?}", startup_start.elapsed());

        // Initialize gpui-component and set dark mode
        gpui_component::init(cx);
        debug!("[BOOT] gpui-component init: {:?}", startup_start.elapsed());
        Theme::change(ThemeMode::Dark, None, cx);
        debug!("[BOOT] Theme set: {:?}", startup_start.elapsed());

        // Register global keyboard shortcuts
        cx.bind_keys([
            // Focus search: / or Cmd+K
            KeyBinding::new("/", FocusSearch, Some("OrionApp")),
            KeyBinding::new("cmd-k", FocusSearch, Some("OrionApp")),
            // Search box: Escape to cancel
            KeyBinding::new("escape", search_box::Escape, Some("SearchBox")),
            // Search results navigation
            KeyBinding::new("k", search_results::SelectPrev, Some("SearchResultsView")),
            KeyBinding::new("up", search_results::SelectPrev, Some("SearchResultsView")),
            KeyBinding::new("j", search_results::SelectNext, Some("SearchResultsView")),
            KeyBinding::new("down", search_results::SelectNext, Some("SearchResultsView")),
            KeyBinding::new("enter", search_results::OpenSelected, Some("SearchResultsView")),
        ]);

        let window_options = WindowOptions {
            window_bounds: Some(gpui::WindowBounds::Windowed(gpui::Bounds {
                origin: gpui::Point::default(),
                size: size(px(1200.), px(800.)),
            })),
            titlebar: Some(TitleBar::title_bar_options()),
            ..Default::default()
        };

        debug!("[BOOT] Window options prepared: {:?}", startup_start.elapsed());

        cx.open_window(window_options, |window, cx| {
            debug!("[BOOT] Window opened: {:?}", startup_start.elapsed());

            // Create OrionApp as a child entity first
            let app_entity = cx.new(|cx| {
                let mut app = OrionApp::new(cx);
                debug!("[BOOT] OrionApp::new() complete: {:?}", startup_start.elapsed());

                // Load Gmail credentials from config file or environment
                match GmailCredentials::load() {
                    Ok(creds) => {
                        if let Err(e) = app.init_gmail(creds.client_id, creds.client_secret) {
                            error!("Failed to initialize Gmail client: {}", e);
                        } else {
                            info!("Gmail client initialized successfully");
                        }
                    }
                    Err(e) => {
                        warn!("Gmail credentials not found: {}", e);
                        if let Some(path) = GmailCredentials::default_credentials_path() {
                            warn!(
                                "To configure Gmail access, either:\n\
                                 1. Place your Google OAuth credentials at: {}\n\
                                 2. Or set environment variables: GMAIL_CLIENT_ID and GMAIL_CLIENT_SECRET",
                                path.display()
                            );
                        }
                    }
                }

                debug!("[BOOT] Gmail init complete: {:?}", startup_start.elapsed());
                app
            });

            // Wire up navigation
            let app_handle = app_entity.clone();
            app_entity.update(cx, |app, cx| {
                app.wire_navigation(app_handle, cx);
                debug!("[BOOT] Navigation wired: {:?}", startup_start.elapsed());

                // Start loading persistent storage in background
                // UI will show skeleton loading state until this completes
                app.load_persistent_storage(cx);
                debug!("[BOOT] Background storage load started: {:?}", startup_start.elapsed());
            });

            // Wrap in gpui-component Root (required for Input component)
            let root = cx.new(|cx| Root::new(app_entity, window, cx));
            debug!("[BOOT] Root component created: {:?}", startup_start.elapsed());
            root
        })
        .expect("Failed to open window");

        debug!("[BOOT] STARTUP COMPLETE: {:?}", startup_start.elapsed());
    });
}
