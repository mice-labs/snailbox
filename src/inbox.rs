//! The Inbox: the SQLite store of Messages, and the operations snailbox offers on it.

use std::num::NonZeroUsize;
use std::path::Path;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use chrono::{DateTime, SecondsFormat, Utc};
use rusqlite::{params, Connection, Row, TransactionBehavior};

use crate::listing::{Cursor, LeftOut, Listing};
use crate::location;
use crate::message::{IdPrefix, Message, MessageId, Name, NewMessage};

mod body;
pub use body::BodySource;

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
// the table. SQLite enforces the REFERENCES clauses per connection, only where foreign_keys is
// on. Inbox::open sets it on each connection it opens, before any transaction, since inside one
// SQLite ignores the pragma; setting it keeps them enforced whatever the build default is
// (snailbox-8ly.1.22). A connection opened another way enforces them only if it sets the pragma
// or its SQLite build defaults it on, as rusqlite's `bundled` build (Cargo.toml) does with
// SQLITE_DEFAULT_FOREIGN_KEYS=1.
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

/// The arguments that name a Message by id or prefix: the `ListFilter` id fields, the Message
/// `show` prints and the parent a new Message replies to.
#[derive(Debug, Clone, Copy)]
pub enum IdArgument {
    After,
    Before,
    Upthread,
    Downthread,
    Thread,
    Show,
    ReplyTo,
}

