//! FFI bindings for UniFFI export
//!
//! This module provides Swift/Kotlin bindings for the mail crate via UniFFI.
//!
//! ## Usage from Swift
//!
//! ```swift
//! import MailFFI
//!
//! // Initialize logging first
//! initializeLogging(callback: myLogCallback, maxLevel: 2)
//!
//! // Initialize the mail service
//! let service = try MailService(
//!     dbPath: "/path/to/mail.db",
//!     blobPath: "/path/to/mail.blobs",
//!     searchIndexPath: "/path/to/mail.search.idx"
//! )
//!
//! // List accounts
//! let accounts = try service.listAccounts()
//!
//! // Sync an account
//! let tokenJson = createTokenJson(
//!     accessToken: accessToken,
//!     refreshToken: refreshToken,
//!     expiresAt: expiresAt
//! )
//! let stats = try service.syncAccount(
//!     accountId: 1,
//!     tokenJson: tokenJson,
//!     clientId: clientId,
//!     clientSecret: clientSecret,
//!     callback: progressCallback
//! )
//! ```

mod logging;
mod service;
mod types;

// Re-export all FFI types and the MailService
pub use logging::{init_ffi_logger, set_log_callback, set_log_level};
pub use service::*;
pub use types::*;
