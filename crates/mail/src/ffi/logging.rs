//! FFI logging backend that routes logs to Swift/Kotlin via callback
//!
//! This module provides a custom `log` backend that can forward log messages
//! to a UniFFI callback, enabling Rust logs to appear in Apple's unified logging.

use std::sync::{Arc, OnceLock, RwLock};

use log::{Level, Log, Metadata, Record, SetLoggerError};

use super::types::{FfiLogLevel, LogCallback};

/// Global storage for the FFI logger
static FFI_LOGGER: OnceLock<FfiLogger> = OnceLock::new();

/// FFI Logger that forwards to a callback when set
struct FfiLogger {
    callback: RwLock<Option<Arc<dyn LogCallback>>>,
    max_level: RwLock<Level>,
}

impl FfiLogger {
    fn new(max_level: Level) -> Self {
        Self {
            callback: RwLock::new(None),
            max_level: RwLock::new(max_level),
        }
    }

    fn set_callback(&self, callback: Option<Arc<dyn LogCallback>>) {
        if let Ok(mut guard) = self.callback.write() {
            *guard = callback;
        }
    }

    fn set_max_level(&self, level: Level) {
        if let Ok(mut guard) = self.max_level.write() {
            *guard = level;
        }
    }

    fn get_max_level(&self) -> Level {
        self.max_level.read().map(|l| *l).unwrap_or(Level::Info)
    }
}

impl Log for FfiLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= self.get_max_level()
            && self
                .callback
                .read()
                .ok()
                .map_or(false, |cb| cb.is_some())
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }

        if let Ok(guard) = self.callback.read() {
            if let Some(ref callback) = *guard {
                let level = FfiLogLevel::from(record.level());
                let target = record.target().to_string();
                let message = format!("{}", record.args());

                // Call the callback - errors are silently ignored to avoid recursion
                callback.on_log(level, target, message);
            }
        }
    }

    fn flush(&self) {}
}

/// Initialize the FFI logger as the global logger
///
/// This should be called once at startup. The callback can be set later via
/// `set_log_callback`. If a callback is not set, logs will be silently dropped.
///
/// # Arguments
/// * `max_level` - Maximum log level to capture (e.g., Level::Debug)
///
/// # Returns
/// Ok(()) if the logger was successfully installed, or Err if a logger was already set.
pub fn init_ffi_logger(max_level: Level) -> Result<(), SetLoggerError> {
    let logger = FFI_LOGGER.get_or_init(|| FfiLogger::new(max_level));

    // Try to set as global logger - may fail if another logger (e.g., env_logger) is already set
    log::set_logger(logger)?;
    log::set_max_level(max_level.to_level_filter());
    Ok(())
}

/// Set the log callback that will receive all log messages
///
/// This can be called at any time after `init_ffi_logger`. Pass `None` to disable
/// logging to the callback (logs will be silently dropped).
///
/// # Thread Safety
/// This function is thread-safe and can be called from any thread.
pub fn set_log_callback(callback: Option<Arc<dyn LogCallback>>) {
    if let Some(logger) = FFI_LOGGER.get() {
        logger.set_callback(callback);
    }
}

/// Update the maximum log level
///
/// This can be called at any time to change the log level filter.
pub fn set_log_level(level: Level) {
    if let Some(logger) = FFI_LOGGER.get() {
        logger.set_max_level(level);
        log::set_max_level(level.to_level_filter());
    }
}
