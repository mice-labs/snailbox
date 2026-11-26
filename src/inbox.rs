//! The Inbox: the SQLite store of Messages, and the operations snailbox offers on it.

use std::num::NonZeroUsize;
use std::path::Path;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use chrono::{DateTime, SecondsFormat, Utc};
use rusqlite::{params, Connection, Row, Rows, TransactionBehavior};

use crate::listing::{Cursor, LeftOut, Listing};
use crate::location;
use crate::message::{IdPrefix, Message, MessageId, NewMessage};

const DATABASE_FILE: &str = "inbox.sqlite3";

// Several agents may write to one Inbox at once, each in its own process; SQLite lets one
// write at a time, so the others wait up to this long for the lock rather than fail. Set here
// rather than left to rusqlite's default so the guarantee does not move with the dependency.
const WRITER_WAIT: Duration = Duration::from_secs(30);

// `arrival` is the Inbox's own order (ADR 0001), assigned by the insert itself.
// AUTOINCREMENT keeps it from ever reusing a number, so a Message stored later always sorts
// later. `created_at` is RFC 3339 text in UTC, since SQLite has no date type. The indexes are
// IF NOT EXISTS so an Inbox created before they existed gains them on its next open, with no
// migration step; without the one on parent each Downthread step of a thread filter scans
// the table.
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
    CREATE INDEX IF NOT EXISTS messages_by_parent ON messages (parent);
    CREATE INDEX IF NOT EXISTS messages_by_sender ON messages (sender);
    CREATE INDEX IF NOT EXISTS receivers_by_name ON receivers (name);
";

pub struct Inbox {
    connection: Connection,
}

/// No Message has an id starting with `prefix`.
#[derive(Debug)]
pub struct NoMessage {
    prefix: IdPrefix,
}

impl std::fmt::Display for NoMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "no Message has an id starting with {}", self.prefix)
    }
}

impl std::error::Error for NoMessage {}

/// The inputs that name a Message by id or prefix: the `ListFilter` id fields, the Message
/// `show` prints and the parent a new Message replies to.
#[derive(Debug, Clone, Copy)]
pub enum IdInput {
    After,
    Before,
    Upthread,
    Downthread,
    Thread,
    Show,
    ReplyTo,
}

/// An id input named a prefix no Message matches; a caller downcasts to it to add a hint
/// naming the input.
#[derive(Debug)]
pub struct NoMessageFor {
    pub input: IdInput,
    pub no_message: NoMessage,
}

impl std::fmt::Display for NoMessageFor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.no_message.fmt(f)
    }
}

