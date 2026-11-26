//! `create --reply-to` stores a Reply, `show` prints a Reply's parent, and `show` and
//! `--reply-to` accept a unique id prefix. Each test runs the built binary against its own
//! temporary `SNAILBOX_HOME`.

mod common;

use common::{create, snailbox, stderr, stdout, TempFolder};
use serde_json::Value;

const BASE: [&str; 6] = ["--from", "alice", "--to", "bob", "--subject", "hi"];

fn reply_args(parent: &str) -> Vec<&str> {
    [&BASE[..], &["--reply-to", parent]].concat()
}

/// Creates Messages until two ids share their first character, and returns that character
/// with both ids. Ids are random, so this takes a handful of tries, not a fixed number.
fn two_ids_sharing_a_first_character(home: &TempFolder) -> (char, String, String) {
    let mut seen: Vec<String> = Vec::new();
    loop {
        let id = create(home.path(), &BASE);
        let first = id.chars().next().expect("an id is not empty");
        if let Some(earlier) = seen.iter().find(|other| other.starts_with(first)) {
            return (first, earlier.clone(), id);
        }
        seen.push(id);
    }
}

#[test]
fn a_reply_shows_its_parents_full_id_in_json_and_text() {
    let home = TempFolder::new("reply-show");
    let parent = create(home.path(), &BASE);
    let reply = create(home.path(), &reply_args(&parent));

    let json = snailbox(home.path(), &["show", &reply, "--json"]);
    let text = snailbox(home.path(), &["show", &reply]);

    assert!(json.status.success(), "show failed: {}", stderr(&json));
    let shown: Value = serde_json::from_str(&stdout(&json)).expect("show --json prints JSON");
    assert_eq!(shown["parent"], parent.as_str());
    assert!(text.status.success(), "show failed: {}", stderr(&text));
    assert!(
        stdout(&text).contains(&format!("\nParent:   {parent}\n")),
        "text was {}",
        stdout(&text)
    );
}

#[test]
fn a_message_that_is_not_a_reply_shows_a_null_parent_and_no_parent_line() {
    let home = TempFolder::new("no-parent");
    let id = create(home.path(), &BASE);

    let json = snailbox(home.path(), &["show", &id, "--json"]);
    let text = snailbox(home.path(), &["show", &id]);

    let shown: Value = serde_json::from_str(&stdout(&json)).expect("show --json prints JSON");
    assert!(
        shown.get("parent").is_some_and(Value::is_null),
        "parent was not null: {shown}"
    );
    assert!(
        !stdout(&text).contains("Parent:"),
        "text was {}",
        stdout(&text)
    );
}

#[test]
fn reply_to_accepts_a_unique_prefix_and_stores_the_full_parent_id() {
    let home = TempFolder::new("reply-prefix");
    let parent = create(home.path(), &BASE);

    let reply = create(home.path(), &reply_args(&parent[..12]));

    let json = snailbox(home.path(), &["show", &reply, "--json"]);
    let shown: Value = serde_json::from_str(&stdout(&json)).expect("show --json prints JSON");
    assert_eq!(shown["parent"], parent.as_str());
}

#[test]
fn reply_to_an_unknown_id_fails_and_stores_nothing() {
    let home = TempFolder::new("reply-unknown");
    let existing = create(home.path(), &BASE);
    let unknown = "00000000-0000-4000-8000-000000000000";

    let output = snailbox(
        home.path(),
        &[&["create"], &reply_args(unknown)[..]].concat(),
    );

    assert!(!output.status.success(), "create succeeded");
    assert!(output.stdout.is_empty(), "stdout was {}", stdout(&output));
    assert_eq!(
        stderr(&output),
        format!("snailbox: no Message has the id {unknown}\n")
    );
    // The empty prefix matches every Message, so it names one only when the Inbox holds one.
    let all = snailbox(home.path(), &["show", ""]);
    assert!(all.status.success(), "show failed: {}", stderr(&all));
    assert!(
        stdout(&all).contains(&existing),
        "show printed {}",
        stdout(&all)
    );
}

#[test]
fn show_accepts_a_prefix_matching_exactly_one_message() {
    let home = TempFolder::new("show-prefix");
    let id = create(home.path(), &BASE);

    let output = snailbox(home.path(), &["show", &id[..8], "--json"]);

    assert!(output.status.success(), "show failed: {}", stderr(&output));
    let shown: Value = serde_json::from_str(&stdout(&output)).expect("show --json prints JSON");
    assert_eq!(shown["id"], id.as_str());
}

#[test]
fn a_prefix_matching_several_messages_fails_and_lists_them() {
    let home = TempFolder::new("ambiguous-prefix");
    let (first, one, other) = two_ids_sharing_a_first_character(&home);

    let shown = snailbox(home.path(), &["show", &first.to_string()]);
    let replied = snailbox(
        home.path(),
        &[&["create"], &reply_args(&first.to_string())[..]].concat(),
    );

    for output in [&shown, &replied] {
        assert!(!output.status.success(), "the command succeeded");
        assert!(output.stdout.is_empty(), "stdout was {}", stdout(output));
        let message = stderr(output);
        assert!(
            message.starts_with(&format!(
                "snailbox: the id prefix {first} matches several Messages:\n"
            )),
            "stderr was {message}"
        );
        assert!(message.contains(&one), "{one} not listed in {message}");
        assert!(message.contains(&other), "{other} not listed in {message}");
    }
}

#[test]
fn a_value_matching_no_message_fails_with_a_clear_error() {
    let home = TempFolder::new("prefix-no-match");
    create(home.path(), &BASE);

    let output = snailbox(home.path(), &["show", "zzzz"]);

    assert!(!output.status.success(), "show succeeded");
    assert!(output.stdout.is_empty(), "stdout was {}", stdout(&output));
    assert_eq!(stderr(&output), "snailbox: no Message has the id zzzz\n");
}
