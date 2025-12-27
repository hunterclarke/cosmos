//! MailService facade for UniFFI export
//!
//! This provides a high-level, FFI-friendly API that wraps the internal
//! storage, sync, search, and action functionality.

use std::path::PathBuf;
use std::sync::Arc;

use crate::ffi::types::*;
use crate::gmail::{GmailAuth, GmailClient, StoredToken};
use crate::models::{Account, ThreadId};
use crate::search::SearchIndex;
use crate::storage::{FileBlobStore, MailStore, SqliteMailStore};
use crate::sync::SyncOptions;

/// Main service object for mail operations
///
/// This is the primary entry point for Swift/Kotlin code to interact with
/// the mail crate. It wraps storage, search, and provides a clean API.
#[derive(uniffi::Object)]
pub struct MailService {
    store: Arc<SqliteMailStore>,
    search_index: Arc<SearchIndex>,
}

#[uniffi::export]
impl MailService {
    /// Create a new MailService with the given paths
    ///
    /// # Arguments
    /// * `db_path` - Path to the SQLite database file
    /// * `blob_path` - Path to the blob storage directory
    /// * `search_index_path` - Path to the Tantivy search index directory
    #[uniffi::constructor]
    pub fn new(
        db_path: String,
        blob_path: String,
        search_index_path: String,
    ) -> Result<Arc<Self>, MailError> {
        // Ensure parent directories exist
        if let Some(parent) = PathBuf::from(&db_path).parent() {
            std::fs::create_dir_all(parent).map_err(|e| MailError::Database {
                message: format!("Failed to create database directory: {}", e),
            })?;
        }
        std::fs::create_dir_all(&blob_path).map_err(|e| MailError::Database {
            message: format!("Failed to create blob directory: {}", e),
        })?;
        std::fs::create_dir_all(&search_index_path).map_err(|e| MailError::Database {
            message: format!("Failed to create search index directory: {}", e),
        })?;

        // Create blob store first - SqliteMailStore needs it
        let blob_store = FileBlobStore::new(&blob_path).map_err(|e| MailError::Database {
            message: format!("Failed to open blob store: {}", e),
        })?;

        // Create store with blob store
        let store = SqliteMailStore::new(&db_path, Box::new(blob_store)).map_err(|e| {
            MailError::Database {
                message: format!("Failed to open database: {}", e),
            }
        })?;

        let search_index = SearchIndex::open(&search_index_path).map_err(|e| MailError::Database {
            message: format!("Failed to open search index: {}", e),
        })?;

        Ok(Arc::new(Self {
            store: Arc::new(store),
            search_index: Arc::new(search_index),
        }))
    }

    // ========================================================================
    // Account Management
    // ========================================================================

    /// List all registered accounts
    pub fn list_accounts(&self) -> Result<Vec<FfiAccount>, MailError> {
        let accounts = self.store.list_accounts()?;
        Ok(accounts.into_iter().map(FfiAccount::from).collect())
    }

    /// Register a new account with the given email
    ///
    /// Returns the created account with its assigned ID.
    pub fn register_account(&self, email: String) -> Result<FfiAccount, MailError> {
        let account = Account::new(&email);
        let account = self.store.register_account(account)?;
        Ok(FfiAccount::from(account))
    }

    /// Get an account by ID
    pub fn get_account(&self, account_id: i64) -> Result<Option<FfiAccount>, MailError> {
        let account = self.store.get_account(account_id)?;
        Ok(account.map(FfiAccount::from))
    }

    /// Get an account by email address
    pub fn get_account_by_email(&self, email: String) -> Result<Option<FfiAccount>, MailError> {
        let account = self.store.get_account_by_email(&email)?;
        Ok(account.map(FfiAccount::from))
    }

    /// Delete an account and all its data
    pub fn delete_account(&self, account_id: i64) -> Result<(), MailError> {
        self.store.delete_account(account_id)?;
        Ok(())
    }

    /// Update the OAuth token for an account
    ///
    /// The token_json should be a JSON-serialized token object.
    pub fn update_account_token(
        &self,
        account_id: i64,
        token_json: String,
    ) -> Result<(), MailError> {
        self.store.update_account_token(account_id, Some(token_json))?;
        Ok(())
    }

    // ========================================================================
    // Thread Queries
    // ========================================================================

