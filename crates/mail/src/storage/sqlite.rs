//! SQLite-based mail storage with blob storage for message bodies

use std::path::Path;
use std::sync::Mutex;

use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, params};
use rusqlite_migration::{M, Migrations};

use super::blob::BlobStore;
use super::traits::{MailStore, MessageBody, MessageMetadata, PendingMessage};
use crate::models::{EmailAddress, Message, MessageId, SyncState, Thread, ThreadId};

/// Database migrations
///
/// Each migration is applied in order. The user_version pragma tracks which
/// migrations have been applied.
fn migrations() -> Migrations<'static> {
    Migrations::new(vec![
        // Migration 1: Initial schema
        M::up(
            r#"
            -- Sync state per account
            CREATE TABLE sync_state (
                account_id TEXT PRIMARY KEY,
                history_id TEXT NOT NULL,
                last_sync_at TEXT NOT NULL,
                sync_version INTEGER NOT NULL DEFAULT 1,
                initial_sync_complete INTEGER NOT NULL DEFAULT 0
            );

            -- Thread metadata
            CREATE TABLE threads (
                id TEXT PRIMARY KEY,
                subject TEXT NOT NULL,
                snippet TEXT NOT NULL,
                last_message_at TEXT NOT NULL,
                message_count INTEGER NOT NULL DEFAULT 0,
                sender_name TEXT,
                sender_email TEXT NOT NULL,
                is_unread INTEGER NOT NULL DEFAULT 0
            );

            CREATE INDEX idx_threads_last_message_at
                ON threads(last_message_at DESC);

            -- Message metadata with zstd-compressed bodies
            CREATE TABLE messages (
                id TEXT PRIMARY KEY,
                thread_id TEXT NOT NULL,
                from_name TEXT,
                from_email TEXT NOT NULL,
                subject TEXT NOT NULL,
                body_preview TEXT NOT NULL,
                received_at TEXT NOT NULL,
                internal_date INTEGER NOT NULL,
                has_body_text INTEGER NOT NULL DEFAULT 0,
                has_body_html INTEGER NOT NULL DEFAULT 0,
                body_text BLOB,  -- zstd compressed
                body_html BLOB,  -- zstd compressed
                FOREIGN KEY (thread_id) REFERENCES threads(id) ON DELETE CASCADE
            );

            CREATE INDEX idx_messages_thread_id ON messages(thread_id);
            CREATE INDEX idx_messages_received_at ON messages(received_at ASC);

            -- Recipients (normalized, many-to-many)
            CREATE TABLE message_recipients (
                message_id TEXT NOT NULL,
                recipient_type TEXT NOT NULL,
                name TEXT,
                email TEXT NOT NULL,
                position INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY (message_id, recipient_type, position),
                FOREIGN KEY (message_id) REFERENCES messages(id) ON DELETE CASCADE
            );

            -- Labels on messages (many-to-many)
            CREATE TABLE message_labels (
                message_id TEXT NOT NULL,
                label_id TEXT NOT NULL,
                PRIMARY KEY (message_id, label_id),
                FOREIGN KEY (message_id) REFERENCES messages(id) ON DELETE CASCADE
            );

            CREATE INDEX idx_message_labels_label ON message_labels(label_id);

            -- Thread-label index for efficient list_threads_by_label
            CREATE TABLE thread_labels (
                thread_id TEXT NOT NULL,
                label_id TEXT NOT NULL,
                last_message_at TEXT NOT NULL,
                PRIMARY KEY (thread_id, label_id),
                FOREIGN KEY (thread_id) REFERENCES threads(id) ON DELETE CASCADE
            );

            CREATE INDEX idx_thread_labels_query
                ON thread_labels(label_id, last_message_at DESC);

            -- Pending messages queue
            CREATE TABLE pending_messages (
                id TEXT PRIMARY KEY,
                data BLOB NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );

            -- Labels on pending messages for prioritization
            CREATE TABLE pending_message_labels (
                message_id TEXT NOT NULL,
                label_id TEXT NOT NULL,
                PRIMARY KEY (message_id, label_id),
                FOREIGN KEY (message_id) REFERENCES pending_messages(id) ON DELETE CASCADE
            );

            CREATE INDEX idx_pending_labels ON pending_message_labels(label_id);
            "#,
        ),
        // Migration 2: Add sync resilience fields
        M::up(
            r#"
            -- Add fetch progress checkpointing fields to sync_state
            ALTER TABLE sync_state ADD COLUMN fetch_page_token TEXT;
            ALTER TABLE sync_state ADD COLUMN messages_listed INTEGER NOT NULL DEFAULT 0;
            ALTER TABLE sync_state ADD COLUMN failed_message_ids TEXT NOT NULL DEFAULT '[]';
            "#,
        ),
    ])
}

/// SQLite-based mail storage
///
/// Uses SQLite for queryable metadata and a BlobStore for large content
/// (message bodies, attachments).
pub struct SqliteMailStore {
    conn: Mutex<Connection>,
    blob_store: Box<dyn BlobStore>,
}

