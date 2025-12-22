//! Gmail OAuth2 authentication
//!
//! Implements OAuth2 authorization code flow for Gmail API authentication.
//! Uses a local HTTP server to receive the OAuth callback.
//! Uses synchronous HTTP (ureq) to be executor-agnostic.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::PathBuf;

/// OAuth2 configuration and token management for Gmail
pub struct GmailAuth {
    client_id: String,
    client_secret: String,
    token_path: PathBuf,
}

/// Stored token data
#[derive(Debug, Serialize, Deserialize)]
struct StoredToken {
    access_token: String,
    refresh_token: Option<String>,
    expires_at: Option<i64>,
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

    /// Create a new GmailAuth instance
    ///
    /// # Arguments
    /// * `client_id` - OAuth2 client ID from Google Cloud Console
    /// * `client_secret` - OAuth2 client secret from Google Cloud Console
    pub fn new(client_id: String, client_secret: String) -> Result<Self> {
        let token_path = Self::default_token_path()?;

        Ok(Self {
            client_id,
            client_secret,
            token_path,
        })
    }

    /// Get the default token storage path (~/.config/cosmos/gmail-tokens.json)
    fn default_token_path() -> Result<PathBuf> {
        config::config_path("gmail-tokens.json").context("Could not determine config directory")
    }

    /// Get a valid access token, refreshing or re-authenticating as needed
    pub fn get_access_token(&self) -> Result<String> {
        // Try to load existing token
        if let Ok(token) = self.load_token() {
            // Check if token is still valid (with 5 minute buffer)
            if let Some(expires_at) = token.expires_at {
                let now = chrono::Utc::now().timestamp();
                if expires_at > now + 300 {
                    return Ok(token.access_token);
                }
            }

            // Try to refresh the token
            if let Some(refresh_token) = token.refresh_token
                && let Ok(new_token) = self.refresh_access_token(&refresh_token)
            {
                self.save_token_response(&new_token)?;
                return Ok(new_token.access_token);
            }
        }

        // Need to authenticate from scratch
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

        println!("\n=== Gmail Authentication Required ===");
        println!("Opening browser for authentication...");
        println!("If the browser doesn't open, visit: {}", auth_url);

        // Open browser
        if let Err(e) = open::that(&auth_url) {
            eprintln!("Failed to open browser: {}. Please open the URL manually.", e);
        }

        // Step 3: Wait for callback with authorization code
        println!("Waiting for authorization...");
        let code = self.wait_for_callback(listener)?;

        // Step 4: Exchange code for tokens
        println!("Exchanging authorization code for tokens...");
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

        println!("Authentication successful!\n");
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

    /// Load stored token from disk
    fn load_token(&self) -> Result<StoredToken> {
        let content = fs::read_to_string(&self.token_path)?;
        let token: StoredToken = serde_json::from_str(&content)?;
        Ok(token)
    }

    /// Save token response to disk
    fn save_token_response(&self, token: &TokenResponse) -> Result<()> {
        // Ensure directory exists
        if let Some(parent) = self.token_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let stored = StoredToken {
            access_token: token.access_token.clone(),
            refresh_token: token.refresh_token.clone(),
            expires_at: token
                .expires_in
                .map(|d| chrono::Utc::now().timestamp() + d as i64),
        };

        let content = serde_json::to_string_pretty(&stored)?;
        fs::write(&self.token_path, content)?;
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
        if self.token_path.exists() {
            fs::remove_file(&self.token_path)?;
        }
        Ok(())
    }
}
