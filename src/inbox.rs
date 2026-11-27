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

// `seq` is the Inbox's own sequence number (ADR 0001), assigned by the insert itself.
// AUTOINCREMENT keeps it from ever reusing a number, so a Message stored later always sorts
// later. `created_at` is RFC 3339 text in UTC, since SQLite has no date type. The indexes are
// IF NOT EXISTS so an Inbox created before they existed gains them on its next open, with no
// migration step; without the one on parent each Downthread step of a thread filter scans
// the table.
const SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS messages (
        seq        INTEGER PRIMARY KEY AUTOINCREMENT,
        id         TEXT NOT NULL UNIQUE,
        sender     TEXT NOT NULL,
        subject    TEXT NOT NULL,
        body       TEXT,
        created_at TEXT NOT NULL,
        parent     TEXT REFERENCES messages (id)
    );
    CREATE TABLE IF NOT EXISTS receivers (
        seq      INTEGER NOT NULL REFERENCES messages (seq),
        position INTEGER NOT NULL,
        name     TEXT NOT NULL,
        PRIMARY KEY (seq, position)
    );
    CREATE INDEX IF NOT EXISTS messages_by_parent ON messages (parent);
    CREATE INDEX IF NOT EXISTS messages_by_sender ON messages (sender);
    CREATE INDEX IF NOT EXISTS receivers_by_name ON receivers (name);
";

/// Renames the `arrival` columns an Inbox created before the sequence number was named `seq`
/// still has, keeping every stored value; SCHEMA's `IF NOT EXISTS` would leave them. An Inbox
/// already renamed is left alone without taking the write lock, so `list` does not wait for
/// writers. Otherwise the check is repeated inside the write transaction that renames, so two
/// processes opening an old Inbox at once rename it once. SQLite's RENAME COLUMN also rewrites
/// the `receivers` reference to `messages (arrival)`, which holds only while the connection's
/// `legacy_alter_table` is off (SQLite's default, and this code never turns it on); with it on,
/// the reference would keep naming the old column.
fn rename_arrival_columns(connection: &mut Connection) -> Result<()> {
    const TABLES: [&str; 2] = ["messages", "receivers"];
    let has_arrival = |connection: &Connection, table: &str| -> rusqlite::Result<bool> {
        connection.query_row(
            "SELECT EXISTS (SELECT 1 FROM pragma_table_info(?1) WHERE name = 'arrival')",
            [table],
            |row| row.get(0),
        )
    };
    let mut needs_rename = false;
    for table in TABLES {
        needs_rename |= has_arrival(connection, table)?;
    }
    if !needs_rename {
        return Ok(());
    }
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    for table in TABLES {
        if has_arrival(&transaction, table)? {
            transaction
                .execute_batch(&format!("ALTER TABLE {table} RENAME COLUMN arrival TO seq"))?;
        }
    }
    Ok(transaction.commit()?)
}

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
        let mut connection = Connection::open(&path)
            .with_context(|| format!("cannot open the Inbox at {}", path.display()))?;
        connection
            .busy_timeout(WRITER_WAIT)
            .context("cannot set up the Inbox")?;
        rename_arrival_columns(&mut connection).context("cannot set up the Inbox")?;
        connection
            .execute_batch(SCHEMA)
            .context("cannot set up the Inbox")?;
        Ok(Self { connection })
    }

    /// Stores `new` with a fresh id and the current time, and returns the id.
    pub fn create(&mut self, new: &NewMessage) -> Result<MessageId> {
        let parent = self.full_id_of(&new.reply_to, IdInput::ReplyTo)?;
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
        let seq = transaction.last_insert_rowid();
        let receivers =
            serde_json::to_string(&new.receivers).context("cannot store the Message")?;
        transaction
            .execute(INSERT_RECEIVERS_SQL, params![seq, receivers])
            .context("cannot store the Message")?;
        transaction.commit().context("cannot store the Message")?;
        Ok(id)
    }

    /// The one Message whose id starts with `prefix` (ADR 0001); fails when no Message or
    /// several do, listing the several.
    fn resolve(&self, prefix: &IdPrefix) -> Result<Message> {
        let mut statement = self.connection.prepare(RESOLVE_SQL)?;
        let matches = read_messages(statement.query([prefix.as_str()])?, |_| Ok(()))?;
        one_of(prefix, matches, |message| message.id.to_string())
    }

    /// `resolve` for the id alone, read by `RESOLVE_ID_SQL`, which loads no Message body or
    /// Receiver.
    fn resolve_id(&self, prefix: &IdPrefix) -> Result<String> {
        let mut statement = self.connection.prepare(RESOLVE_ID_SQL)?;
        let ids = statement
            .query_map([prefix.as_str()], |row| row.get("id"))?
            .collect::<rusqlite::Result<Vec<String>>>()?;
        one_of(prefix, ids, |id| id.clone())
    }

    /// `resolve` for an id input, whose no-match error names `input`.
    fn resolve_input(&self, prefix: &IdPrefix, input: IdInput) -> Result<Message> {
        named_by(self.resolve(prefix), input)
    }

    /// `resolve_id` for an id input, whose no-match error names `input`.
    fn resolve_id_input(&self, prefix: &IdPrefix, input: IdInput) -> Result<String> {
        named_by(self.resolve_id(prefix), input)
    }

    /// The full id of the Message `prefix` names, or none when there is no `prefix`; fails as
    /// `resolve_id_input` does when no Message matches.
    fn full_id_of(&self, prefix: &Option<IdPrefix>, input: IdInput) -> Result<Option<String>> {
        prefix
            .as_ref()
            .map(|prefix| self.resolve_id_input(prefix, input))
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

/// The one of `matches` that the id prefix `prefix` names: fails when none does, or when several
/// do, listing their ids as `id_of` gives them. Both resolvers decide 0, 1 or several here.
fn one_of<T>(prefix: &IdPrefix, mut matches: Vec<T>, id_of: impl Fn(&T) -> String) -> Result<T> {
    match matches.len() {
        0 => Err(NoMessage {
            prefix: prefix.clone(),
        }
        .into()),
        1 => Ok(matches.remove(0)),
        _ => bail!(
            "the id prefix {prefix} matches several Messages:\n{}",
            matches.iter().map(id_of).collect::<Vec<_>>().join("\n")
        ),
    }
}

/// `result`, its no-match error named for `input`.
fn named_by<T>(result: Result<T>, input: IdInput) -> Result<T> {
    result.map_err(|error| match error.downcast::<NoMessage>() {
        Ok(no_message) => NoMessageFor { input, no_message }.into(),
        Err(error) => error,
    })
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
            "WHERE receivers.seq = messages.seq) AS receivers ",
            $tail
        )
    };
}

