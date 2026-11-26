//! The Inbox's database file as a release before this one left it: an Inbox written with the
//! schema that had no indexes still opens and lists, and gains the indexes on that open; and a
//! stored creation time that is not an RFC 3339 time, one with no offset included, fails `show`
//! and `list` with an error saying so.
//! These tests write the database file directly, since the command line cannot produce an
//! index-less Inbox or a malformed time; everything they check is the built binary's output,
//! apart from the index names, which only the file shows.

mod common;

use std::path::{Path, PathBuf};

use common::{create, snailbox, stderr, stdout, TempFolder};
use rusqlite::Connection;

// The schema as snailbox created it before the indexes on messages.parent, messages.sender and
// receivers.name existed.
const SCHEMA_WITHOUT_INDEXES: &str = "
    CREATE TABLE messages (
        arrival    INTEGER PRIMARY KEY AUTOINCREMENT,
        id         TEXT NOT NULL UNIQUE,
        sender     TEXT NOT NULL,
        subject    TEXT NOT NULL,
        body       TEXT,
        created_at TEXT NOT NULL,
        parent     TEXT REFERENCES messages (id)
    );
    CREATE TABLE receivers (
        arrival  INTEGER NOT NULL REFERENCES messages (arrival),
        position INTEGER NOT NULL,
        name     TEXT NOT NULL,
        PRIMARY KEY (arrival, position)
    );
";

const ROOT_ID: &str = "11111111-1111-4111-8111-111111111111";
const REPLY_ID: &str = "22222222-2222-4222-8222-222222222222";

fn database(home: &Path) -> PathBuf {
    home.join("inbox.sqlite3")
}

/// Writes an Inbox with the index-less schema holding a Message and a Reply to it, each with
/// `created_at` as its stored creation time.
fn write_inbox_without_indexes(home: &Path, created_at: &str) {
    let connection = Connection::open(database(home)).expect("the test can create the Inbox");
    connection
        .execute_batch(SCHEMA_WITHOUT_INDEXES)
        .expect("the old schema applies");
    connection
        .execute_batch(&format!(
            "INSERT INTO messages (id, sender, subject, body, created_at, parent) VALUES
                 ('{ROOT_ID}', 'alice', 'root', NULL, '{created_at}', NULL),
                 ('{REPLY_ID}', 'bob', 'reply', 'ok', '{created_at}', '{ROOT_ID}');
             INSERT INTO receivers (arrival, position, name) VALUES
                 (1, 0, 'bob'), (2, 0, 'alice');"
        ))
        .expect("the old rows insert");
}

/// The names of the indexes snailbox creates, as the database file lists them.
fn index_names(home: &Path) -> Vec<String> {
    let connection = Connection::open(database(home)).expect("the Inbox opens");
    let mut statement = connection
        .prepare(
            "SELECT name FROM sqlite_master
             WHERE type = 'index' AND name NOT LIKE 'sqlite_autoindex%' ORDER BY name",
        )
        .expect("the index query prepares");
    let names = statement
        .query_map([], |row| row.get(0))
        .expect("the index query runs")
        .collect::<rusqlite::Result<Vec<String>>>()
        .expect("each index name reads");
    names
}

const EXPECTED_INDEXES: [&str; 3] = [
    "messages_by_parent",
    "messages_by_sender",
    "receivers_by_name",
];

#[test]
fn an_inbox_without_indexes_opens_lists_and_gains_them() {
    let home = TempFolder::new("file-old-schema");
    write_inbox_without_indexes(home.path(), "2026-11-26T01:02:03Z");

    let output = snailbox(
        home.path(),
        &["list", "--downthread", "1111", "--from", "bob", "--json"],
    );

    assert!(output.status.success(), "list failed: {}", stderr(&output));
    assert!(output.stderr.is_empty(), "stderr was {}", stderr(&output));
    let listed: serde_json::Value =
        serde_json::from_str(&stdout(&output)).expect("list --json prints JSON");
    assert_eq!(
        listed,
        serde_json::json!({
            "messages": [{
                "id": REPLY_ID,
                "from": "bob",
                "to": ["alice"],
                "subject": "reply",
                "created_at": "2026-11-26T01:02:03Z",
            }],
            "left_out": 0,
            "next": null,
        })
    );
    let shown = snailbox(home.path(), &["show", REPLY_ID, "--json"]);
    assert!(shown.status.success(), "show failed: {}", stderr(&shown));
    let shown: serde_json::Value =
        serde_json::from_str(&stdout(&shown)).expect("show --json prints JSON");
    assert_eq!(shown["body"], "ok");
    assert_eq!(shown["parent"], ROOT_ID);
    assert_eq!(index_names(home.path()), EXPECTED_INDEXES);
}

#[test]
fn a_new_inbox_has_the_indexes() {
    let home = TempFolder::new("file-new-schema");
    create(
        home.path(),
        &["--from", "alice", "--to", "bob", "--subject", "hello"],
    );

    assert_eq!(index_names(home.path()), EXPECTED_INDEXES);
}

/// Asserts that `show` and `list` each fail on an Inbox whose creation times are `stored`,
/// with an error quoting it.
fn assert_show_and_list_reject_time(name: &str, stored: &str) {
    let home = TempFolder::new(name);
    write_inbox_without_indexes(home.path(), stored);

    for args in [&["show", ROOT_ID][..], &["list"][..]] {
        let output = snailbox(home.path(), args);

        assert!(!output.status.success(), "{args:?} succeeded");
        assert!(output.stdout.is_empty(), "stdout was {}", stdout(&output));
        assert!(
            stderr(&output).starts_with(&format!(
                "snailbox: the Inbox holds a malformed time {stored}: "
            )),
            "{args:?} stderr was {}",
            stderr(&output)
        );
    }
}

#[test]
fn a_malformed_creation_time_fails_show_and_list_saying_so() {
    assert_show_and_list_reject_time("file-malformed-time", "yesterday");
}

#[test]
fn a_creation_time_without_an_offset_is_malformed() {
    assert_show_and_list_reject_time("file-naive-time", "2026-11-26 01:02:03");
}