/// An id argument named a prefix no Message matches; a caller downcasts to it to add a hint
/// naming the argument.
#[derive(Debug)]
pub struct NoMessageFor {
    pub argument: IdArgument,
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
            .pragma_update(None, "foreign_keys", true)
            .context("cannot set up the Inbox")?;
        connection
            .execute_batch(SCHEMA)
            .context("cannot set up the Inbox")?;
        Ok(Self { connection })
    }

    /// Stores `new` with a fresh id and the current time, and returns the id.
    pub fn create(&mut self, new: &NewMessage) -> Result<MessageId> {
        let parent = self.full_id_of(&new.reply_to, IdArgument::ReplyTo)?;
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
                    new.sender.as_str(),
                    new.subject,
                    new.body,
                    created_at,
                    parent
                ],
            )
            .context("cannot store the Message")?;
        let seq = transaction.last_insert_rowid();
        let receivers =
            serde_json::to_string(&new.receivers.iter().map(Name::as_str).collect::<Vec<_>>())
                .context("cannot store the Message")?;
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
        let matches = statement
            .query_and_then([prefix.as_str()], message_from_row)?
            .collect::<Result<Vec<Message>>>()?;
        one_of(prefix, matches, |message| message.id.to_string())
    }

    /// `resolve` for the id alone, read by `RESOLVE_ID_SQL`, which loads no Message body or
    /// Receiver.
    fn resolve_id(&self, prefix: &IdPrefix) -> Result<MessageId> {
        let mut statement = self.connection.prepare(RESOLVE_ID_SQL)?;
        let ids = statement
            .query_map([prefix.as_str()], |row| row.get::<_, String>("id"))?
            .map(|id| stored_id("id", &id?))
            .collect::<Result<Vec<MessageId>>>()?;
        one_of(prefix, ids, MessageId::to_string)
    }

    /// The full id of the Message `prefix` names, or none when there is no `prefix`; fails as
    /// `resolve_id` does, its no-match error naming `argument`.
    fn full_id_of(
        &self,
        prefix: &Option<IdPrefix>,
        argument: IdArgument,
    ) -> Result<Option<String>> {
        prefix
            .as_ref()
            .map(|prefix| named_by(self.resolve_id(prefix), argument).map(|id| id.to_string()))
            .transpose()
    }

    /// The Message whose id is, or is the only one to start with, `prefix`; fails when no
    /// Message does or several do.
    pub fn show(&self, prefix: &IdPrefix) -> Result<Message> {
        named_by(self.resolve(prefix), IdArgument::Show)
    }

    /// The Messages matching `filter`, in the order this Inbox stored them, oldest first
    /// (ADR 0001); creation time plays no part in the order. When the limit cuts the matches,
    /// the oldest are kept if `after` is given and the newest otherwise, and the listing says
    /// what was left out and where to continue.
    pub fn list(&self, filter: &ListFilter) -> Result<Listing> {
        let after = self.full_id_of(&filter.after, IdArgument::After)?;
        let before = self.full_id_of(&filter.before, IdArgument::Before)?;
        let upthread = self.full_id_of(&filter.upthread, IdArgument::Upthread)?;
        let downthread = self.full_id_of(&filter.downthread, IdArgument::Downthread)?;
        let thread = self.full_id_of(&filter.thread, IdArgument::Thread)?;
        let paging_forward = after.is_some();
        let sql = if paging_forward {
            LIST_OLDEST_SQL
        } else {
            LIST_NEWEST_SQL
        };
        let mut statement = self.connection.prepare(sql)?;
        let mut rows = statement.query(params![
            filter.from,
            filter.to,
            after,
            before,
            upthread,
            downthread,
            thread,
            filter.limit.count(),
        ])?;
        let mut matched = 0;
        let mut messages = Vec::new();
        while let Some(row) = rows.next()? {
            matched = row.get("matched")?;
            messages.push(message_from_row(row)?);
        }
        let count = matched - messages.len();
        // A limit is at least 1, so a listing that left Messages out kept at least one.
        let left_out = (count > 0).then(|| LeftOut {
            count,
            cursor: if paging_forward {
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

/// `result`, its no-match error named for `argument`.
fn named_by<T>(result: Result<T>, argument: IdArgument) -> Result<T> {
    result.map_err(|error| match error.downcast::<NoMessage>() {
        Ok(no_message) => NoMessageFor {
            argument,
            no_message,
        }
        .into(),
        Err(error) => error,
    })
}

/// The Message a row of `select_message!` holds, its Receivers read from the JSON array the
/// query aggregates.
fn message_from_row(row: &Row<'_>) -> Result<Message> {
    let id: String = row.get("id")?;
    let parent: Option<String> = row.get("parent")?;
    let receivers: String = row.get("receivers")?;
    Ok(Message {
        id: stored_id("id", &id)?,
        sender: row.get("sender")?,
        receivers: serde_json::from_str(&receivers)
            .with_context(|| format!("the Inbox holds malformed Receivers {receivers}"))?,
        subject: row.get("subject")?,
        body: row.get("body")?,
        parent: parent
            .map(|parent| stored_id("parent", &parent))
            .transpose()?,
        created_at: created_at_from_row(row)?,
    })
}

/// The Message id the `column` column stores as `stored`; fails, naming the column and quoting
/// the text, when it is not one.
fn stored_id(column: &str, stored: &str) -> Result<MessageId> {
    stored
        .parse()
        .with_context(|| format!("the Inbox holds a malformed {column} {stored}"))
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
    /// The most Messages to return.
    pub limit: Limit,
}

/// The most Messages a `list` returns: 50 unless the caller says otherwise, and never fewer
/// than 1; there is no unlimited listing.
#[derive(Clone, Copy, Debug)]
pub struct Limit(NonZeroUsize);

impl Limit {
    const DEFAULT: NonZeroUsize = NonZeroUsize::new(50).expect("50 is not zero");

    fn count(self) -> usize {
        self.0.get()
    }
}

impl Default for Limit {
    fn default() -> Self {
        Self(Self::DEFAULT)
    }
}

impl std::fmt::Display for Limit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::str::FromStr for Limit {
    type Err = anyhow::Error;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let count: usize = text.parse()?;
        NonZeroUsize::new(count)
            .map(Self)
            .context("the limit must be at least 1")
    }
}

/// Prepends the SELECT that reads a whole Message, its Receivers aggregated in position order
/// as a JSON array: a Receiver name can hold a comma, so a separator-joined string would not
/// split back (snailbox-6vk). The ORDER BY inside json_group_array needs SQLite 3.44 or later,
/// which holds because rusqlite's `bundled` feature (Cargo.toml) compiles in its own newer
/// SQLite; linked against an older system SQLite the query would fail to prepare. `format!`
/// cannot build a const, so the join is `concat!`, which takes literals only.
macro_rules! select_message {
    ($tail:expr) => {
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
macro_rules! id_prefix_range {
    () => {
        "FROM messages WHERE id >= ?1 AND id < ?1 || char(1114111) ORDER BY seq"
    };
}

const RESOLVE_SQL: &str = select_message!(id_prefix_range!());

const RESOLVE_ID_SQL: &str = concat!("SELECT id ", id_prefix_range!());

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

#[cfg(test)]
mod test_folder {
    use std::path::Path;

    /// A folder under the temp directory for one test's Inbox, removed on drop.
    pub struct TempFolder(std::path::PathBuf);

    impl TempFolder {
        pub fn new(name: &str) -> Self {
            let folder =
                std::env::temp_dir().join(format!("snailbox-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&folder);
            Self(folder)
        }

        pub fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempFolder {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

// These tests pin the rule the Inbox module decides for the command-line layer: the default
// limit. It goes through the module's public types, never the command line.
#[cfg(test)]
mod rule_tests {
    use super::test_folder::TempFolder;
    use super::*;

    fn name(text: &str) -> Name {
        text.parse().expect("the name is not blank")
    }

    fn unfiltered(limit: Limit) -> ListFilter {
        ListFilter {
            to: None,
            from: None,
            after: None,
            before: None,
            upthread: None,
            downthread: None,
            thread: None,
            limit,
        }
    }

    fn store(inbox: &mut Inbox, count: usize) {
        for _ in 0..count {
            inbox
                .create(&NewMessage {
                    sender: name("alice"),
                    receivers: vec![name("bob")],
                    subject: "hi".to_owned(),
                    body: None,
                    reply_to: None,
                })
                .expect("the Message is stored");
        }
    }

    #[test]
    fn a_listing_without_a_limit_keeps_the_most_recent_fifty() {
        let folder = TempFolder::new("default-limit");
        let mut inbox = Inbox::open(folder.path()).expect("the Inbox opens");
        store(&mut inbox, 52);

        let listing = inbox.list(&unfiltered(Limit::default())).expect("lists");

        assert_eq!(listing.messages.len(), 50);
        assert_eq!(listing.left_out.expect("two were left out").count, 2);
    }

    #[test]
    fn a_limit_of_zero_is_refused() {
        let error = "0".parse::<Limit>().expect_err("0 is not a limit");

        assert_eq!(error.to_string(), "the limit must be at least 1");
    }

    #[test]
    fn a_limit_that_is_not_a_number_is_refused() {
        let error = "many".parse::<Limit>().expect_err("many is not a number");

        assert_eq!(error.to_string(), "invalid digit found in string");
    }
}

// These tests pin the guarantee that an Inbox refuses a row whose REFERENCES name nothing, not
// the foreign_keys pragma Inbox::open sets: on rusqlite's `bundled` build, which defaults the
// pragma on, they pass without it too. They test at the database because the CLI never writes
// such a row.
#[cfg(test)]
mod tests {
    use super::test_folder::TempFolder;
    use super::*;

    fn assert_foreign_key_refused(result: rusqlite::Result<usize>) {
        let error = result.expect_err("the insert is refused");
        assert!(
            error.to_string().contains("FOREIGN KEY constraint failed"),
            "the insert failed for another reason: {error}"
        );
    }

    #[test]
    fn a_message_whose_parent_names_no_message_is_refused() {
        let folder = TempFolder::new("fk-parent");
        let inbox = Inbox::open(folder.path()).expect("the Inbox opens");

        let result = inbox.connection.execute(
            "INSERT INTO messages (id, sender, subject, body, created_at, parent)
             VALUES ('33333333-3333-4333-8333-333333333333', 'alice', 'hi', NULL,
                     '2026-11-27T00:00:00Z', 'no-such-id')",
            [],
        );

        assert_foreign_key_refused(result);
    }

    #[test]
    fn a_receiver_whose_seq_names_no_message_is_refused() {
        let folder = TempFolder::new("fk-receiver");
        let inbox = Inbox::open(folder.path()).expect("the Inbox opens");

        let result = inbox.connection.execute(
            "INSERT INTO receivers (seq, position, name) VALUES (999, 0, 'bob')",
            [],
        );

        assert_foreign_key_refused(result);
    }
}
