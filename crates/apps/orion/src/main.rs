//! Orion - A read-only Gmail inbox viewer
//!
//! This is the main entry point for the Orion mail application.

use gpui::prelude::*;
use gpui::{px, size, Application, WindowOptions};
use gpui_component::{Theme, ThemeMode, TitleBar};
use mail::GmailCredentials;

mod app;
mod components;
mod views;

use app::OrionApp;

fn main() {
    // Bootstrap config directory
    if let Err(e) = config::init() {
        eprintln!("Failed to initialize config directory: {}", e);
    }

    Application::new().run(|cx| {
        // Initialize gpui-component and set dark mode
        gpui_component::init(cx);
        Theme::change(ThemeMode::Dark, None, cx);

        let window_options = WindowOptions {
            window_bounds: Some(gpui::WindowBounds::Windowed(gpui::Bounds {
                origin: gpui::Point::default(),
                size: size(px(1200.), px(800.)),
            })),
            titlebar: Some(TitleBar::title_bar_options()),
            ..Default::default()
        };

        cx.open_window(window_options, |_window, cx| {
            cx.new(|cx| {
                let mut app = OrionApp::new(cx);

                // Load Gmail credentials from config file or environment
                match GmailCredentials::load() {
                    Ok(creds) => {
                        if let Err(e) = app.init_gmail(creds.client_id, creds.client_secret) {
                            eprintln!("Failed to initialize Gmail client: {}", e);
                        } else {
                            println!("Gmail client initialized successfully");
                        }
                    }
                    Err(e) => {
                        eprintln!("Gmail credentials not found: {}", e);
                        if let Some(path) = GmailCredentials::default_credentials_path() {
                            eprintln!(
                                "\nTo configure Gmail access, either:\n\
                                 1. Place your Google OAuth credentials at:\n   {}\n\
                                 2. Or set environment variables:\n   \
                                    GMAIL_CLIENT_ID and GMAIL_CLIENT_SECRET",
                                path.display()
                            );
                        }
                    }
                }

                // Wire up navigation by passing app entity to child views
                let app_handle = cx.entity().clone();
                app.wire_navigation(app_handle, cx);

                // Load initial threads
                if let Some(inbox) = &app.inbox_view {
                    inbox.update(cx, |view, cx| view.load_threads(cx));
                }

                app
            })
        })
        .expect("Failed to open window");

        println!("App started successfully");
    });
}
