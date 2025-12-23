//! File-based blob storage with zstd compression

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use super::blob::{BlobKey, BlobStore, ContentType};

/// File-based blob storage with zstd compression
///
/// Directory structure:
/// ```text
/// blobs/
///   ab/
///     ab12cd34ef56.txt.zst     # body_text for message ab12cd34ef56
///     ab12cd34ef56.html.zst    # body_html for message ab12cd34ef56
///     ab12cd34ef56.att.0.zst   # attachment 0
///   cd/
///     cd78ef90ab12.txt.zst
/// ```
pub struct FileBlobStore {
    root: PathBuf,
    compression_level: i32,
}

impl FileBlobStore {
    /// Create a new file blob store at the given path
    pub fn new(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        fs::create_dir_all(&root).context("Failed to create blob storage directory")?;
        Ok(Self {
            root,
            compression_level: 3, // Good balance of speed vs compression
        })
    }

    /// Get the shard directory (first 2 chars of message_id)
    fn shard<'a>(&self, message_id: &'a str) -> &'a str {
        if message_id.len() >= 2 {
            &message_id[..2]
        } else {
            "xx"
        }
    }

    /// Get the file path for a blob key
    fn blob_path(&self, key: &BlobKey) -> PathBuf {
        let shard = self.shard(&key.message_id);

        let filename = match (&key.content_type, &key.part_id) {
            (ContentType::BodyText, _) => format!("{}.txt.zst", key.message_id),
            (ContentType::BodyHtml, _) => format!("{}.html.zst", key.message_id),
            (ContentType::Attachment, Some(part)) => {
                format!("{}.att.{}.zst", key.message_id, part)
            }
            (ContentType::Attachment, None) => format!("{}.att.zst", key.message_id),
        };

        self.root.join(shard).join(filename)
    }

    /// List all blob files for a message
    fn list_blobs_for_message(&self, message_id: &str) -> Result<Vec<PathBuf>> {
        let shard = self.shard(message_id);
        let shard_dir = self.root.join(shard);

        if !shard_dir.exists() {
            return Ok(Vec::new());
        }

        let prefix = format!("{}.", message_id);
        let mut paths = Vec::new();

        for entry in fs::read_dir(&shard_dir)? {
            let entry = entry?;
            let file_name = entry.file_name();
            if let Some(name) = file_name.to_str() {
                if name.starts_with(&prefix) {
                    paths.push(entry.path());
                }
            }
        }

        Ok(paths)
    }
}

impl BlobStore for FileBlobStore {
    fn put(&self, key: &BlobKey, data: &[u8]) -> Result<()> {
        let path = self.blob_path(key);

        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Compress with zstd
        let compressed =
            zstd::encode_all(data, self.compression_level).context("Failed to compress blob")?;

        // Write atomically (write to temp, then rename)
        let temp_path = path.with_extension("tmp");
        fs::write(&temp_path, &compressed)?;
        fs::rename(&temp_path, &path)?;

        Ok(())
    }

    fn get(&self, key: &BlobKey) -> Result<Option<Vec<u8>>> {
        let path = self.blob_path(key);

        if !path.exists() {
            return Ok(None);
        }

        let compressed = fs::read(&path)?;
        let mut decoder = zstd::Decoder::new(compressed.as_slice())?;
        let mut decompressed = Vec::new();
        decoder
            .read_to_end(&mut decompressed)
            .context("Failed to decompress blob")?;

        Ok(Some(decompressed))
    }

    fn exists(&self, key: &BlobKey) -> Result<bool> {
        Ok(self.blob_path(key).exists())
    }

    fn delete(&self, key: &BlobKey) -> Result<()> {
        let path = self.blob_path(key);
        if path.exists() {
            fs::remove_file(&path)?;
        }
        Ok(())
    }

    fn delete_all_for_message(&self, message_id: &str) -> Result<()> {
        for path in self.list_blobs_for_message(message_id)? {
            fs::remove_file(&path)?;
        }
        Ok(())
    }

    fn clear(&self) -> Result<()> {
        if self.root.exists() {
            fs::remove_dir_all(&self.root)?;
            fs::create_dir_all(&self.root)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_put_get_body_text() {
        let dir = tempdir().unwrap();
        let store = FileBlobStore::new(dir.path().join("blobs")).unwrap();

        let key = BlobKey::body_text("abc123");
        let data = b"Hello, world!";

        store.put(&key, data).unwrap();
        let retrieved = store.get(&key).unwrap().unwrap();

        assert_eq!(retrieved, data);
    }

    #[test]
    fn test_put_get_body_html() {
        let dir = tempdir().unwrap();
        let store = FileBlobStore::new(dir.path().join("blobs")).unwrap();

        let key = BlobKey::body_html("abc123");
        let data = b"<html><body>Hello</body></html>";

        store.put(&key, data).unwrap();
        let retrieved = store.get(&key).unwrap().unwrap();

        assert_eq!(retrieved, data);
    }

    #[test]
    fn test_get_nonexistent() {
        let dir = tempdir().unwrap();
        let store = FileBlobStore::new(dir.path().join("blobs")).unwrap();

        let key = BlobKey::body_text("nonexistent");
        let result = store.get(&key).unwrap();

        assert!(result.is_none());
    }

    #[test]
    fn test_exists() {
        let dir = tempdir().unwrap();
        let store = FileBlobStore::new(dir.path().join("blobs")).unwrap();

        let key = BlobKey::body_text("abc123");

        assert!(!store.exists(&key).unwrap());

        store.put(&key, b"data").unwrap();

        assert!(store.exists(&key).unwrap());
    }

    #[test]
    fn test_delete() {
        let dir = tempdir().unwrap();
        let store = FileBlobStore::new(dir.path().join("blobs")).unwrap();

        let key = BlobKey::body_text("abc123");
        store.put(&key, b"data").unwrap();

        assert!(store.exists(&key).unwrap());

        store.delete(&key).unwrap();

        assert!(!store.exists(&key).unwrap());
    }

    #[test]
    fn test_delete_all_for_message() {
        let dir = tempdir().unwrap();
        let store = FileBlobStore::new(dir.path().join("blobs")).unwrap();

        let text_key = BlobKey::body_text("abc123");
        let html_key = BlobKey::body_html("abc123");

        store.put(&text_key, b"text").unwrap();
        store.put(&html_key, b"html").unwrap();

        assert!(store.exists(&text_key).unwrap());
        assert!(store.exists(&html_key).unwrap());

        store.delete_all_for_message("abc123").unwrap();

        assert!(!store.exists(&text_key).unwrap());
        assert!(!store.exists(&html_key).unwrap());
    }

    #[test]
    fn test_compression() {
        let dir = tempdir().unwrap();
        let store = FileBlobStore::new(dir.path().join("blobs")).unwrap();

        let key = BlobKey::body_html("abc123");
        // Create a string that compresses well
        let data = "Hello, world! ".repeat(1000);

        store.put(&key, data.as_bytes()).unwrap();

        // Check that the file is smaller than the original
        let path = store.blob_path(&key);
        let compressed_size = fs::metadata(&path).unwrap().len();

        assert!(
            compressed_size < data.len() as u64,
            "Compressed size {} should be less than original {}",
            compressed_size,
            data.len()
        );

        // Verify data round-trips correctly
        let retrieved = store.get(&key).unwrap().unwrap();
        assert_eq!(retrieved, data.as_bytes());
    }
}
