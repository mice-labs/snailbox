//! The Inbox: the SQLite store of Messages, and the operations snailbox offers on it.

use std::path::Path;

use anyhow::{bail, Context, Result};
use chrono::{DateTime, SecondsFormat, Utc};
use rusqlite::{params, Connection, OptionalExtension};

use crate::location;
use crate::message::{Message, MessageId, NewMessage};

const DATABASE_FILE: &str = "inbox.sqlite3";

// `arrival` is the Inbox's own order (ADR 0001), assigned by the insert itself.
// AUTOINCREMENT keeps it from ever reusing a number, so a Message stored later always sorts
// later.
const SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS messages (
        arrival    INTEGER PRIMARY KEY AUTOINCREMENT,
        id         TEXT NOT NULL UNIQUE,
        sender     TEXT NOT NULL,
        subject    TEXT NOT NULL,
        body       TEXT,
        created_at TEXT NOT NULL
    );
    CREATE TABLE IF NOT EXISTS receivers (
        arrival  INTEGER NOT NULL REFERENCES messages (arrival),
        position INTEGER NOT NULL,
        name     TEXT NOT NULL,
        PRIMARY KEY (arrival, position)
    );
";

pub struct Inbox {
    connection: Connection,
}

impl Inbox {
    /// Opens the Inbox in `folder`, creating the folder and database on first use.
    pub fn open(folder: &Path) -> Result<Self> {
        location::ensure_folder(folder)?;
        let path = folder.join(DATABASE_FILE);
        let connection = Connection::open(&path)
            .with_context(|| format!("cannot open the Inbox at {}", path.display()))?;
        connection
            .execute_batch(SCHEMA)
            .context("cannot set up the Inbox")?;
        Ok(Self { connection })
    }

    /// Stores `new` with a fresh id and the current time, and returns the id.
    pub fn create(&mut self, new: &NewMessage) -> Result<MessageId> {
        let id = MessageId::random();
        let created_at = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
        let transaction = self.connection.transaction()?;
        transaction.execute(
            "INSERT INTO messages (id, sender, subject, body, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                id.to_string(),
                new.sender,
                new.subject,
                new.body,
                created_at
            ],
        )?;
        let arrival = transaction.last_insert_rowid();
        for (position, name) in new.receivers.iter().enumerate() {
            transaction.execute(
                "INSERT INTO receivers (arrival, position, name) VALUES (?1, ?2, ?3)",
                params![arrival, position, name],
            )?;
        }
        transaction.commit().context("cannot store the Message")?;
        Ok(id)
    }

    /// The Message whose full id is `id`.
    pub fn show(&self, id: &str) -> Result<Message> {
        let row = self
            .connection
            .query_row(
                "SELECT arrival, id, sender, subject, body, created_at
                 FROM messages WHERE id = ?1",
                [id],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, Option<String>>(4)?,
                        row.get::<_, String>(5)?,
                    ))
                },
            )
            .optional()?;
        let Some((arrival, stored_id, sender, subject, body, created_at)) = row else {
            bail!("no Message has the id {id}");
        };
        let mut statement = self
            .connection
            .prepare("SELECT name FROM receivers WHERE arrival = ?1 ORDER BY position")?;
        let receivers = statement
            .query_map([arrival], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(Message {
            id: stored_id
                .parse()
                .with_context(|| format!("the Inbox holds a malformed id {stored_id}"))?,
            sender,
            receivers,
            subject,
            body,
            created_at: DateTime::parse_from_rfc3339(&created_at)
                .with_context(|| format!("the Inbox holds a malformed time {created_at}"))?
                .with_timezone(&Utc),
        })
    }
}
