//! Account model representing a Gmail account

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A registered Gmail account
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Account {
    /// Unique integer identifier (database primary key)
    pub id: i64,
    /// Email address (unique)
    pub email: String,
    /// Display name (can be customized by user)
    pub display_name: Option<String>,
    /// Avatar color (HSL string for consistent coloring in UI)
    pub avatar_color: String,
    /// Whether this is the primary/default account
    pub is_primary: bool,
    /// When the account was added
    pub added_at: DateTime<Utc>,
    /// OAuth token data (JSON-serialized)
    pub token_data: Option<String>,
}

impl Account {
    /// Create a new account (id will be assigned by database)
    pub fn new(email: impl Into<String>) -> Self {
        let email = email.into();
        let avatar_color = Self::generate_color(&email);
        Self {
            id: 0, // Will be set by database
            email,
            display_name: None,
            avatar_color,
            is_primary: false,
            added_at: Utc::now(),
            token_data: None,
        }
    }

    /// Create an account with a known ID (loaded from database)
    pub fn with_id(id: i64, email: impl Into<String>) -> Self {
        let email = email.into();
        let avatar_color = Self::generate_color(&email);
        Self {
            id,
            email,
            display_name: None,
            avatar_color,
            is_primary: false,
            added_at: Utc::now(),
            token_data: None,
        }
    }

    /// Set the OAuth token data (JSON-serialized)
    pub fn with_token_data(mut self, token_data: impl Into<String>) -> Self {
        self.token_data = Some(token_data.into());
        self
    }

    /// Set as primary account
    pub fn with_primary(mut self, is_primary: bool) -> Self {
        self.is_primary = is_primary;
        self
    }

    /// Set display name
    pub fn with_display_name(mut self, name: impl Into<String>) -> Self {
        self.display_name = Some(name.into());
        self
    }

    /// Generate a consistent color based on email address
    fn generate_color(email: &str) -> String {
        // Simple hash-based color generation
        let hash: u32 = email
            .bytes()
            .fold(0u32, |acc, b| acc.wrapping_add(b as u32).wrapping_mul(31));

        // Generate HSL with fixed saturation and lightness for readability
        let hue = hash % 360;
        format!("hsl({}, 65%, 45%)", hue)
    }

    /// Get the first letter of the email for avatar display
    pub fn avatar_letter(&self) -> String {
        self.email
            .chars()
            .next()
            .map(|c| c.to_uppercase().to_string())
            .unwrap_or_else(|| "?".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_account_new() {
        let account = Account::new("test@example.com");
        assert_eq!(account.id, 0); // Not yet assigned
        assert_eq!(account.email, "test@example.com");
        assert!(!account.is_primary);
        assert!(account.display_name.is_none());
    }

    #[test]
    fn test_account_with_id() {
        let account = Account::with_id(42, "test@example.com");
        assert_eq!(account.id, 42);
        assert_eq!(account.email, "test@example.com");
    }

    #[test]
    fn test_account_with_primary() {
        let account = Account::new("test@example.com").with_primary(true);
        assert!(account.is_primary);
    }

    #[test]
    fn test_avatar_letter() {
        let account = Account::new("test@example.com");
        assert_eq!(account.avatar_letter(), "T");
    }

    #[test]
    fn test_consistent_color() {
        let account1 = Account::new("test@example.com");
        let account2 = Account::new("test@example.com");
        assert_eq!(account1.avatar_color, account2.avatar_color);
    }
}