impl SqliteMailStore {
    /// Create a new SQLite mail store
    ///
    /// - `db_path`: Path to the SQLite database file
    /// - `blob_store`: Storage for message bodies
    pub fn new(db_path: impl AsRef<Path>, blob_store: Box<dyn BlobStore>) -> Result<Self> {
        let mut conn = Connection::open(db_path.as_ref())
            .with_context(|| format!("Failed to open database at {:?}", db_path.as_ref()))?;

        // Configure SQLite for performance
        //
        // WAL (Write-Ahead Logging) mode:
        //   - Allows concurrent readers during writes
        //   - Faster writes (sequential IO vs random)
        //   - Better crash recovery
        //
        // SYNCHRONOUS = NORMAL:
        //   - Syncs at critical moments but not every transaction
        //   - Good balance of durability vs performance
        //   - Safe with WAL mode (WAL provides additional protection)
        //
        // cache_size = -64000:
        //   - Negative value = KB (64MB cache)
        //   - Keeps frequently accessed pages in memory
        //   - Reduces disk reads for repeated queries
        //
        // temp_store = MEMORY:
        //   - Temporary tables/indices stored in RAM
        //   - Faster sorting and temporary operations
        //
        // mmap_size = 256MB:
        //   - Memory-maps the database file
        //   - Faster reads by avoiding read() syscalls
        //   - OS page cache handles caching
        //
        // foreign_keys = ON:
        //   - Enforces referential integrity
        //   - Required for ON DELETE CASCADE to work
        conn.execute_batch(
            r#"
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            PRAGMA cache_size = -64000;
            PRAGMA temp_store = MEMORY;
            PRAGMA mmap_size = 268435456;
            PRAGMA foreign_keys = ON;
            "#,
        )?;

        // Run migrations
        migrations()
            .to_latest(&mut conn)
            .context("Failed to run database migrations")?;

        Ok(Self {
            conn: Mutex::new(conn),
            blob_store,
        })
    }

