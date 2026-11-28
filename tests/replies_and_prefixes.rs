//! `create --reply-to` stores a Reply, `show` prints a Reply's parent, and `show` and
//! `--reply-to` accept a unique id prefix. Each test runs the built binary against its own
//! temporary `SNAILBOX_HOME`.

mod common;

use common::{create, snailbox, stderr, stdout, two_ids_sharing_a_first_character, TempFolder};
use serde_json::Value;
use std::process::Output;

const BASE: [&str; 6] = ["--from", "alice", "--to", "bob", "--subject", "hi"];

fn reply_args(parent: &str) -> Vec<&str> {
    [&BASE[..], &["--reply-to", parent]].concat()
}

/// Runs `snailbox create` with `--reply-to parent`, whatever the outcome.
fn create_reply(home: &TempFolder, parent: &str) -> Output {
    snailbox(
        home.path(),
        &[&["create"], &reply_args(parent)[..]].concat(),
    )
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

    let output = create_reply(&home, unknown);

    assert!(!output.status.success(), "create succeeded");
    assert!(output.stdout.is_empty(), "stdout was {}", stdout(&output));
    assert!(
        stderr(&output).starts_with(&format!(
            "snailbox: no Message has an id starting with {unknown};"
        )),
        "stderr was {}",
        stderr(&output)
    );
    // Each hex digit is a one-character prefix: exactly one of them resolves while the Inbox
    // holds only `existing`, and a stored reply would make a second digit resolve.
    let resolving: Vec<String> = "0123456789abcdef"
        .chars()
        .map(|digit| snailbox(home.path(), &["show", &digit.to_string(), "--json"]))
        .filter(|shown| shown.status.success())
        .map(|shown| {
            let json: Value = serde_json::from_str(&stdout(&shown)).expect("show prints JSON");
            json["id"].as_str().expect("id is a string").to_string()
        })
        .collect();
    assert_eq!(resolving, [existing]);
}

#[test]
fn an_empty_prefix_is_rejected_by_show_and_by_reply_to() {
    let home = TempFolder::new("empty-prefix");
    create(home.path(), &BASE);

    let shown = snailbox(home.path(), &["show", ""]);
    let replied = create_reply(&home, "");

    for output in [&shown, &replied] {
        assert!(!output.status.success(), "the command succeeded");
        assert!(output.stdout.is_empty(), "stdout was {}", stdout(output));
        assert!(
            stderr(output).contains("an id prefix cannot be empty"),
            "stderr was {}",
            stderr(output)
        );
    }
}

#[test]
fn a_prefix_matches_whatever_its_letter_case() {
    let home = TempFolder::new("prefix-case");
    let id = create(home.path(), &BASE);

    let output = snailbox(home.path(), &["show", &id[..12].to_uppercase(), "--json"]);

    assert!(output.status.success(), "show failed: {}", stderr(&output));
    let shown: Value = serde_json::from_str(&stdout(&output)).expect("show --json prints JSON");
    assert_eq!(shown["id"], id.as_str());
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

    let shown = snailbox(home.path(), &["show", &first]);
    let replied = create_reply(&home, &first);

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
fn every_id_argument_matching_several_messages_ends_on_giving_more_characters() {
    let home = TempFolder::new("ambiguous-next-step");
    let (first, _, _) = two_ids_sharing_a_first_character(&home);
    let reply_to = [&["create"], &reply_args(&first)[..]].concat();
    let invocations: Vec<Vec<&str>> = vec![
        vec!["show", &first],
        reply_to,
        vec!["list", "--after", &first],
        vec!["list", "--before", &first],
        vec!["list", "--upthread", &first],
        vec!["list", "--downthread", &first],
        vec!["list", "--thread", &first],
    ];

    for args in invocations {
        let output = snailbox(home.path(), &args);

        assert!(!output.status.success(), "{args:?} succeeded");
        assert!(
            stderr(&output).ends_with("\ngive more characters of the id to pick one\n"),
            "stderr for {args:?} was {}",
            stderr(&output)
        );
    }
}

#[test]
fn every_id_argument_matching_no_message_ends_on_running_snailbox_list() {
    let home = TempFolder::new("no-match-next-step");
    create(home.path(), &BASE);
    let reply_to = [&["create"], &reply_args("zzzz")[..]].concat();
    let invocations: Vec<Vec<&str>> = vec![
        vec!["show", "zzzz"],
        reply_to,
        vec!["list", "--after", "zzzz"],
        vec!["list", "--before", "zzzz"],
        vec!["list", "--upthread", "zzzz"],
        vec!["list", "--downthread", "zzzz"],
        vec!["list", "--thread", "zzzz"],
    ];

    for args in invocations {
        let output = snailbox(home.path(), &args);

        assert!(!output.status.success(), "{args:?} succeeded");
        assert!(
            stderr(&output).ends_with("; run `snailbox list` to find one\n"),
            "stderr for {args:?} was {}",
            stderr(&output)
        );
    }
}

#[test]
fn a_value_matching_no_message_fails_saying_what_to_do_next() {
    let home = TempFolder::new("prefix-no-match");
    create(home.path(), &BASE);

    let shown = snailbox(home.path(), &["show", "zzzz"]);
    let replied = create_reply(&home, "zzzz");

    for (output, names) in [(&shown, "show's ID argument"), (&replied, "--reply-to")] {
        assert!(!output.status.success(), "{names} zzzz succeeded");
        assert!(output.stdout.is_empty(), "stdout was {}", stdout(output));
        assert_eq!(
            stderr(output),
            format!(
                "snailbox: no Message has an id starting with zzzz; {names} takes a Message id \
                 (or a unique prefix of one), not a date or time; run `snailbox list` to find \
                 one\n"
            )
        );
    }
}

#[test]
fn a_date_or_time_given_to_show_or_reply_to_is_an_error_saying_it_takes_a_message_id() {
    let home = TempFolder::new("prefix-date");
    let id = create(home.path(), &BASE);

    // An all-digit value is a valid hex id prefix, so one that this run's random id happens to
    // start with names that Message, not a date; leave it out.
    for value in [
        "2026-11-26",
        "2026-11-26T10:00:00Z",
        "10:30",
        "11/26/2026",
        "20261126",
    ]
    .into_iter()
    .filter(|value| !id.starts_with(value))
    {
        let shown = snailbox(home.path(), &["show", value]);
        let replied = create_reply(&home, value);

        for (output, names) in [(&shown, "show's ID argument"), (&replied, "--reply-to")] {
            assert!(!output.status.success(), "{names} {value} succeeded");
            assert!(
                output.stdout.is_empty(),
                "stdout for {names} {value} was {}",
                stdout(output)
            );
            assert!(
                stderr(output).contains(&format!(
                    "{names} takes a Message id (or a unique prefix of one), not a date or time"
                )),
                "stderr for {names} {value} was {}",
                stderr(output)
            );
        }
    }
}
