//! Custom assets for Orion
//!
//! Extends gpui-component with additional Lucide icons needed for mail actions.

use gpui::{AssetSource, SharedString};
use rust_embed::RustEmbed;

/// Custom icons embedded from the assets/icons directory
#[derive(RustEmbed)]
#[folder = "assets/icons"]
#[include = "*.svg"]
struct CustomIcons;

/// Combined asset source that checks custom icons first, then falls back to gpui-component-assets
pub struct OrionAssets;

impl AssetSource for OrionAssets {
    fn load(&self, path: &str) -> gpui::Result<Option<std::borrow::Cow<'static, [u8]>>> {
        // Check custom icons first (they're in icons/ subdirectory format)
        if path.starts_with("icons/") {
            let icon_name = path.strip_prefix("icons/").unwrap_or(path);
            if let Some(file) = CustomIcons::get(icon_name) {
                return Ok(Some(file.data));
            }
        }

        // Fall back to gpui-component-assets
        gpui_component_assets::Assets.load(path)
    }

    fn list(&self, path: &str) -> gpui::Result<Vec<SharedString>> {
        let mut results = Vec::new();

        // Add custom icons
        if path.is_empty() || path == "icons" || path == "icons/" {
            for file in CustomIcons::iter() {
                results.push(format!("icons/{}", file).into());
            }
        }

        // Add gpui-component-assets
        results.extend(gpui_component_assets::Assets.list(path)?);

        Ok(results)
    }
}

/// Custom icon names for Orion-specific icons that implement IconNamed
pub mod icons {
    use gpui::SharedString;
    use gpui_component::IconNamed;

    /// Archive icon (box with down arrow)
    #[derive(Clone, Copy)]
    pub struct Archive;

    impl IconNamed for Archive {
        fn path(self) -> SharedString {
            "icons/archive.svg".into()
        }
    }

    /// Mail open icon (for read/unread status)
    #[derive(Clone, Copy)]
    pub struct MailOpen;

    impl IconNamed for MailOpen {
        fn path(self) -> SharedString {
            "icons/mail-open.svg".into()
        }
    }

    /// Refresh icon (for sync)
    #[derive(Clone, Copy)]
    pub struct RefreshCw;

    impl IconNamed for RefreshCw {
        fn path(self) -> SharedString {
            "icons/refresh-cw.svg".into()
        }
    }
}
