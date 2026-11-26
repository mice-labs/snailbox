//! The Inbox: the SQLite store of Messages, and the operations snailbox offers on it.

use std::path::Path;

use anyhow::{bail, Context, Result};
use chrono::{DateTime, SecondsFormat, Utc};
use rusqlite::{params, Connection};

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
        created_at TEXT NOT NULL,
        parent     TEXT REFERENCES messages (id)
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
        let parent = new
            .reply_to
            .as_deref()
            .map(|prefix| self.resolve(prefix))
            .transpose()?;
        let id = MessageId::random();
        let created_at = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
        let transaction = self
            .connection
            .transaction()
            .context("cannot store the Message")?;
        transaction
            .execute(
                "INSERT INTO messages (id, sender, subject, body, created_at, parent)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    id.to_string(),
                    new.sender,
                    new.subject,
                    new.body,
                    created_at,
                    parent
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

    /// The full id of the one Message whose id starts with `prefix` (ADR 0001); fails when no
    /// Message or several do, listing the several.
    fn resolve(&self, prefix: &str) -> Result<String> {
        let mut statement = self.connection.prepare(
            "SELECT id FROM messages WHERE substr(id, 1, length(?1)) = ?1 ORDER BY arrival",
        )?;
        let mut matches = statement
            .query_map([prefix], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        match matches.len() {
            0 => bail!("no Message has the id {prefix}"),
            1 => Ok(matches.remove(0)),
            _ => bail!(
                "the id prefix {prefix} matches several Messages:\n{}",
                matches.join("\n")
            ),
        }
    }

    /// The Message whose id is `id` or starts with it.
    pub fn show(&self, id: &str) -> Result<Message> {
        let id = self.resolve(id)?;
        let (arrival, stored_id, sender, subject, body, created_at, parent) =
            self.connection.query_row(
                "SELECT arrival, id, sender, subject, body, created_at, parent
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
                        row.get::<_, Option<String>>(6)?,
                    ))
                },
            )?;
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
            parent: parent
                .map(|parent| {
                    parent
                        .parse()
                        .with_context(|| format!("the Inbox holds a malformed parent {parent}"))
                })
                .transpose()?,
            created_at: DateTime::parse_from_rfc3339(&created_at)
                .with_context(|| format!("the Inbox holds a malformed time {created_at}"))?
                .with_timezone(&Utc),
        })
    }
}
