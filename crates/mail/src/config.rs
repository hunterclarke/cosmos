//! Configuration loading for mail services
//!
//! Supports loading OAuth credentials from:
//! 1. JSON file (Google Cloud Console format)
//! 2. Environment variables (fallback)

use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// Credentials filename in the Cosmos config directory
const CREDENTIALS_FILE: &str = "google-credentials.json";

/// OAuth credentials for Gmail API access
#[derive(Debug, Clone)]
pub struct GmailCredentials {
    pub client_id: String,
    pub client_secret: String,
}

/// Google Cloud Console credential file format (installed app)
#[derive(Deserialize)]
struct GoogleCredentialFile {
    installed: Option<InstalledCredentials>,
    web: Option<InstalledCredentials>,
}

#[derive(Deserialize)]
struct InstalledCredentials {
    client_id: String,
    client_secret: String,
}

impl GmailCredentials {
    /// Load credentials from the default location (~/.config/cosmos/google-credentials.json)
    /// Falls back to environment variables if file not found.
    pub fn load() -> Result<Self> {
        // Try default config file first
        if config::config_exists(CREDENTIALS_FILE) {
            let creds: GoogleCredentialFile = config::load_json(CREDENTIALS_FILE)?;
            return Self::from_credential_file(creds);
        }

        // Fall back to environment variables
        Self::from_env()
    }

    /// Load credentials from a specific JSON file
    pub fn from_file(path: &Path) -> Result<Self> {
        let creds: GoogleCredentialFile = config::load_json_file(path)?;
        Self::from_credential_file(creds)
    }

    /// Parse credentials from a GoogleCredentialFile
    fn from_credential_file(creds: GoogleCredentialFile) -> Result<Self> {
        // Support both "installed" (desktop) and "web" credential types
        let installed = creds
            .installed
            .or(creds.web)
            .context("Credentials file missing 'installed' or 'web' section")?;

        Ok(Self {
            client_id: installed.client_id,
            client_secret: installed.client_secret,
        })
    }

    /// Parse credentials from JSON string (Google Cloud Console format)
    pub fn from_json(json: &str) -> Result<Self> {
        let creds: GoogleCredentialFile =
            serde_json::from_str(json).context("Failed to parse credentials JSON")?;
        Self::from_credential_file(creds)
    }

    /// Load credentials from environment variables
    pub fn from_env() -> Result<Self> {
        let client_id = std::env::var("GMAIL_CLIENT_ID")
            .context("GMAIL_CLIENT_ID environment variable not set")?;
        let client_secret = std::env::var("GMAIL_CLIENT_SECRET")
            .context("GMAIL_CLIENT_SECRET environment variable not set")?;

        Ok(Self {
            client_id,
            client_secret,
        })
    }

    /// Get the default credentials file path (~/.config/cosmos/google-credentials.json)
    pub fn default_credentials_path() -> Option<PathBuf> {
        config::config_path(CREDENTIALS_FILE)
    }

    /// Check if credentials are available (either file or env vars)
    pub fn is_available() -> bool {
        if config::config_exists(CREDENTIALS_FILE) {
            return true;
        }
        std::env::var("GMAIL_CLIENT_ID").is_ok() && std::env::var("GMAIL_CLIENT_SECRET").is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_installed_credentials() {
        let json = r#"{
            "installed": {
                "client_id": "test-client-id.apps.googleusercontent.com",
                "client_secret": "test-secret",
                "auth_uri": "https://accounts.google.com/o/oauth2/auth",
                "token_uri": "https://oauth2.googleapis.com/token"
            }
        }"#;

        let creds = GmailCredentials::from_json(json).unwrap();
        assert_eq!(creds.client_id, "test-client-id.apps.googleusercontent.com");
        assert_eq!(creds.client_secret, "test-secret");
    }

    #[test]
    fn test_parse_web_credentials() {
        let json = r#"{
            "web": {
                "client_id": "web-client-id.apps.googleusercontent.com",
                "client_secret": "web-secret"
            }
        }"#;

        let creds = GmailCredentials::from_json(json).unwrap();
        assert_eq!(creds.client_id, "web-client-id.apps.googleusercontent.com");
        assert_eq!(creds.client_secret, "web-secret");
    }

    #[test]
    fn test_invalid_json() {
        let json = r#"{ "other": {} }"#;
        assert!(GmailCredentials::from_json(json).is_err());
    }
}