    /// Update the thread_labels denormalized index for a thread
    fn update_thread_labels(&self, conn: &Connection, thread_id: &str) -> Result<()> {
        // Get thread's last_message_at
        let last_message_at: Option<String> = conn
            .query_row(
                "SELECT last_message_at FROM threads WHERE id = ?",
                [thread_id],
                |row| row.get(0),
            )
            .optional()?;

        let Some(last_message_at) = last_message_at else {
            return Ok(());
        };

        // Get all unique labels for messages in this thread
        let mut stmt = conn.prepare(
            "SELECT DISTINCT label_id FROM message_labels
             WHERE message_id IN (SELECT id FROM messages WHERE thread_id = ?)",
        )?;

        let labels: Vec<String> = stmt
            .query_map([thread_id], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?;

        // Clear existing thread_labels for this thread
        conn.execute("DELETE FROM thread_labels WHERE thread_id = ?", [thread_id])?;

        // Insert new entries
        let mut insert_stmt = conn.prepare(
            "INSERT INTO thread_labels (thread_id, label_id, last_message_at) VALUES (?, ?, ?)",
        )?;

        for label in labels {
            insert_stmt.execute(params![thread_id, label, last_message_at])?;
        }

        Ok(())
    }

    /// Load recipients for a message
    fn load_recipients(
        &self,
        conn: &Connection,
        message_id: &str,
        recipient_type: &str,
    ) -> Result<Vec<EmailAddress>> {
        let mut stmt = conn.prepare(
            "SELECT name, email FROM message_recipients
             WHERE message_id = ? AND recipient_type = ?
             ORDER BY position",
        )?;

        let recipients = stmt
            .query_map(params![message_id, recipient_type], |row| {
                Ok(EmailAddress {
                    name: row.get(0)?,
                    email: row.get(1)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(recipients)
    }

    /// Load labels for a message
    fn load_labels(&self, conn: &Connection, message_id: &str) -> Result<Vec<String>> {
        let mut stmt = conn.prepare("SELECT label_id FROM message_labels WHERE message_id = ?")?;

        let labels = stmt
            .query_map([message_id], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(labels)
    }

    /// Save recipients for a message
    fn save_recipients(
        &self,
        conn: &Connection,
        message_id: &str,
        recipient_type: &str,
        recipients: &[EmailAddress],
    ) -> Result<()> {
        let mut stmt = conn.prepare(
            "INSERT INTO message_recipients (message_id, recipient_type, name, email, position)
             VALUES (?, ?, ?, ?, ?)",
        )?;

        for (i, addr) in recipients.iter().enumerate() {
            stmt.execute(params![
                message_id,
                recipient_type,
                addr.name,
                addr.email,
                i as i64
            ])?;
        }

        Ok(())
    }

    /// Save labels for a message
    fn save_labels(&self, conn: &Connection, message_id: &str, labels: &[String]) -> Result<()> {
        let mut stmt =
            conn.prepare("INSERT INTO message_labels (message_id, label_id) VALUES (?, ?)")?;

        for label in labels {
            stmt.execute(params![message_id, label])?;
        }

        Ok(())
    }

    /// Load a MessageMetadata from a row
    fn load_message_metadata(
        &self,
        conn: &Connection,
        message_id: &str,
    ) -> Result<Option<MessageMetadata>> {
        let row: Option<(
            String,
            String,
            Option<String>,
            String,
            String,
            String,
            String,
            i64,
            bool,
            bool,
        )> = conn
            .query_row(
                "SELECT id, thread_id, from_name, from_email, subject, body_preview,
                        received_at, internal_date, has_body_text, has_body_html
                 FROM messages WHERE id = ?",
                [message_id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                        row.get(7)?,
                        row.get(8)?,
                        row.get(9)?,
                    ))
                },
            )
            .optional()?;

        let Some((
            id,
            thread_id,
            from_name,
            from_email,
            subject,
            body_preview,
            received_at_str,
            internal_date,
            has_body_text,
            has_body_html,
        )) = row
        else {
            return Ok(None);
        };

        let to = self.load_recipients(conn, &id, "to")?;
        let cc = self.load_recipients(conn, &id, "cc")?;
        let label_ids = self.load_labels(conn, &id)?;

        let received_at = chrono::DateTime::parse_from_rfc3339(&received_at_str)
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or_else(|_| chrono::Utc::now());

        Ok(Some(MessageMetadata {
            id: MessageId::new(id),
            thread_id: ThreadId::new(thread_id),
            from: EmailAddress {
                name: from_name,
                email: from_email,
            },
            to,
            cc,
            subject,
            body_preview,
            received_at,
            internal_date,
            label_ids,
            has_body_text,
            has_body_html,
        }))
    }
}

impl MailStore for SqliteMailStore {
    fn upsert_thread(&self, thread: Thread) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        // Use ON CONFLICT DO UPDATE instead of INSERT OR REPLACE
        // INSERT OR REPLACE deletes the old row first, which triggers CASCADE
        // and deletes all messages referencing the thread!
        conn.execute(
            "INSERT INTO threads
             (id, subject, snippet, last_message_at, message_count, sender_name, sender_email, is_unread)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET
                subject = excluded.subject,
                snippet = excluded.snippet,
                last_message_at = excluded.last_message_at,
                message_count = excluded.message_count,
                sender_name = excluded.sender_name,
                sender_email = excluded.sender_email,
                is_unread = excluded.is_unread",
            params![
                thread.id.as_str(),
                thread.subject,
                thread.snippet,
                thread.last_message_at.to_rfc3339(),
                thread.message_count as i64,
                thread.sender_name,
                thread.sender_email,
                thread.is_unread,
            ],
        )?;

        Ok(())
    }

    fn upsert_message(&self, message: Message) -> Result<()> {
        // Compress bodies with zstd (level 3 = good balance of speed vs compression)
        let body_text_compressed = message
            .body_text
            .as_ref()
            .map(|text| zstd::encode_all(text.as_bytes(), 3))
            .transpose()
            .context("Failed to compress body_text")?;

        let body_html_compressed = message
            .body_html
            .as_ref()
            .map(|html| zstd::encode_all(html.as_bytes(), 3))
            .transpose()
            .context("Failed to compress body_html")?;

        let has_body_text = body_text_compressed.is_some();
        let has_body_html = body_html_compressed.is_some();

        // Update SQLite in a transaction
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;

        // Delete old recipients and labels first
        tx.execute(
            "DELETE FROM message_recipients WHERE message_id = ?",
            [message.id.as_str()],
        )?;
        tx.execute(
            "DELETE FROM message_labels WHERE message_id = ?",
            [message.id.as_str()],
        )?;

        // Insert/update message metadata with compressed bodies
        // Use ON CONFLICT DO UPDATE to avoid CASCADE delete issues
        tx.execute(
            "INSERT INTO messages
             (id, thread_id, from_name, from_email, subject, body_preview,
              received_at, internal_date, has_body_text, has_body_html,
              body_text, body_html)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET
                thread_id = excluded.thread_id,
                from_name = excluded.from_name,
                from_email = excluded.from_email,
                subject = excluded.subject,
                body_preview = excluded.body_preview,
                received_at = excluded.received_at,
                internal_date = excluded.internal_date,
                has_body_text = excluded.has_body_text,
                has_body_html = excluded.has_body_html,
                body_text = excluded.body_text,
                body_html = excluded.body_html",
            params![
                message.id.as_str(),
                message.thread_id.as_str(),
                message.from.name,
                message.from.email,
                message.subject,
                message.body_preview,
                message.received_at.to_rfc3339(),
                message.internal_date,
                has_body_text,
                has_body_html,
                body_text_compressed,
                body_html_compressed,
            ],
        )?;

        // Save recipients
        self.save_recipients(&tx, message.id.as_str(), "to", &message.to)?;
        self.save_recipients(&tx, message.id.as_str(), "cc", &message.cc)?;

        // Save labels
        self.save_labels(&tx, message.id.as_str(), &message.label_ids)?;

        // Update thread_labels index
        self.update_thread_labels(&tx, message.thread_id.as_str())?;

        tx.commit()?;
        Ok(())
    }

    fn link_message_to_thread(&self, _msg_id: &MessageId, _thread_id: &ThreadId) -> Result<()> {
        // In SQLite, the message already has thread_id as a column
        // This method is a no-op since the relationship is established in upsert_message
        Ok(())
    }

    fn get_thread(&self, id: &ThreadId) -> Result<Option<Thread>> {
        let conn = self.conn.lock().unwrap();

        let row: Option<(
            String,
            String,
            String,
            String,
            i64,
            Option<String>,
            String,
            bool,
        )> = conn
            .query_row(
                "SELECT id, subject, snippet, last_message_at, message_count,
                        sender_name, sender_email, is_unread
                 FROM threads WHERE id = ?",
                [id.as_str()],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                        row.get(7)?,
                    ))
                },
            )
            .optional()?;

        let Some((
            id,
            subject,
            snippet,
            last_message_at_str,
            message_count,
            sender_name,
            sender_email,
            is_unread,
        )) = row
        else {
            return Ok(None);
        };

        let last_message_at = chrono::DateTime::parse_from_rfc3339(&last_message_at_str)
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or_else(|_| chrono::Utc::now());

        Ok(Some(Thread {
            id: ThreadId::new(id),
            subject,
            snippet,
            last_message_at,
            message_count: message_count as usize,
            sender_name,
            sender_email,
            is_unread,
        }))
    }

