//! Gmail OAuth2 authentication
//!
//! Implements OAuth2 authorization code flow for Gmail API authentication.
//! Uses a local HTTP server to receive the OAuth callback.
//! Uses synchronous HTTP (ureq) to be executor-agnostic.
//!
//! Supports two storage modes:
//! - Database: tokens stored in SQLite as JSON strings
//! - File: tokens stored in JSON files (legacy, for testing)

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::RwLock;

/// Token storage mode
enum TokenStorage {
    /// Store tokens in a file (legacy mode)
    File(PathBuf),
    /// Store tokens in memory (for database mode - caller handles persistence)
    Memory(RwLock<Option<String>>),
}

/// OAuth2 configuration and token management for Gmail
pub struct GmailAuth {
    client_id: String,
    client_secret: String,
    storage: TokenStorage,
}

/// Stored token data (public for database serialization)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredToken {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<i64>,
}

/// Token response from Google
#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
    #[allow(dead_code)]
    token_type: String,
}

impl GmailAuth {
    /// Gmail API OAuth2 endpoints
    const AUTH_URL: &'static str = "https://accounts.google.com/o/oauth2/v2/auth";
    const TOKEN_URL: &'static str = "https://oauth2.googleapis.com/token";

    /// Required scope for Gmail access (modify allows read + label changes)
    const GMAIL_MODIFY_SCOPE: &'static str = "https://www.googleapis.com/auth/gmail.modify";

    /// Port range to try for local OAuth callback server
    const PORT_RANGE_START: u16 = 8080;
    const PORT_RANGE_END: u16 = 8090;

    /// Create a new GmailAuth instance with default token path (file storage)
    ///
    /// # Arguments
    /// * `client_id` - OAuth2 client ID from Google Cloud Console
    /// * `client_secret` - OAuth2 client secret from Google Cloud Console
    pub fn new(client_id: String, client_secret: String) -> Result<Self> {
        let token_path = Self::default_token_path()?;

        Ok(Self {
            client_id,
            client_secret,
            storage: TokenStorage::File(token_path),
        })
    }

    /// Create a GmailAuth instance for a specific account (file storage)
    ///
    /// Uses a per-account token file based on the email address.
    /// Token path: `~/.config/cosmos/gmail-tokens-{sanitized_email}.json`
    ///
    /// # Arguments
    /// * `client_id` - OAuth2 client ID from Google Cloud Console
    /// * `client_secret` - OAuth2 client secret from Google Cloud Console
    /// * `email` - Email address of the account
    pub fn for_account(client_id: String, client_secret: String, email: &str) -> Result<Self> {
        let token_path = Self::account_token_path(email)?;

        Ok(Self {
            client_id,
            client_secret,
            storage: TokenStorage::File(token_path),
        })
    }

    /// Create a GmailAuth instance with in-memory token storage (for database mode)
    ///
    /// The token_data should be a JSON-serialized StoredToken, or None for new accounts.
    /// After authentication or token refresh, call `get_token_data()` to get the
    /// updated token JSON for saving to the database.
    ///
    /// # Arguments
    /// * `client_id` - OAuth2 client ID from Google Cloud Console
    /// * `client_secret` - OAuth2 client secret from Google Cloud Console
    /// * `token_data` - Optional JSON-serialized token data from database
    pub fn with_token_data(
        client_id: String,
        client_secret: String,
        token_data: Option<String>,
    ) -> Self {
        Self {
            client_id,
            client_secret,
            storage: TokenStorage::Memory(RwLock::new(token_data)),
        }
    }

    /// Get the current token data as a JSON string for database storage
    ///
    /// Returns None if no token has been obtained yet.
    pub fn get_token_data(&self) -> Option<String> {
        match &self.storage {
            TokenStorage::File(path) => fs::read_to_string(path).ok(),
            TokenStorage::Memory(data) => data.read().unwrap().clone(),
        }
    }

    /// Get the token storage path for a specific account
    ///
    /// Sanitizes the email to create a valid filename:
    /// - `@` becomes `-at-`
    /// - `.` becomes `-`
    /// - Lowercase
    pub fn account_token_path(email: &str) -> Result<PathBuf> {
        let sanitized = email
            .replace('@', "-at-")
            .replace('.', "-")
            .to_lowercase();
        config::config_path(&format!("gmail-tokens-{}.json", sanitized))
            .context("Could not determine config directory")
    }

    /// Get the default token storage path (~/.config/cosmos/gmail-tokens.json)
    fn default_token_path() -> Result<PathBuf> {
        config::config_path("gmail-tokens.json").context("Could not determine config directory")
    }

    /// Get the token path being used by this instance (only for file storage mode)
    pub fn token_path(&self) -> Option<&PathBuf> {
        match &self.storage {
            TokenStorage::File(path) => Some(path),
            TokenStorage::Memory(_) => None,
        }
    }