    /// List threads with pagination
    ///
    /// Returns threads sorted by last_message_at descending (newest first).
    pub fn list_threads(
        &self,
        label: Option<String>,
        account_id: Option<i64>,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<FfiThreadSummary>, MailError> {
        // Treat "ALL" as None (no label filter = all mail)
        let label = label.filter(|l| l != "ALL");

        let threads = match (label.as_deref(), account_id) {
            (Some(label), Some(account_id)) => self.store.list_threads_by_label_for_account(
                label,
                Some(account_id),
                limit as usize,
                offset as usize,
            )?,
            (Some(label), None) => {
                self.store
                    .list_threads_by_label(label, limit as usize, offset as usize)?
            }
            (None, Some(account_id)) => self
                .store
                .list_threads_for_account(Some(account_id), limit as usize, offset as usize)?,
            (None, None) => self.store.list_threads(limit as usize, offset as usize)?,
        };

        Ok(threads
            .into_iter()
            .map(|t| FfiThreadSummary::from(crate::query::ThreadSummary::from(t)))
            .collect())
    }

    /// Get detailed thread information including all messages
    pub fn get_thread_detail(&self, thread_id: String) -> Result<Option<FfiThreadDetail>, MailError> {
        let tid = ThreadId::new(thread_id);
        let detail = crate::query::get_thread_detail(self.store.as_ref(), &tid)?;
        Ok(detail.map(FfiThreadDetail::from))
    }

    /// Count threads (optionally filtered by label and/or account)
    pub fn count_threads(
        &self,
        label: Option<String>,
        account_id: Option<i64>,
    ) -> Result<u32, MailError> {
        // Treat "ALL" as None (no label filter = all mail)
        let label = label.filter(|l| l != "ALL");

        let count = match (label.as_deref(), account_id) {
            (Some(label), Some(account_id)) => {
                self.store.count_threads_by_label_for_account(label, Some(account_id))?
            }
            (Some(label), None) => self.store.count_threads_by_label(label)?,
            (None, Some(account_id)) => self.store.count_threads_for_account(Some(account_id))?,
            (None, None) => self.store.count_threads()?,
        };
        Ok(count as u32)
    }

    /// Count unread threads for a label
    pub fn count_unread(
        &self,
        label: String,
        account_id: Option<i64>,
    ) -> Result<u32, MailError> {
        let count = match account_id {
            Some(account_id) => self
                .store
                .count_unread_threads_by_label_for_account(&label, Some(account_id))?,
            None => self.store.count_unread_threads_by_label(&label)?,
        };
        Ok(count as u32)
    }

    // ========================================================================
    // Search
    // ========================================================================

    /// Search threads by query string
    ///
    /// Supports Gmail-style operators like `from:`, `to:`, `subject:`,
    /// `is:unread`, `in:inbox`, `before:`, `after:`.
    pub fn search(
        &self,
        query: String,
        limit: u32,
        account_id: Option<i64>,
    ) -> Result<Vec<FfiSearchResult>, MailError> {
        let results = crate::search::search_threads_for_account(
            &self.search_index,
            self.store.as_ref(),
            &query,
            limit as usize,
            account_id,
        )?;
        Ok(results.into_iter().map(FfiSearchResult::from).collect())
    }

    // ========================================================================
    // Sync
    // ========================================================================

    /// Get the current sync state for an account
    pub fn get_sync_state(&self, account_id: i64) -> Result<Option<FfiSyncState>, MailError> {
        let state = self.store.get_sync_state(account_id)?;
        Ok(state.map(FfiSyncState::from))
    }

    /// Sync an account with Gmail
    ///
    /// This performs either an initial sync or incremental sync depending on
    /// the current sync state.
    ///
    /// # Arguments
    /// * `account_id` - The account to sync
    /// * `token_json` - JSON-serialized token with access_token, refresh_token, expires_at
    /// * `client_id` - OAuth client ID
    /// * `client_secret` - OAuth client secret
    /// * `callback` - Progress callback for UI updates
    pub fn sync_account(
        &self,
        account_id: i64,
        token_json: String,
        client_id: String,
        client_secret: String,
        callback: Box<dyn SyncProgressCallback>,
    ) -> Result<FfiSyncStats, MailError> {
        // Create a GmailClient with the provided token
        let auth = GmailAuth::with_token_data(client_id, client_secret, Some(token_json));
        let gmail = GmailClient::new(auth);

        // Set up sync options with search index for incremental indexing
        let options = SyncOptions {
            max_messages: None,
            full_resync: false,
            search_index: Some(self.search_index.clone()),
        };

        // Notify starting
        callback.on_progress(0, None, "Starting sync...".to_string());

        // Run sync
        let stats = crate::sync::sync_gmail(&gmail, self.store.as_ref(), account_id, options)
            .map_err(|e| {
                callback.on_error(e.to_string());
                MailError::Sync {
                    message: e.to_string(),
                }
            })?;

        // Notify completion
        callback.on_progress(
            stats.messages_fetched as u32,
            Some(stats.messages_fetched as u32),
            "Sync complete".to_string(),
        );

        Ok(FfiSyncStats::from(stats))
    }

    /// Perform a full resync, clearing existing data
    pub fn full_resync(
        &self,
        account_id: i64,
        token_json: String,
        client_id: String,
        client_secret: String,
        callback: Box<dyn SyncProgressCallback>,
    ) -> Result<FfiSyncStats, MailError> {
        // Create a GmailClient with the provided token
        let auth = GmailAuth::with_token_data(client_id, client_secret, Some(token_json));
        let gmail = GmailClient::new(auth);

        // Set up sync options with full_resync flag
        let options = SyncOptions {
            max_messages: None,
            full_resync: true,
            search_index: Some(self.search_index.clone()),
        };

        callback.on_progress(0, None, "Starting full resync...".to_string());

        let stats = crate::sync::sync_gmail(&gmail, self.store.as_ref(), account_id, options)
            .map_err(|e| {
                callback.on_error(e.to_string());
                MailError::Sync {
                    message: e.to_string(),
                }
            })?;

        callback.on_progress(
            stats.messages_fetched as u32,
            Some(stats.messages_fetched as u32),
            "Full resync complete".to_string(),
        );

        Ok(FfiSyncStats::from(stats))
    }

    // ========================================================================
    // Actions
    // ========================================================================

    /// Archive a thread (remove INBOX label)
    pub fn archive_thread(
        &self,
        thread_id: String,
        token_json: String,
        client_id: String,
        client_secret: String,
    ) -> Result<(), MailError> {
        let auth = GmailAuth::with_token_data(client_id, client_secret, Some(token_json));
        let gmail = GmailClient::new(auth);
        let handler = crate::actions::ActionHandler::new(Arc::new(gmail), self.store.clone());

        handler
            .archive_thread(&ThreadId::new(thread_id))
            .map_err(|e| MailError::Network {
                message: e.to_string(),
            })?;
        Ok(())
    }

    /// Toggle star on a thread
    ///
    /// Returns the new starred state (true = starred, false = unstarred).
    pub fn toggle_star(
        &self,
        thread_id: String,
        token_json: String,
        client_id: String,
        client_secret: String,
    ) -> Result<bool, MailError> {
        let auth = GmailAuth::with_token_data(client_id, client_secret, Some(token_json));
        let gmail = GmailClient::new(auth);
        let handler = crate::actions::ActionHandler::new(Arc::new(gmail), self.store.clone());

        let is_starred = handler
            .toggle_star(&ThreadId::new(thread_id))
            .map_err(|e| MailError::Network {
                message: e.to_string(),
            })?;
        Ok(is_starred)
    }

    /// Set the read state of a thread
    pub fn set_read(
        &self,
        thread_id: String,
        is_read: bool,
        token_json: String,
        client_id: String,
        client_secret: String,
    ) -> Result<(), MailError> {
        let auth = GmailAuth::with_token_data(client_id, client_secret, Some(token_json));
        let gmail = GmailClient::new(auth);
        let handler = crate::actions::ActionHandler::new(Arc::new(gmail), self.store.clone());

        handler
            .set_read(&ThreadId::new(thread_id), is_read)
            .map_err(|e| MailError::Network {
                message: e.to_string(),
            })?;
        Ok(())
    }

    /// Move a thread to trash
    pub fn trash_thread(
        &self,
        thread_id: String,
        token_json: String,
        client_id: String,
        client_secret: String,
    ) -> Result<(), MailError> {
        let auth = GmailAuth::with_token_data(client_id, client_secret, Some(token_json));
        let gmail = GmailClient::new(auth);
        let handler = crate::actions::ActionHandler::new(Arc::new(gmail), self.store.clone());

        handler
            .trash_thread(&ThreadId::new(thread_id))
            .map_err(|e| MailError::Network {
                message: e.to_string(),
            })?;
        Ok(())
    }
}

// ============================================================================
// Free Functions
// ============================================================================

/// Parse a search query and return the parsed structure
///
/// This is useful for validating queries before executing them.
#[uniffi::export]
pub fn parse_search_query(query: String) -> String {
    let parsed = crate::search::parse_query(&query);
    format!("{:?}", parsed)
}

/// Get the icon emoji for a label
#[uniffi::export]
pub fn get_label_icon(label_id: String) -> String {
    crate::models::label_icon(&label_id).to_string()
}

/// Get the sort order for a label (lower = higher priority)
#[uniffi::export]
pub fn get_label_sort_order(label_id: String) -> u32 {
    crate::models::label_sort_order(&label_id)
}

/// Create a token JSON string from OAuth response components
///
/// This helper creates the JSON format expected by the sync and action methods.
/// Swift should call this after completing OAuth to create the token string.
///
/// # Arguments
/// * `access_token` - The OAuth access token
/// * `refresh_token` - The OAuth refresh token (optional but recommended)
/// * `expires_at` - Unix timestamp when the token expires (optional)
#[uniffi::export]
pub fn create_token_json(
    access_token: String,
    refresh_token: Option<String>,
    expires_at: Option<i64>,
) -> String {
    let token = StoredToken {
        access_token,
        refresh_token,
        expires_at,
    };
    serde_json::to_string(&token).unwrap_or_else(|_| "{}".to_string())
}