impl std::error::Error for NoMessageFor {}

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
            .map(|prefix| self.resolve_input(prefix, IdInput::ReplyTo))
            .transpose()?
            .map(|parent| parent.id.to_string());
        let id = MessageId::random();
        let created_at = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
        // Immediate makes the lock point explicit: the write lock is taken at BEGIN, where a
        // busy writer waits out WRITER_WAIT.
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

    /// The one Message whose id starts with `prefix` (ADR 0001); fails when no Message or
    /// several do, listing the several.
    fn resolve(&self, prefix: &IdPrefix) -> Result<Message> {
        let mut statement = self.connection.prepare(RESOLVE_SQL)?;
        let mut matches = read_messages(statement.query([prefix.as_str()])?, |_| Ok(()))?;
        match matches.len() {
            0 => Err(NoMessage {
                prefix: prefix.clone(),
            }
            .into()),
            1 => Ok(matches.remove(0)),
            _ => bail!(
                "the id prefix {prefix} matches several Messages:\n{}",
                matches
                    .iter()
                    .map(|message| message.id.to_string())
                    .collect::<Vec<_>>()
                    .join("\n")
            ),
        }
    }

    /// `resolve` for an id input, whose no-match error names `input`.
    fn resolve_input(&self, prefix: &IdPrefix, input: IdInput) -> Result<Message> {
        self.resolve(prefix)
            .map_err(|error| match error.downcast::<NoMessage>() {
                Ok(no_message) => NoMessageFor { input, no_message }.into(),
                Err(error) => error,
            })
    }

    /// The full id of the Message `prefix` names, or none when there is no `prefix`; fails as
    /// `resolve_input` does when no Message matches.
    fn full_id_of(&self, prefix: &Option<IdPrefix>, input: IdInput) -> Result<Option<String>> {
        prefix
            .as_ref()
            .map(|prefix| {
                self.resolve_input(prefix, input)
                    .map(|message| message.id.to_string())
            })
            .transpose()
    }

    /// The Message whose id is, or is the only one to start with, `prefix`; fails when no
    /// Message does or several do.
    pub fn show(&self, prefix: &IdPrefix) -> Result<Message> {
        self.resolve_input(prefix, IdInput::Show)
    }

    /// The Messages matching `filter`, in the order this Inbox stored them, oldest first
    /// (ADR 0001); creation time plays no part in the order. When the limit cuts the matches,
    /// the oldest are kept if `after` is given and the newest otherwise, and the listing says
    /// what was left out and where to continue.
    pub fn list(&self, filter: &ListFilter) -> Result<Listing> {
        let after = self.full_id_of(&filter.after, IdInput::After)?;
        let before = self.full_id_of(&filter.before, IdInput::Before)?;
        let upthread = self.full_id_of(&filter.upthread, IdInput::Upthread)?;
        let downthread = self.full_id_of(&filter.downthread, IdInput::Downthread)?;
        let thread = self.full_id_of(&filter.thread, IdInput::Thread)?;
        let keeps_oldest = after.is_some();
        let sql = if keeps_oldest {
            LIST_OLDEST_SQL
        } else {
            LIST_NEWEST_SQL
        };
        let mut statement = self.connection.prepare(sql)?;
        let rows = statement.query(params![
            filter.from,
            filter.to,
            after,
            before,
            upthread,
            downthread,
            thread,
            filter.limit.map(NonZeroUsize::get),
        ])?;
        let mut matched = 0;
        let messages = read_messages(rows, |row| {
            matched = row.get("matched")?;
            Ok(())
        })?;
        let count = matched - messages.len();
        // A non-zero limit that left Messages out kept at least one.
        let left_out = (count > 0).then(|| LeftOut {
            count,
            cursor: if keeps_oldest {
                Cursor::After(messages.last().expect("a kept Message").id.clone())
            } else {
                Cursor::Before(messages.first().expect("a kept Message").id.clone())
            },
        });
        Ok(Listing { messages, left_out })
    }
}