    fn get_message(&self, id: &MessageId) -> Result<Option<Message>> {
        let metadata = {
            let conn = self.conn.lock().unwrap();
            self.load_message_metadata(&conn, id.as_str())?
        };

        let Some(metadata) = metadata else {
            return Ok(None);
        };

        let body = self.get_message_body(id)?.unwrap_or_default();

        Ok(Some(metadata.with_body(body)))
    }

    fn get_message_metadata(&self, id: &MessageId) -> Result<Option<MessageMetadata>> {
        let conn = self.conn.lock().unwrap();
        self.load_message_metadata(&conn, id.as_str())
    }

    fn get_message_body(&self, id: &MessageId) -> Result<Option<MessageBody>> {
        let conn = self.conn.lock().unwrap();

        let row: Option<(Option<Vec<u8>>, Option<Vec<u8>>)> = conn
            .query_row(
                "SELECT body_text, body_html FROM messages WHERE id = ?",
                [id.as_str()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;

        let Some((body_text_compressed, body_html_compressed)) = row else {
            return Ok(None);
        };

        // Decompress bodies
        let text = body_text_compressed
            .map(|data| {
                zstd::decode_all(data.as_slice())
                    .context("Failed to decompress body_text")
                    .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
            })
            .transpose()?;

        let html = body_html_compressed
            .map(|data| {
                zstd::decode_all(data.as_slice())
                    .context("Failed to decompress body_html")
                    .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
            })
            .transpose()?;

        if text.is_none() && html.is_none() {
            return Ok(None);
        }

        Ok(Some(MessageBody { text, html }))
    }

    fn list_threads(&self, limit: usize, offset: usize) -> Result<Vec<Thread>> {
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn.prepare(
            "SELECT id, subject, snippet, last_message_at, message_count,
                    sender_name, sender_email, is_unread
             FROM threads
             ORDER BY last_message_at DESC
             LIMIT ? OFFSET ?",
        )?;

        let threads = stmt
            .query_map(params![limit as i64, offset as i64], |row| {
                let last_message_at_str: String = row.get(3)?;
                let last_message_at = chrono::DateTime::parse_from_rfc3339(&last_message_at_str)
                    .map(|dt| dt.with_timezone(&chrono::Utc))
                    .unwrap_or_else(|_| chrono::Utc::now());

                Ok(Thread {
                    id: ThreadId::new(row.get::<_, String>(0)?),
                    subject: row.get(1)?,
                    snippet: row.get(2)?,
                    last_message_at,
                    message_count: row.get::<_, i64>(4)? as usize,
                    sender_name: row.get(5)?,
                    sender_email: row.get(6)?,
                    is_unread: row.get(7)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(threads)
    }

    fn list_threads_by_label(
        &self,
        label: &str,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<Thread>> {
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn.prepare(
            "SELECT t.id, t.subject, t.snippet, t.last_message_at, t.message_count,
                    t.sender_name, t.sender_email, t.is_unread
             FROM threads t
             INNER JOIN thread_labels tl ON t.id = tl.thread_id
             WHERE tl.label_id = ?
             ORDER BY tl.last_message_at DESC
             LIMIT ? OFFSET ?",
        )?;

        let threads = stmt
            .query_map(params![label, limit as i64, offset as i64], |row| {
                let last_message_at_str: String = row.get(3)?;
                let last_message_at = chrono::DateTime::parse_from_rfc3339(&last_message_at_str)
                    .map(|dt| dt.with_timezone(&chrono::Utc))
                    .unwrap_or_else(|_| chrono::Utc::now());

                Ok(Thread {
                    id: ThreadId::new(row.get::<_, String>(0)?),
                    subject: row.get(1)?,
                    snippet: row.get(2)?,
                    last_message_at,
                    message_count: row.get::<_, i64>(4)? as usize,
                    sender_name: row.get(5)?,
                    sender_email: row.get(6)?,
                    is_unread: row.get(7)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(threads)
    }

    fn list_messages_for_thread(&self, thread_id: &ThreadId) -> Result<Vec<MessageMetadata>> {
        let conn = self.conn.lock().unwrap();

        let mut stmt =
            conn.prepare("SELECT id FROM messages WHERE thread_id = ? ORDER BY received_at ASC")?;

        let message_ids: Vec<String> = stmt
            .query_map([thread_id.as_str()], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?;

        let mut messages = Vec::new();
        for id in &message_ids {
            if let Some(metadata) = self.load_message_metadata(&conn, id)? {
                messages.push(metadata);
            } else {
                log::warn!("[STORE] Failed to load metadata for message {}", id);
            }
        }

        Ok(messages)
    }

    fn list_messages_for_thread_with_bodies(&self, thread_id: &ThreadId) -> Result<Vec<Message>> {
        let metadata_list = self.list_messages_for_thread(thread_id)?;

        let mut messages = Vec::new();
        for metadata in metadata_list {
            let body = self.get_message_body(&metadata.id)?.unwrap_or_default();
            messages.push(metadata.with_body(body));
        }

        Ok(messages)
    }

    fn has_message(&self, id: &MessageId) -> Result<bool> {
        let conn = self.conn.lock().unwrap();

        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM messages WHERE id = ?",
            [id.as_str()],
            |row| row.get(0),
        )?;

        Ok(count > 0)
    }

    fn count_threads(&self) -> Result<usize> {
        let conn = self.conn.lock().unwrap();

        let count: i64 = conn.query_row("SELECT COUNT(*) FROM threads", [], |row| row.get(0))?;

        Ok(count as usize)
    }

    fn count_messages_in_thread(&self, thread_id: &ThreadId) -> Result<usize> {
        let conn = self.conn.lock().unwrap();

        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM messages WHERE thread_id = ?",
            [thread_id.as_str()],
            |row| row.get(0),
        )?;

        Ok(count as usize)
    }

    fn clear(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        conn.execute_batch(
            "DELETE FROM pending_message_labels;
             DELETE FROM pending_messages;
             DELETE FROM thread_labels;
             DELETE FROM message_labels;
             DELETE FROM message_recipients;
             DELETE FROM messages;
             DELETE FROM threads;
             DELETE FROM sync_state;",
        )?;

        self.blob_store.clear()?;

        Ok(())
    }

    fn get_sync_state(&self, account_id: &str) -> Result<Option<SyncState>> {
        let conn = self.conn.lock().unwrap();

        let row: Option<(String, String, String, u32, bool, Option<String>, i64, String)> = conn
            .query_row(
                "SELECT account_id, history_id, last_sync_at, sync_version, initial_sync_complete,
                        fetch_page_token, messages_listed, failed_message_ids
                 FROM sync_state WHERE account_id = ?",
                [account_id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                        row.get(7)?,
                    ))
                },
            )
            .optional()?;

        let Some((
            account_id,
            history_id,
            last_sync_at_str,
            sync_version,
            initial_sync_complete,
            fetch_page_token,
            messages_listed,
            failed_message_ids_json,
        )) = row
        else {
            return Ok(None);
        };

        let last_sync_at = chrono::DateTime::parse_from_rfc3339(&last_sync_at_str)
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or_else(|_| chrono::Utc::now());

        // Parse failed_message_ids from JSON
        let failed_message_ids: Vec<String> =
            serde_json::from_str(&failed_message_ids_json).unwrap_or_default();

        Ok(Some(SyncState {
            account_id,
            history_id,
            last_sync_at,
            sync_version,
            initial_sync_complete,
            fetch_page_token,
            messages_listed: messages_listed as usize,
            failed_message_ids,
        }))
    }

    fn save_sync_state(&self, state: SyncState) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        // Serialize failed_message_ids to JSON
        let failed_message_ids_json = serde_json::to_string(&state.failed_message_ids)?;

        conn.execute(
            "INSERT OR REPLACE INTO sync_state
             (account_id, history_id, last_sync_at, sync_version, initial_sync_complete,
              fetch_page_token, messages_listed, failed_message_ids)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                state.account_id,
                state.history_id,
                state.last_sync_at.to_rfc3339(),
                state.sync_version,
                state.initial_sync_complete,
                state.fetch_page_token,
                state.messages_listed as i64,
                failed_message_ids_json,
            ],
        )?;

        Ok(())
    }

    fn delete_sync_state(&self, account_id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM sync_state WHERE account_id = ?", [account_id])?;
        Ok(())
    }

    fn has_thread(&self, id: &ThreadId) -> Result<bool> {
        let conn = self.conn.lock().unwrap();

        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM threads WHERE id = ?",
            [id.as_str()],
            |row| row.get(0),
        )?;

        Ok(count > 0)
    }

    fn clear_mail_data(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        conn.execute_batch(
            "DELETE FROM thread_labels;
             DELETE FROM message_labels;
             DELETE FROM message_recipients;
             DELETE FROM messages;
             DELETE FROM threads;",
        )?;

        self.blob_store.clear()?;

        Ok(())
    }

    fn get_message_ids_for_thread(&self, thread_id: &ThreadId) -> Result<Vec<MessageId>> {
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn.prepare("SELECT id FROM messages WHERE thread_id = ?")?;

        let ids = stmt
            .query_map([thread_id.as_str()], |row| {
                Ok(MessageId::new(row.get::<_, String>(0)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(ids)
    }

    fn update_message_labels(&self, message_id: &MessageId, label_ids: Vec<String>) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;

        // Get thread_id for index update
        let thread_id: Option<String> = tx
            .query_row(
                "SELECT thread_id FROM messages WHERE id = ?",
                [message_id.as_str()],
                |row| row.get(0),
            )
            .optional()?;

        let Some(thread_id) = thread_id else {
            return Ok(()); // Message not found
        };

        // Delete old labels
        tx.execute(
            "DELETE FROM message_labels WHERE message_id = ?",
            [message_id.as_str()],
        )?;

        // Insert new labels
        let mut stmt =
            tx.prepare("INSERT INTO message_labels (message_id, label_id) VALUES (?, ?)")?;
        for label in &label_ids {
            stmt.execute(params![message_id.as_str(), label])?;
        }
        drop(stmt);

        // Update thread is_unread flag
        let any_unread: bool = tx
            .query_row(
                "SELECT EXISTS(
                    SELECT 1 FROM message_labels ml
                    JOIN messages m ON ml.message_id = m.id
                    WHERE m.thread_id = ? AND ml.label_id = 'UNREAD'
                )",
                [&thread_id],
                |row| row.get(0),
            )
            .unwrap_or(false);

        tx.execute(
            "UPDATE threads SET is_unread = ? WHERE id = ?",
            params![any_unread, thread_id],
        )?;

        // Update thread_labels index
        self.update_thread_labels(&tx, &thread_id)?;

        tx.commit()?;
        Ok(())
    }

    fn delete_message(&self, message_id: &MessageId) -> Result<()> {
        // Delete blobs first
        self.blob_store
            .delete_all_for_message(message_id.as_str())?;

        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;

        // Get thread_id before deleting
        let thread_id: Option<String> = tx
            .query_row(
                "SELECT thread_id FROM messages WHERE id = ?",
                [message_id.as_str()],
                |row| row.get(0),
            )
            .optional()?;

        // Delete message (cascades to recipients and labels)
        tx.execute("DELETE FROM messages WHERE id = ?", [message_id.as_str()])?;

        // Update thread if it still exists
        if let Some(thread_id) = thread_id {
            let remaining: i64 = tx.query_row(
                "SELECT COUNT(*) FROM messages WHERE thread_id = ?",
                [&thread_id],
                |row| row.get(0),
            )?;

            if remaining == 0 {
                // Delete thread entirely
                tx.execute("DELETE FROM threads WHERE id = ?", [&thread_id])?;
            } else {
                // Update message count
                tx.execute(
                    "UPDATE threads SET message_count = ? WHERE id = ?",
                    params![remaining, thread_id],
                )?;

                // Update thread_labels index
                self.update_thread_labels(&tx, &thread_id)?;
            }
        }

        tx.commit()?;
        Ok(())
    }

    fn store_pending_message(
        &self,
        id: &MessageId,
        data: &[u8],
        label_ids: Vec<String>,
    ) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;

        tx.execute(
            "INSERT OR REPLACE INTO pending_messages (id, data) VALUES (?, ?)",
            params![id.as_str(), data],
        )?;

        // Delete old labels first
        tx.execute(
            "DELETE FROM pending_message_labels WHERE message_id = ?",
            [id.as_str()],
        )?;

        // Insert labels
        let mut stmt =
            tx.prepare("INSERT INTO pending_message_labels (message_id, label_id) VALUES (?, ?)")?;
        for label in &label_ids {
            stmt.execute(params![id.as_str(), label])?;
        }
        drop(stmt);

        tx.commit()?;
        Ok(())
    }

    fn has_pending_message(&self, id: &MessageId) -> Result<bool> {
        let conn = self.conn.lock().unwrap();

        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM pending_messages WHERE id = ?",
            [id.as_str()],
            |row| row.get(0),
        )?;

        Ok(count > 0)
    }

    fn get_pending_messages(
        &self,
        label: Option<&str>,
        limit: usize,
    ) -> Result<Vec<PendingMessage>> {
        let conn = self.conn.lock().unwrap();

        let messages: Vec<(String, Vec<u8>)> = if let Some(label) = label {
            // Get messages with specific label
            let mut stmt = conn.prepare(
                "SELECT p.id, p.data FROM pending_messages p
                 INNER JOIN pending_message_labels pl ON p.id = pl.message_id
                 WHERE pl.label_id = ?
                 LIMIT ?",
            )?;

            stmt.query_map(params![label, limit as i64], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?
        } else {
            // Get INBOX messages first, then others
            let mut inbox_stmt = conn.prepare(
                "SELECT p.id, p.data FROM pending_messages p
                 INNER JOIN pending_message_labels pl ON p.id = pl.message_id
                 WHERE pl.label_id = 'INBOX'
                 LIMIT ?",
            )?;

            let inbox: Vec<(String, Vec<u8>)> = inbox_stmt
                .query_map(params![limit as i64], |row| Ok((row.get(0)?, row.get(1)?)))?
                .collect::<Result<Vec<_>, _>>()?;

            if inbox.len() >= limit {
                inbox
            } else {
                let remaining = limit - inbox.len();
                let inbox_ids: Vec<String> = inbox.iter().map(|(id, _)| id.clone()).collect();

                let mut other_stmt = conn.prepare(
                    "SELECT id, data FROM pending_messages
                     WHERE id NOT IN (SELECT message_id FROM pending_message_labels WHERE label_id = 'INBOX')
                     LIMIT ?",
                )?;

                let others: Vec<(String, Vec<u8>)> = other_stmt
                    .query_map(params![remaining as i64], |row| {
                        Ok((row.get(0)?, row.get(1)?))
                    })?
                    .collect::<Result<Vec<_>, _>>()?;

                let mut result = inbox;
                for item in others {
                    if !inbox_ids.contains(&item.0) {
                        result.push(item);
                    }
                }
                result
            }
        };

        // Load labels for each message
        let mut result = Vec::new();
        for (id, data) in messages {
            let mut label_stmt =
                conn.prepare("SELECT label_id FROM pending_message_labels WHERE message_id = ?")?;
            let label_ids: Vec<String> = label_stmt
                .query_map([&id], |row| row.get(0))?
                .collect::<Result<Vec<_>, _>>()?;

            result.push(PendingMessage {
                id: MessageId::new(id),
                data,
                label_ids,
            });
        }

        Ok(result)
    }

    fn delete_pending_message(&self, id: &MessageId) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        // Labels are deleted via CASCADE
        conn.execute("DELETE FROM pending_messages WHERE id = ?", [id.as_str()])?;
        Ok(())
    }

    fn count_pending_messages(&self, label: Option<&str>) -> Result<usize> {
        let conn = self.conn.lock().unwrap();

        let count: i64 = if let Some(label) = label {
            conn.query_row(
                "SELECT COUNT(DISTINCT p.id) FROM pending_messages p
                 INNER JOIN pending_message_labels pl ON p.id = pl.message_id
                 WHERE pl.label_id = ?",
                [label],
                |row| row.get(0),
            )?
        } else {
            conn.query_row("SELECT COUNT(*) FROM pending_messages", [], |row| {
                row.get(0)
            })?
        };

        Ok(count as usize)
    }

    fn clear_pending_messages(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        // Labels are deleted via CASCADE
        conn.execute("DELETE FROM pending_messages", [])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::blob_file::FileBlobStore;
    use chrono::Utc;
    use tempfile::tempdir;

    fn create_test_store() -> (SqliteMailStore, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        // Use .test.sqlite extension to clearly distinguish from production databases
        let db_path = dir.path().join("mail.test.sqlite");
        let blob_path = dir.path().join("blobs.test");

        let blob_store = Box::new(FileBlobStore::new(&blob_path).unwrap());
        let store = SqliteMailStore::new(&db_path, blob_store).unwrap();

        (store, dir)
    }

    fn make_test_thread(id: &str, subject: &str) -> Thread {
        Thread::new(
            ThreadId::new(id),
            subject.to_string(),
            "Test snippet".to_string(),
            Utc::now(),
            1,
            Some("Test User".to_string()),
            "test@example.com".to_string(),
            false,
        )
    }

    fn make_test_message(id: &str, thread_id: &str) -> Message {
        Message::builder(MessageId::new(id), ThreadId::new(thread_id))
            .from(EmailAddress::new("test@example.com"))
            .subject("Test")
            .body_preview("Test preview")
            .body_text(Some("Test body text".to_string()))
            .body_html(Some("<p>Test body HTML</p>".to_string()))
            .label_ids(vec!["INBOX".to_string(), "UNREAD".to_string()])
            .build()
    }

    #[test]
    fn test_thread_crud() {
        let (store, _dir) = create_test_store();

        let thread = make_test_thread("t1", "Test Thread");
        store.upsert_thread(thread.clone()).unwrap();

        let retrieved = store.get_thread(&ThreadId::new("t1")).unwrap().unwrap();
        assert_eq!(retrieved.subject, "Test Thread");
        assert!(store.has_thread(&ThreadId::new("t1")).unwrap());
        assert!(!store.has_thread(&ThreadId::new("t2")).unwrap());
    }

    #[test]
    fn test_message_crud() {
        let (store, _dir) = create_test_store();

        let thread = make_test_thread("t1", "Test Thread");
        store.upsert_thread(thread).unwrap();

        let message = make_test_message("m1", "t1");
        store.upsert_message(message).unwrap();

        // Test get_message (with body)
        let retrieved = store.get_message(&MessageId::new("m1")).unwrap().unwrap();
        assert_eq!(retrieved.subject, "Test");
        assert_eq!(retrieved.body_text, Some("Test body text".to_string()));
        assert_eq!(
            retrieved.body_html,
            Some("<p>Test body HTML</p>".to_string())
        );

        // Test get_message_metadata (without body)
        let metadata = store
            .get_message_metadata(&MessageId::new("m1"))
            .unwrap()
            .unwrap();
        assert_eq!(metadata.subject, "Test");
        assert!(metadata.has_body_text);
        assert!(metadata.has_body_html);

        // Test get_message_body
        let body = store
            .get_message_body(&MessageId::new("m1"))
            .unwrap()
            .unwrap();
        assert_eq!(body.text, Some("Test body text".to_string()));
        assert_eq!(body.html, Some("<p>Test body HTML</p>".to_string()));

        assert!(store.has_message(&MessageId::new("m1")).unwrap());
        assert!(!store.has_message(&MessageId::new("m2")).unwrap());
    }

    #[test]
    fn test_list_threads() {
        let (store, _dir) = create_test_store();

        for i in 0..5 {
            let thread = make_test_thread(&format!("t{}", i), &format!("Thread {}", i));
            store.upsert_thread(thread).unwrap();
        }

        let threads = store.list_threads(10, 0).unwrap();
        assert_eq!(threads.len(), 5);

        let threads = store.list_threads(2, 0).unwrap();
        assert_eq!(threads.len(), 2);

        let threads = store.list_threads(10, 3).unwrap();
        assert_eq!(threads.len(), 2);
    }

    #[test]
    fn test_list_threads_by_label() {
        let (store, _dir) = create_test_store();

        let thread = make_test_thread("t1", "Test Thread");
        store.upsert_thread(thread).unwrap();

        let message = make_test_message("m1", "t1");
        store.upsert_message(message).unwrap();

        let threads = store.list_threads_by_label("INBOX", 10, 0).unwrap();
        assert_eq!(threads.len(), 1);

        let threads = store.list_threads_by_label("SENT", 10, 0).unwrap();
        assert_eq!(threads.len(), 0);
    }

    #[test]
    fn test_sync_state() {
        let (store, _dir) = create_test_store();

        assert!(store.get_sync_state("user@gmail.com").unwrap().is_none());

        let state = SyncState::new("user@gmail.com", "12345");
        store.save_sync_state(state).unwrap();

        let retrieved = store.get_sync_state("user@gmail.com").unwrap().unwrap();
        assert_eq!(retrieved.history_id, "12345");

        store.delete_sync_state("user@gmail.com").unwrap();
        assert!(store.get_sync_state("user@gmail.com").unwrap().is_none());
    }

    #[test]
    fn test_pending_messages() {
        let (store, _dir) = create_test_store();

        let id = MessageId::new("p1");
        let data = b"raw gmail json";
        let labels = vec!["INBOX".to_string()];

        store.store_pending_message(&id, data, labels).unwrap();

        assert!(store.has_pending_message(&id).unwrap());
        assert_eq!(store.count_pending_messages(None).unwrap(), 1);
        assert_eq!(store.count_pending_messages(Some("INBOX")).unwrap(), 1);

        let pending = store.get_pending_messages(None, 10).unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].data, data);

        store.delete_pending_message(&id).unwrap();
        assert!(!store.has_pending_message(&id).unwrap());
    }

    #[test]
    fn test_delete_message() {
        let (store, _dir) = create_test_store();

        let thread = make_test_thread("t1", "Test Thread");
        store.upsert_thread(thread).unwrap();

        let message = make_test_message("m1", "t1");
        store.upsert_message(message).unwrap();

        assert!(store.has_message(&MessageId::new("m1")).unwrap());

        store.delete_message(&MessageId::new("m1")).unwrap();

        assert!(!store.has_message(&MessageId::new("m1")).unwrap());
        // Thread should also be deleted since it was the only message
        assert!(!store.has_thread(&ThreadId::new("t1")).unwrap());
    }

    #[test]
    fn test_update_labels() {
        let (store, _dir) = create_test_store();

        let thread = make_test_thread("t1", "Test Thread");
        store.upsert_thread(thread).unwrap();

        let message = make_test_message("m1", "t1");
        store.upsert_message(message).unwrap();

        // Check initial labels
        let msg = store.get_message(&MessageId::new("m1")).unwrap().unwrap();
        assert!(msg.label_ids.contains(&"UNREAD".to_string()));

        // Update labels (mark as read)
        store
            .update_message_labels(&MessageId::new("m1"), vec!["INBOX".to_string()])
            .unwrap();

        // Check updated labels
        let msg = store.get_message(&MessageId::new("m1")).unwrap().unwrap();
        assert!(!msg.label_ids.contains(&"UNREAD".to_string()));
        assert!(msg.label_ids.contains(&"INBOX".to_string()));

        // Check thread is_unread updated
        let thread = store.get_thread(&ThreadId::new("t1")).unwrap().unwrap();
        assert!(!thread.is_unread);
    }

    #[test]
    fn test_list_messages_for_thread_multiple() {
        let (store, _dir) = create_test_store();

        let thread = make_test_thread("t1", "Test Thread");
        store.upsert_thread(thread).unwrap();

        // Add 3 messages to the same thread
        for i in 1..=3 {
            let msg = Message::builder(MessageId::new(format!("m{}", i)), ThreadId::new("t1"))
                .from(EmailAddress::new(format!("sender{}@example.com", i)))
                .subject(format!("Message {}", i))
                .body_preview(format!("Preview {}", i))
                .body_text(Some(format!("Body text {}", i)))
                .body_html(Some(format!("<p>Body HTML {}</p>", i)))
                .received_at(Utc::now() + chrono::Duration::seconds(i as i64))
                .label_ids(vec!["INBOX".to_string()])
                .build();
            store.upsert_message(msg).unwrap();
        }

        // list_messages_for_thread should return all 3
        let metadata_list = store
            .list_messages_for_thread(&ThreadId::new("t1"))
            .unwrap();
        assert_eq!(
            metadata_list.len(),
            3,
            "Expected 3 messages, got {}",
            metadata_list.len()
        );

        // list_messages_for_thread_with_bodies should also return all 3
        let messages = store
            .list_messages_for_thread_with_bodies(&ThreadId::new("t1"))
            .unwrap();
        assert_eq!(
            messages.len(),
            3,
            "Expected 3 messages with bodies, got {}",
            messages.len()
        );

        // Verify each message has the correct body
        for (i, msg) in messages.iter().enumerate() {
            let expected_text = format!("Body text {}", i + 1);
            assert_eq!(msg.body_text, Some(expected_text.clone()));
        }
    }
}
