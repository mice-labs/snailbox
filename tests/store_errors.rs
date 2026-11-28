//! What the binary says on standard error when the Store cannot be set up: a database file that
//! is not a SQLite database fails `list` with an error naming the Store and exits non-zero,
//! with nothing on standard output. "cannot open the Store at" is not asserted here: the
//! database file is created before SQLite opens it, so a path SQLite cannot open fails first
//! with "cannot open or create", which does not name the Store.

mod common;

use common::{snailbox, stderr, stdout, TempFolder, DATABASE_FILE_NAME};

#[test]
fn a_database_file_that_is_not_sqlite_fails_saying_it_cannot_set_up_the_store() {
    let home = TempFolder::new("errors-not-a-database");
    std::fs::write(
        home.path().join(DATABASE_FILE_NAME),
        "this is not a SQLite database file, and is long enough to have a header\n".repeat(20),
    )
    .expect("the test can write the file");

    let output = snailbox(home.path(), &["list"]);

    assert!(!output.status.success(), "list succeeded");
    assert!(output.stdout.is_empty(), "stdout was {}", stdout(&output));
    assert!(
        stderr(&output).starts_with("snailbox: cannot set up the Store: "),
        "stderr was {}",
        stderr(&output)
    );
}
