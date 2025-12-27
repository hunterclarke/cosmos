//! Account item component for the sidebar account section

use gpui::prelude::*;
use gpui::*;
use gpui_component::spinner::Spinner;
use gpui_component::{ActiveTheme, Icon, IconName, Sizable, Size};
use mail::Account;

/// A single account row in the sidebar
#[derive(IntoElement)]
pub struct AccountItem {
    account: Account,
    is_selected: bool,
    is_syncing: bool,
    unread_count: u32,
}

impl AccountItem {
    pub fn new(account: Account, is_selected: bool) -> Self {
        Self {
            account,
            is_selected,
            is_syncing: false,
            unread_count: 0,
        }
    }

    /// Set whether this account is currently syncing
    pub fn syncing(mut self, is_syncing: bool) -> Self {
        self.is_syncing = is_syncing;
        self
    }

    /// Set the unread count for this account
    #[allow(dead_code)]
    pub fn unread(mut self, count: u32) -> Self {
        self.unread_count = count;
        self
    }

    /// Get the first letter of the email for the avatar
    fn avatar_letter(&self) -> String {
        self.account
            .email
            .chars()
            .next()
            .map(|c| c.to_uppercase().to_string())
            .unwrap_or_else(|| "?".to_string())
    }

    /// Get display name (email address, truncated if needed)
    fn display_name(&self) -> &str {
        &self.account.email
    }
}

impl RenderOnce for AccountItem {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();

        // Use list_active colors for selected state
        let bg_color = if self.is_selected {
            theme.list_active
        } else {
            theme.transparent
        };

        let text_color = if self.is_selected {
            theme.foreground
        } else {
            theme.muted_foreground
        };

        let border_color = if self.is_selected {
            theme.list_active_border
        } else {
            theme.transparent
        };

        let avatar_letter = self.avatar_letter();
        let display_name = self.display_name().to_string();

        // Parse avatar color from the account (stored as HSL string like "hsl(200, 70%, 50%)")
        // For now, use a default color - can enhance later to parse the stored color
        let avatar_bg = theme.primary;
        let avatar_fg = theme.primary_foreground;

        div()
            .w_full()
            .px_3()
            .py_1p5()
            .my_px()
            .rounded_md()
            .bg(bg_color)
            .border_l_2()
            .border_color(border_color)
            .cursor_pointer()
            .hover(|style| style.bg(theme.list_hover))
            .flex()
            .justify_between()
            .items_center()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    // Avatar circle
                    .child(
                        div()
                            .size_5()
                            .rounded_full()
                            .bg(avatar_bg)
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_xs()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(avatar_fg)
                            .child(avatar_letter),
                    )
                    // Email address
                    .child(
                        div()
                            .text_sm()
                            .text_color(text_color)
                            .text_ellipsis()
                            .overflow_hidden()
                            .max_w(px(140.))
                            .font_weight(if self.is_selected {
                                FontWeight::MEDIUM
                            } else {
                                FontWeight::NORMAL
                            })
                            .child(display_name),
                    ),
            )
            // Right side: sync indicator or unread count
            .when(self.is_syncing, |el| {
                el.child(Spinner::new().with_size(Size::XSmall))
            })
            .when(!self.is_syncing && self.unread_count > 0, |el| {
                el.child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(format!("{}", self.unread_count)),
                )
            })
    }
}

/// "All Accounts" unified view item
#[derive(IntoElement)]
pub struct AllAccountsItem {
    is_selected: bool,
    total_unread: u32,
}

impl AllAccountsItem {
    pub fn new(is_selected: bool) -> Self {
        Self {
            is_selected,
            total_unread: 0,
        }
    }

    /// Set the total unread count across all accounts
    #[allow(dead_code)]
    pub fn unread(mut self, count: u32) -> Self {
        self.total_unread = count;
        self
    }
}

impl RenderOnce for AllAccountsItem {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();

        let bg_color = if self.is_selected {
            theme.list_active
        } else {
            theme.transparent
        };

        let text_color = if self.is_selected {
            theme.foreground
        } else {
            theme.muted_foreground
        };

        let border_color = if self.is_selected {
            theme.list_active_border
        } else {
            theme.transparent
        };

        div()
            .w_full()
            .px_3()
            .py_1p5()
            .my_px()
            .rounded_md()
            .bg(bg_color)
            .border_l_2()
            .border_color(border_color)
            .cursor_pointer()
            .hover(|style| style.bg(theme.list_hover))
            .flex()
            .justify_between()
            .items_center()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    // Multi-account icon (using Inbox as fallback)
                    .child(
                        Icon::new(IconName::Inbox)
                            .with_size(Size::Small)
                            .text_color(text_color),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(text_color)
                            .font_weight(if self.is_selected {
                                FontWeight::MEDIUM
                            } else {
                                FontWeight::NORMAL
                            })
                            .child("All Accounts"),
                    ),
            )
            .when(self.total_unread > 0, |el| {
                el.child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(format!("{}", self.total_unread)),
                )
            })
    }
}
