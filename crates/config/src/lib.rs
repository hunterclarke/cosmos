//! Configuration loading for Cosmos applications
//!
//! Provides utilities for loading configuration files from the shared
//! Cosmos config directory (~/.config/cosmos/).
//!
//! Call [`init`] at application startup to bootstrap the config directory.

use anyhow::{Context, Result};
use serde::de::DeserializeOwned;
use std::path::{Path, PathBuf};

/// Initialize the Cosmos config directory.
///
/// Creates ~/.config/cosmos/ if it doesn't exist.
/// Call this once at application startup.
pub fn init() -> Result<PathBuf> {
    ensure_config_dir()
}

/// Get the Cosmos config directory (~/.config/cosmos/)
pub fn config_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|p| p.join("cosmos"))
}

/// Get the path to a config file within the Cosmos config directory
pub fn config_path(filename: &str) -> Option<PathBuf> {
    config_dir().map(|p| p.join(filename))
}

/// Load and parse a JSON config file from the Cosmos config directory
pub fn load_json<T: DeserializeOwned>(filename: &str) -> Result<T> {
    let path = config_path(filename).context("Could not determine config directory")?;
    load_json_file(&path)
}

/// Load and parse a JSON file from an arbitrary path
pub fn load_json_file<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read config file: {}", path.display()))?;
    serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse config file: {}", path.display()))
}

/// Check if a config file exists in the Cosmos config directory
pub fn config_exists(filename: &str) -> bool {
    config_path(filename).is_some_and(|p| p.exists())
}

/// Ensure the Cosmos config directory exists
pub fn ensure_config_dir() -> Result<PathBuf> {
    let dir = config_dir().context("Could not determine config directory")?;
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("Failed to create config directory: {}", dir.display()))?;
    Ok(dir)
}

/// Save a value as JSON to a config file in the Cosmos config directory
pub fn save_json<T: serde::Serialize>(filename: &str, value: &T) -> Result<()> {
    let dir = ensure_config_dir()?;
    let path = dir.join(filename);
    let content = serde_json::to_string_pretty(value)?;
    std::fs::write(&path, content)
        .with_context(|| format!("Failed to write config file: {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_dir() {
        let dir = config_dir();
        assert!(dir.is_some());
        assert!(dir.unwrap().ends_with("cosmos"));
    }

    #[test]
    fn test_config_path() {
        let path = config_path("test.json");
        assert!(path.is_some());
        let path = path.unwrap();
        assert!(path.ends_with("cosmos/test.json"));
    }
}
