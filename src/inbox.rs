//! The Inbox: the SQLite store of Messages, and the operations snailbox offers on it.

use std::num::NonZeroUsize;
use std::path::Path;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use chrono::{DateTime, SecondsFormat, Utc};
use rusqlite::{params, Connection, TransactionBehavior};

use crate::location;
use crate::message::{IdPrefix, Message, MessageId, NewMessage};

const DATABASE_FILE: &str = "inbox.sqlite3";

// Several agents may write to one Inbox at once, each in its own process; SQLite lets one
// write at a time, so the others wait up to this long for the lock rather than fail. Set here
// rather than left to rusqlite's default so the guarantee does not move with the dependency.
const WRITER_WAIT: Duration = Duration::from_secs(30);

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

/// No Message has an id starting with `prefix`; a caller downcasts to it to add a hint.
#[derive(Debug)]
pub struct NoMessage {
    pub prefix: IdPrefix,
}

impl std::fmt::Display for NoMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "no Message has an id starting with {}", self.prefix)
    }
}

impl std::error::Error for NoMessage {}

impl Inbox {
    /// Opens the Inbox in `folder`, creating the folder and database on first use.
    pub fn open(folder: &Path) -> Result<Self> {
        location::ensure_folder(folder)?;
        let path = folder.join(DATABASE_FILE);
        let connection = Connection::open(&path)
            .with_context(|| format!("cannot open the Inbox at {}", path.display()))?;
        connection
            .busy_timeout(WRITER_WAIT)
            .context("cannot set up the Inbox")?;
        connection
            .execute_batch(SCHEMA)
            .context("cannot set up the Inbox")?;
        Ok(Self { connection })
    }