// ?1 is the Message's seq and ?2 its Receivers as a JSON array; json_each's key is the index,
// which is the position. last_insert_rowid() inside the SELECT would advance with each
// receivers row, so the Message's seq is bound.
const INSERT_RECEIVERS_SQL: &str = "INSERT INTO receivers (seq, position, name)
     SELECT ?1, key, value FROM json_each(?2)";

// A prefix match written as a range on id, the form the unique index on id serves. Under the
// BINARY collation id has (SCHEMA gives it no other), U+10FFFF sorts after every character an
// id holds, so the range ends past every id with the prefix; a collation that ordered
// otherwise would need another upper bound.
const RESOLVE_SQL: &str =
    select_message!("FROM messages WHERE id >= ?1 AND id < ?1 || char(1114111) ORDER BY seq");

// The id alone, over the same range and order as RESOLVE_SQL, whose comment gives the reason.
const RESOLVE_ID_SQL: &str =
    "SELECT id FROM messages WHERE id >= ?1 AND id < ?1 || char(1114111) ORDER BY seq";

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
             kept(seq, matched) AS (
                 SELECT seq, count(*) OVER () FROM messages
                 WHERE (?1 IS NULL OR sender = ?1)
                   AND (?2 IS NULL OR EXISTS (
                        SELECT 1 FROM receivers
                        WHERE receivers.seq = messages.seq AND receivers.name = ?2))
                   AND (?3 IS NULL OR seq > (SELECT seq FROM messages WHERE id = ?3))
                   AND (?4 IS NULL OR seq < (SELECT seq FROM messages WHERE id = ?4))
                   AND (?5 IS NULL OR id IN (SELECT id FROM upthread WHERE flag = 'upthread'))
                   AND (?6 IS NULL OR id IN (SELECT id FROM downthread WHERE flag = 'downthread'))
                   AND (?7 IS NULL
                        OR id IN (SELECT id FROM upthread WHERE flag = 'thread')
                        OR id IN (SELECT id FROM downthread WHERE flag = 'thread'))
                 ORDER BY seq ",
            $order,
            " LIMIT coalesce(?8, -1)) ",
            select_message!(
                ", kept.matched FROM kept JOIN messages ON messages.seq = kept.seq
                 ORDER BY messages.seq"
            )
        )
    };
}

const LIST_OLDEST_SQL: &str = list_sql!("ASC");
const LIST_NEWEST_SQL: &str = list_sql!("DESC");