    /// Get a valid access token, refreshing or re-authenticating as needed
    pub fn get_access_token(&self) -> Result<String> {
        // Try to load existing token
        match self.load_token() {
            Ok(token) => {
                log::debug!("Token loaded, expires_at: {:?}", token.expires_at);
                // Check if token is still valid (with 5 minute buffer)
                if let Some(expires_at) = token.expires_at {
                    let now = chrono::Utc::now().timestamp();
                    log::debug!("Token expires_at={}, now={}, diff={}", expires_at, now, expires_at - now);
                    if expires_at > now + 300 {
                        log::debug!("Token is valid, returning access token");
                        return Ok(token.access_token);
                    }
                    log::debug!("Token expired or expiring soon, attempting refresh");
                }

                // Try to refresh the token
                if let Some(ref refresh_token) = token.refresh_token {
                    log::debug!("Attempting token refresh");
                    match self.refresh_access_token(refresh_token) {
                        Ok(new_token) => {
                            self.save_token_response(&new_token)?;
                            log::debug!("Token refreshed successfully");
                            return Ok(new_token.access_token);
                        }
                        Err(e) => {
                            log::warn!("Token refresh failed: {}", e);
                        }
                    }
                } else {
                    log::warn!("No refresh token available");
                }
            }
            Err(e) => {
                log::debug!("Failed to load token: {}", e);
            }
        }

        // For in-memory storage (FFI/mobile), we cannot do interactive auth
        // Return an error so the caller can re-authenticate through the native flow
        if matches!(&self.storage, TokenStorage::Memory(_)) {
            anyhow::bail!("Token expired or invalid. Please re-authenticate through the app.");
        }

        // Need to authenticate from scratch (only for file-based desktop auth)
        let token = self.authorization_code_auth()?;
        self.save_token_response(&token)?;
        Ok(token.access_token)
    }

    /// Perform authorization code flow authentication
    fn authorization_code_auth(&self) -> Result<TokenResponse> {
        // Step 1: Start local server to receive callback
        let (listener, port) = self.start_local_server()?;
        let redirect_uri = format!("http://localhost:{}", port);

        // Step 2: Build authorization URL
        let auth_url = format!(
            "{}?client_id={}&redirect_uri={}&response_type=code&scope={}&access_type=offline&prompt=consent",
            Self::AUTH_URL,
            urlencoding::encode(&self.client_id),
            urlencoding::encode(&redirect_uri),
            urlencoding::encode(Self::GMAIL_MODIFY_SCOPE),
        );

        log::info!("=== Gmail Authentication Required ===");
        log::info!("Opening browser for authentication...");
        log::info!("If the browser doesn't open, visit: {}", auth_url);

        // Open browser
        if let Err(e) = open::that(&auth_url) {
            log::warn!("Failed to open browser: {}. Please open the URL manually.", e);
        }

        // Step 3: Wait for callback with authorization code
        log::info!("Waiting for authorization...");
        let code = self.wait_for_callback(listener)?;

        // Step 4: Exchange code for tokens
        log::info!("Exchanging authorization code for tokens...");
        let mut response = ureq::post(Self::TOKEN_URL)
            .send_form([
                ("client_id", self.client_id.as_str()),
                ("client_secret", self.client_secret.as_str()),
                ("code", code.as_str()),
                ("grant_type", "authorization_code"),
                ("redirect_uri", redirect_uri.as_str()),
            ])
            .context("Failed to exchange authorization code")?;

        let token: TokenResponse = response
            .body_mut()
            .read_json()
            .context("Failed to parse token response")?;

        log::info!("Authentication successful!");
        Ok(token)
    }

    /// Start a local TCP server on an available port
    fn start_local_server(&self) -> Result<(TcpListener, u16)> {
        for port in Self::PORT_RANGE_START..=Self::PORT_RANGE_END {
            if let Ok(listener) = TcpListener::bind(format!("127.0.0.1:{}", port)) {
                return Ok((listener, port));
            }
        }
        anyhow::bail!(
            "Could not bind to any port in range {}-{}",
            Self::PORT_RANGE_START,
            Self::PORT_RANGE_END
        )
    }