/// The Messages `rows` hold, each read by `message_from_row`, with `also` run on each row for
/// any column beyond the Message's own.
fn read_messages(
    mut rows: Rows<'_>,
    mut also: impl FnMut(&Row<'_>) -> rusqlite::Result<()>,
) -> Result<Vec<Message>> {
    let mut messages = Vec::new();
    while let Some(row) = rows.next()? {
        also(row)?;
        messages.push(message_from_row(row)?);
    }
    Ok(messages)
}

/// The Message a row of `select_message!` holds, its Receivers read from the JSON array the
/// query aggregates.
fn message_from_row(row: &Row<'_>) -> Result<Message> {
    let id: String = row.get("id")?;
    let parent: Option<String> = row.get("parent")?;
    let receivers: String = row.get("receivers")?;
    Ok(Message {
        id: id
            .parse()
            .with_context(|| format!("the Inbox holds a malformed id {id}"))?,
        sender: row.get("sender")?,
        receivers: serde_json::from_str(&receivers)
            .with_context(|| format!("the Inbox holds malformed Receivers {receivers}"))?,
        subject: row.get("subject")?,
        body: row.get("body")?,
        parent: parent
            .map(|parent| {
                parent
                    .parse()
                    .with_context(|| format!("the Inbox holds a malformed parent {parent}"))
            })
            .transpose()?,
        created_at: created_at_from_row(row)?,
    })
}

/// The row's creation time, read as strict RFC 3339; a stored text that is not one fails with
/// an error quoting it, and a value that is not text fails with rusqlite's column type error.
/// Parsed here rather than by rusqlite's chrono `FromSql`, which also accepts a time with no
/// offset.
fn created_at_from_row(row: &Row<'_>) -> Result<DateTime<Utc>> {
    let stored: String = row.get("created_at")?;
    Ok(DateTime::parse_from_rfc3339(&stored)
        .with_context(|| format!("the Inbox holds a malformed time {stored}"))?
        .with_timezone(&Utc))
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
    /// Matches only the Message with this id or prefix and the Messages it replies to, at any
    /// depth.
    pub upthread: Option<IdPrefix>,
    /// Matches only the Message with this id or prefix and the Replies beneath it, at any
    /// depth.
    pub downthread: Option<IdPrefix>,
    /// Matches only the Message with this id or prefix with everything upthread and downthread
    /// of it.
    pub thread: Option<IdPrefix>,
    /// The most Messages to return; `None` returns every match.
    pub limit: Option<NonZeroUsize>,
}

/// Prepends the SELECT that reads a whole Message, its Receivers aggregated in position order
/// as a JSON array: a Receiver name can hold a comma, so a separator-joined string would not
/// split back (snailbox-6vk). The ORDER BY inside json_group_array needs SQLite 3.44 or later,
/// which holds because rusqlite's `bundled` feature (Cargo.toml) compiles in its own newer
/// SQLite; linked against an older system SQLite the query would fail to prepare. `format!`
/// cannot build a const, so the join is `concat!`, which takes literals only.
macro_rules! select_message {
    ($tail:literal) => {
        concat!(
            "SELECT messages.id, messages.sender, messages.subject, messages.body, ",
            "messages.created_at, messages.parent, ",
            "(SELECT json_group_array(name ORDER BY position) FROM receivers ",
            "WHERE receivers.arrival = messages.arrival) AS receivers ",
            $tail
        )
    };
}

// A prefix match written as a range on id, the form the unique index on id serves. Under the
// BINARY collation id has (SCHEMA gives it no other), U+10FFFF sorts after every character an
// id holds, so the range ends past every id with the prefix; a collation that ordered
// otherwise would need another upper bound.
const RESOLVE_SQL: &str =
    select_message!("FROM messages WHERE id >= ?1 AND id < ?1 || char(1114111) ORDER BY arrival");

/// The `list` query: ?1 Sender, ?2 Receiver, ?3 and ?4 the full ids of the --after and
/// --before Messages, ?5, ?6 and ?7 the full ids that start the --upthread, --downthread and
/// --thread filters, ?8 the limit; NULL leaves each out. `upthread` walks from each start to
/// the Messages it replies to and `downthread` to the Replies beneath it, each row tagged with
/// the flag it started from, so several thread flags given together each narrow the result.
/// `kept` cuts the matches to the limit from the end `$order` names, and `matched` counts
/// every match, so the caller learns how many the limit left out.
macro_rules! list_sql {
    ($order:literal) => {
        concat!(
            "WITH RECURSIVE
             upthread(flag, id) AS (
                 SELECT 'upthread', ?5 WHERE ?5 IS NOT NULL
                 UNION SELECT 'thread', ?7 WHERE ?7 IS NOT NULL
                 UNION SELECT upthread.flag, messages.parent FROM messages
                       JOIN upthread ON messages.id = upthread.id
                       WHERE messages.parent IS NOT NULL),
             downthread(flag, id) AS (
                 SELECT 'downthread', ?6 WHERE ?6 IS NOT NULL
                 UNION SELECT 'thread', ?7 WHERE ?7 IS NOT NULL
                 UNION SELECT downthread.flag, messages.id FROM messages
                       JOIN downthread ON messages.parent = downthread.id),
             kept(arrival, matched) AS (
                 SELECT arrival, count(*) OVER () FROM messages
                 WHERE (?1 IS NULL OR sender = ?1)
                   AND (?2 IS NULL OR EXISTS (
                        SELECT 1 FROM receivers
                        WHERE receivers.arrival = messages.arrival AND receivers.name = ?2))
                   AND (?3 IS NULL OR arrival > (SELECT arrival FROM messages WHERE id = ?3))
                   AND (?4 IS NULL OR arrival < (SELECT arrival FROM messages WHERE id = ?4))
                   AND (?5 IS NULL OR id IN (SELECT id FROM upthread WHERE flag = 'upthread'))
                   AND (?6 IS NULL OR id IN (SELECT id FROM downthread WHERE flag = 'downthread'))
                   AND (?7 IS NULL
                        OR id IN (SELECT id FROM upthread WHERE flag = 'thread')
                        OR id IN (SELECT id FROM downthread WHERE flag = 'thread'))
                 ORDER BY arrival ",
            $order,
            " LIMIT coalesce(?8, -1)) ",
            select_message!(
                ", kept.matched FROM kept JOIN messages ON messages.arrival = kept.arrival
                 ORDER BY messages.arrival"
            )
        )
    };
}

const LIST_OLDEST_SQL: &str = list_sql!("ASC");
const LIST_NEWEST_SQL: &str = list_sql!("DESC");
