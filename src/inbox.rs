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
        let transaction = self
            .connection
            .transaction()
            .context("cannot store the Message")?;
        transaction
            .execute(
                "INSERT INTO messages (id, sender, subject, body, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    id.to_string(),
                    new.sender,
                    new.subject,
                    new.body,
                    created_at
                ],
            )
            .context("cannot store the Message")?;
        let arrival = transaction.last_insert_rowid();
        for (position, name) in new.receivers.iter().enumerate() {
            transaction
                .execute(
                    "INSERT INTO receivers (arrival, position, name) VALUES (?1, ?2, ?3)",
                    params![arrival, position, name],
                )
                .with_context(|| format!("cannot store the Message's Receiver {name}"))?;
        }
        transaction.commit().context("cannot store the Message")?;
        Ok(id)
    }

    /// The Message whose full id is `id`.
    pub fn show(&self, id: &str) -> Result<Message> {
        let row = self
            .connection
            .query_row(
                &format!("{SELECT_MESSAGE} WHERE id = ?1"),
                [id],
                StoredMessage::from_row,
            )
            .optional()?;
        let Some(row) = row else {
            bail!("no Message has the id {id}");
        };
        self.message(row)
    }

    /// The Messages matching `filter`, in the order this Inbox stored them, oldest first
    /// (ADR 0001); creation time plays no part in the order.
    pub fn list(&self, filter: &ListFilter) -> Result<Vec<Message>> {
        let mut statement = self.connection.prepare(&format!(
            "{SELECT_MESSAGE}
             WHERE (?1 IS NULL OR sender = ?1)
               AND (?2 IS NULL OR EXISTS (
                    SELECT 1 FROM receivers
                    WHERE receivers.arrival = messages.arrival AND receivers.name = ?2))
             ORDER BY arrival"
        ))?;
        let rows = statement
            .query_map(params![filter.from, filter.to], StoredMessage::from_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows.into_iter().map(|row| self.message(row)).collect()
    }

    fn message(&self, row: StoredMessage) -> Result<Message> {
        let mut statement = self
            .connection
            .prepare("SELECT name FROM receivers WHERE arrival = ?1 ORDER BY position")?;
        let receivers = statement
            .query_map([row.arrival], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(Message {
            id: row
                .id
                .parse()
                .with_context(|| format!("the Inbox holds a malformed id {}", row.id))?,
            sender: row.sender,
            receivers,
            subject: row.subject,
            body: row.body,
            created_at: DateTime::parse_from_rfc3339(&row.created_at)
                .with_context(|| format!("the Inbox holds a malformed time {}", row.created_at))?
                .with_timezone(&Utc),
        })
    }
}

/// Which Messages `list` returns; a field left `None` does not narrow the result.
pub struct ListFilter {
    /// Matches a Message that has this name among its Receivers.
    pub to: Option<String>,
    /// Matches a Message whose Sender is this name.
    pub from: Option<String>,
}

const SELECT_MESSAGE: &str = "SELECT arrival, id, sender, subject, body, created_at FROM messages";

/// A row of `messages` as stored, before its Receivers are attached.
struct StoredMessage {
    arrival: i64,
    id: String,
    sender: String,
    subject: String,
    body: Option<String>,
    created_at: String,
}

impl StoredMessage {
    fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            arrival: row.get(0)?,
            id: row.get(1)?,
            sender: row.get(2)?,
            subject: row.get(3)?,
            body: row.get(4)?,
            created_at: row.get(5)?,
        })
    }
}