    /// Wait for OAuth callback and extract authorization code
    fn wait_for_callback(&self, listener: TcpListener) -> Result<String> {
        let (mut stream, _) = listener.accept().context("Failed to accept connection")?;

        let mut reader = BufReader::new(&stream);
        let mut request_line = String::new();
        reader
            .read_line(&mut request_line)
            .context("Failed to read request")?;

        // Parse the request to get the code
        // Format: GET /?code=AUTH_CODE&scope=... HTTP/1.1
        let code = request_line
            .split_whitespace()
            .nth(1) // Get the path
            .and_then(|path| {
                path.split('?')
                    .nth(1) // Get query string
                    .and_then(|query| {
                        query.split('&').find_map(|param| {
                            let mut parts = param.split('=');
                            if parts.next() == Some("code") {
                                parts.next().map(|s| s.to_string())
                            } else {
                                None
                            }
                        })
                    })
            });

        // Check for error in callback
        let error = request_line
            .split_whitespace()
            .nth(1)
            .and_then(|path| {
                path.split('?').nth(1).and_then(|query| {
                    query.split('&').find_map(|param| {
                        let mut parts = param.split('=');
                        if parts.next() == Some("error") {
                            parts.next().map(|s| s.to_string())
                        } else {
                            None
                        }
                    })
                })
            });

        // Send response to browser
        let (status, body) = if code.is_some() {
            ("200 OK", "Authentication successful! You can close this window.")
        } else {
            ("400 Bad Request", "Authentication failed. Please try again.")
        };

        let response = format!(
            "HTTP/1.1 {}\r\nContent-Type: text/html\r\nConnection: close\r\n\r\n<html><body><h1>{}</h1></body></html>",
            status, body
        );
        stream.write_all(response.as_bytes()).ok();

        if let Some(err) = error {
            anyhow::bail!("OAuth error: {}", err);
        }

        code.context("No authorization code received")
    }

    /// Refresh an access token using a refresh token
    fn refresh_access_token(&self, refresh_token: &str) -> Result<TokenResponse> {
        let response = ureq::post(Self::TOKEN_URL)
            .send_form([
                ("client_id", self.client_id.as_str()),
                ("client_secret", self.client_secret.as_str()),
                ("refresh_token", refresh_token),
                ("grant_type", "refresh_token"),
            ])
            .context("Failed to refresh access token")?;

        let mut token: TokenResponse = response
            .into_body()
            .read_json()
            .context("Failed to parse refresh token response")?;

        // Preserve the refresh token if not returned
        if token.refresh_token.is_none() {
            token.refresh_token = Some(refresh_token.to_string());
        }

        Ok(token)
    }

    /// Load stored token
    fn load_token(&self) -> Result<StoredToken> {
        let content = match &self.storage {
            TokenStorage::File(path) => fs::read_to_string(path)?,
            TokenStorage::Memory(data) => {
                let data = data
                    .read()
                    .unwrap()
                    .clone()
                    .context("No token data in memory")?;
                log::debug!("Loading token from memory, len={}", data.len());
                data
            }
        };
        let token: StoredToken = serde_json::from_str(&content).context("Failed to parse token JSON")?;
        Ok(token)
    }

    /// Save token response
    fn save_token_response(&self, token: &TokenResponse) -> Result<()> {
        let stored = StoredToken {
            access_token: token.access_token.clone(),
            refresh_token: token.refresh_token.clone(),
            expires_at: token
                .expires_in
                .map(|d| chrono::Utc::now().timestamp() + d as i64),
        };

        let content = serde_json::to_string_pretty(&stored)?;

        match &self.storage {
            TokenStorage::File(path) => {
                // Ensure directory exists
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::write(path, content)?;
            }
            TokenStorage::Memory(data) => {
                *data.write().unwrap() = Some(content);
            }
        }
        Ok(())
    }

    /// Check if the user is already authenticated
    pub fn is_authenticated(&self) -> bool {
        if let Ok(token) = self.load_token() {
            if let Some(expires_at) = token.expires_at {
                let now = chrono::Utc::now().timestamp();
                if expires_at > now + 300 {
                    return true;
                }
            }
            // Try refresh
            if let Some(refresh_token) = token.refresh_token {
                return self.refresh_access_token(&refresh_token).is_ok();
            }
        }
        false
    }

    /// Clear stored tokens (logout)
    pub fn logout(&self) -> Result<()> {
        match &self.storage {
            TokenStorage::File(path) => {
                if path.exists() {
                    fs::remove_file(path)?;
                }
            }
            TokenStorage::Memory(data) => {
                *data.write().unwrap() = None;
            }
        }
        Ok(())
    }

    /// Discover all account emails with saved token files
    ///
    /// Scans the config directory for `gmail-tokens-*.json` files and
    /// extracts the email addresses from the filenames.
    ///
    /// Returns a list of email addresses that have token files.
    pub fn discover_account_emails() -> Result<Vec<String>> {
        let config_dir =
            config::config_dir().context("Could not determine config directory")?;

        let mut emails = Vec::new();

        // Read directory entries
        let entries = match fs::read_dir(&config_dir) {
            Ok(entries) => entries,
            Err(_) => return Ok(emails), // Directory doesn't exist, no accounts
        };

        for entry in entries.flatten() {
            let file_name = entry.file_name();
            let name = file_name.to_string_lossy();

            // Match pattern: gmail-tokens-{sanitized_email}.json
            if let Some(rest) = name.strip_prefix("gmail-tokens-") {
                if let Some(sanitized) = rest.strip_suffix(".json") {
                    // Convert sanitized email back to real email
                    // -at- -> @, - -> .
                    // Note: This is a best-effort reverse of the sanitization
                    let email = sanitized
                        .replace("-at-", "@")
                        .replace('-', ".");

                    emails.push(email);
                }
            }
        }

        Ok(emails)
    }
}