    /// Stores `new` with a fresh id and the current time, and returns the id.
    pub fn create(&mut self, new: &NewMessage) -> Result<MessageId> {
        let parent = new
            .reply_to
            .as_ref()
            .map(|prefix| self.resolve(prefix))
            .transpose()?
            .map(|parent| parent.to_string());
        let id = MessageId::random();
        let created_at = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
        // Immediate takes the write lock at BEGIN, where a busy writer waits out WRITER_WAIT; a
        // deferred transaction that reads before it writes can instead fail at once.
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
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
    fn resolve(&self, prefix: &IdPrefix) -> Result<MessageId> {
        let mut statement = self.connection.prepare(
            "SELECT id FROM messages WHERE substr(id, 1, length(?1)) = ?1 ORDER BY arrival",
        )?;
        let mut matches = statement
            .query_map([prefix.as_str()], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        match matches.len() {
            0 => Err(NoMessage {
                prefix: prefix.clone(),
            }
            .into()),
            1 => {
                let stored = matches.remove(0);
                stored
                    .parse()
                    .with_context(|| format!("the Inbox holds a malformed id {stored}"))
            }
            _ => bail!(
                "the id prefix {prefix} matches several Messages:\n{}",
                matches.join("\n")
            ),
        }
    }

    /// The Message whose id is, or is the only one to start with, `prefix`; fails when no
    /// Message does or several do.
    pub fn show(&self, prefix: &IdPrefix) -> Result<Message> {
        let id = self.resolve(prefix)?;
        let row = self.connection.query_row(
            &format!("{SELECT_MESSAGE} WHERE id = ?1"),
            [id.to_string()],
            StoredMessage::from_row,
        )?;
        self.with_receivers(row)
    }

    /// The Messages matching `filter`, in the order this Inbox stored them, oldest first
    /// (ADR 0001); creation time plays no part in the order. When the limit cuts the matches,
    /// the oldest are kept if `after` is given and the newest otherwise, and the listing says
    /// what was left out and where to continue.
    pub fn list(&self, filter: &ListFilter) -> Result<Listing> {
        let after = filter
            .after
            .as_ref()
            .map(|prefix| self.arrival_of(prefix))
            .transpose()?;
        let before = filter
            .before
            .as_ref()
            .map(|prefix| self.arrival_of(prefix))
            .transpose()?;
        let mut statement = self.connection.prepare(&format!(
            "{SELECT_MESSAGE}
             WHERE (?1 IS NULL OR sender = ?1)
               AND (?2 IS NULL OR EXISTS (
                    SELECT 1 FROM receivers
                    WHERE receivers.arrival = messages.arrival AND receivers.name = ?2))
               AND (?3 IS NULL OR arrival > ?3)
               AND (?4 IS NULL OR arrival < ?4)
             ORDER BY arrival"
        ))?;
        let mut rows = statement
            .query_map(
                params![filter.from, filter.to, after, before],
                StoredMessage::from_row,
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let excess = filter
            .limit
            .map_or(0, |limit| rows.len().saturating_sub(limit.get()));
        let keeps_oldest = after.is_some();
        if keeps_oldest {
            rows.truncate(rows.len() - excess);
        } else {
            rows.drain(..excess);
        }
        let messages = rows
            .into_iter()
            .map(|row| self.with_receivers(row))
            .collect::<Result<Vec<_>>>()?;
        // A non-zero limit that left Messages out kept at least one.
        let left_out = match (excess, keeps_oldest) {
            (0, _) => None,
            (count, true) => Some(LeftOut::Newer {
                count,
                after: messages.last().expect("a kept Message").id.clone(),
            }),
            (count, false) => Some(LeftOut::Older {
                count,
                before: messages.first().expect("a kept Message").id.clone(),
            }),
        };
        Ok(Listing { messages, left_out })
    }

    /// The arrival number of the one Message whose id starts with `prefix`.
    fn arrival_of(&self, prefix: &IdPrefix) -> Result<i64> {
        let id = self.resolve(prefix)?;
        Ok(self.connection.query_row(
            "SELECT arrival FROM messages WHERE id = ?1",
            [id.to_string()],
            |row| row.get(0),
        )?)
    }

    /// The Message `row` stores, with its Receivers read from the `receivers` table.
    fn with_receivers(&self, row: StoredMessage) -> Result<Message> {
        let mut statement = self
            .connection
            .prepare_cached("SELECT name FROM receivers WHERE arrival = ?1 ORDER BY position")?;
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
            parent: row
                .parent
                .map(|parent| {
                    parent
                        .parse()
                        .with_context(|| format!("the Inbox holds a malformed parent {parent}"))
                })
                .transpose()?,
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
    /// Matches only Messages this Inbox stored after the Message with this id or prefix.
    pub after: Option<IdPrefix>,
    /// Matches only Messages this Inbox stored before the Message with this id or prefix.
    pub before: Option<IdPrefix>,
    /// The most Messages to return; `None` returns every match.
    pub limit: Option<NonZeroUsize>,
}

/// What `list` found: the Messages returned, oldest first, and what the limit cut.
pub struct Listing {
    pub messages: Vec<Message>,
    /// `None` when the limit cut nothing.
    pub left_out: Option<LeftOut>,
}

/// The Messages a limit cut from a listing, and the cursor that continues from the ones kept.
pub enum LeftOut {
    /// `count` Messages stored before the first one returned; `before` is that Message's id.
    Older { count: usize, before: MessageId },
    /// `count` Messages stored after the last one returned; `after` is that Message's id.
    Newer { count: usize, after: MessageId },
}

impl LeftOut {
    pub fn count(&self) -> usize {
        match self {
            Self::Older { count, .. } | Self::Newer { count, .. } => *count,
        }
    }
}

const SELECT_MESSAGE: &str =
    "SELECT arrival, id, sender, subject, body, created_at, parent FROM messages";

/// A row of `messages` as stored, before its Receivers are attached.
struct StoredMessage {
    arrival: i64,
    id: String,
    sender: String,
    subject: String,
    body: Option<String>,
    created_at: String,
    parent: Option<String>,
}

impl StoredMessage {
    fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            arrival: row.get("arrival")?,
            id: row.get("id")?,
            sender: row.get("sender")?,
            subject: row.get("subject")?,
            body: row.get("body")?,
            created_at: row.get("created_at")?,
            parent: row.get("parent")?,
        })
    }
}
