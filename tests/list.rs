//! `list` prints Messages in the order this Inbox stored them, oldest first, narrowed by
//! `--to` and `--from`. Each test runs the built binary against its own temporary
//! `SNAILBOX_HOME`.

mod common;

use common::{
    create, listed_messages, listed_subjects, snailbox, stderr, stdout, store, TempFolder,
};
use serde_json::Value;

#[test]
fn list_on_a_fresh_inbox_prints_nothing_and_succeeds() {
    let home = TempFolder::new("list-fresh");

    let output = snailbox(home.path(), &["list"]);

    assert!(output.status.success(), "list failed: {}", stderr(&output));
    assert!(output.stdout.is_empty(), "stdout was {}", stdout(&output));
    assert!(output.stderr.is_empty(), "stderr was {}", stderr(&output));
}

#[test]
fn list_on_an_empty_inbox_with_json_prints_an_object_with_no_messages() {
    let home = TempFolder::new("list-empty-json");

    let output = snailbox(home.path(), &["list", "--json"]);

    assert!(output.status.success(), "list failed: {}", stderr(&output));
    let value: Value = serde_json::from_str(&stdout(&output)).expect("stdout is JSON");
    assert_eq!(
        value,
        serde_json::json!({"messages": [], "left_out": 0, "next": null})
    );
}

#[test]
fn list_prints_one_line_per_message_with_short_id_time_sender_receivers_and_subject() {
    let home = TempFolder::new("list-line");
    let id = store(&home, "alice", &["bob", "carol"], "lunch?");

    let output = snailbox(home.path(), &["list"]);

    assert!(output.status.success(), "list failed: {}", stderr(&output));
    let text = stdout(&output);
    let line = text
        .strip_suffix('\n')
        .expect("the line ends with a newline");
    let fields: Vec<&str> = line.split('\t').collect();
    assert_eq!(fields.len(), 5, "fields were {fields:?}");
    assert_eq!(fields[0], &id[..8]);
    assert!(
        chrono::DateTime::parse_from_str(fields[1], "%Y-%m-%d %H:%M:%S %:z").is_ok(),
        "not a local time: {:?}",
        fields[1]
    );
    assert_eq!(fields[2], "alice");
    assert_eq!(fields[3], "bob,carol");
    assert_eq!(fields[4], "lunch?");
}

#[test]
fn list_returns_messages_oldest_stored_first() {
    let home = TempFolder::new("list-order");
    // Twelve Messages stored with descending subjects, which sort differently as text, and
    // with random ids, so neither subject nor id order explains the result. This does not
    // prove that creation time plays no part: the command line offers no way to give a Message
    // a creation time other than now, and while the system clock does not step back during
    // the test, creation times never run backwards, so a list sorted by creation time would
    // also pass here; if the clock does step back, this test can fail for that reason alone. That the order is the Inbox's own is pinned by
    // ADR 0001 and the `ORDER BY seq` in `Inbox::list`.
    let subjects: Vec<String> = (0..12).map(|n| format!("message {}", 12 - n)).collect();
    for subject in &subjects {
        store(&home, "alice", &["bob"], subject);
    }

    assert_eq!(listed_subjects(&home, &[]), subjects);
}

#[test]
fn a_tab_or_newline_in_a_field_does_not_break_the_list_line() {
    let home = TempFolder::new("list-escape");
    store(
        &home,
        "al\tice",
        &["bo\nb", "ca\trol"],
        "two\nlines\tand tabs",
    );

    let output = snailbox(home.path(), &["list"]);

    assert!(output.status.success(), "list failed: {}", stderr(&output));
    let text = stdout(&output);
    assert_eq!(text.lines().count(), 1, "output was {text:?}");
    let line = text.strip_suffix('\n').expect("a trailing newline");
    let fields: Vec<&str> = line.split('\t').collect();
    assert_eq!(fields.len(), 5, "fields were {fields:?}");
    assert_eq!(fields[2], "al ice");
    assert_eq!(fields[3], "bo b,ca rol");
    assert_eq!(fields[4], "two lines and tabs");
}

#[test]
fn list_json_returns_the_same_messages_with_full_ids_in_sequence_number_order() {
    let home = TempFolder::new("list-json");
    let first = store(&home, "alice", &["bob"], "first");
    let second = store(&home, "carol", &["bob", "dave"], "second");

    let output = snailbox(home.path(), &["list", "--json"]);

    assert!(output.status.success(), "list failed: {}", stderr(&output));
    let value: Value = serde_json::from_str(&stdout(&output)).expect("stdout is JSON");
    assert_eq!(value["left_out"], 0);
    assert_eq!(value["next"], Value::Null);
    let messages = value["messages"].as_array().expect("a messages array");
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0]["id"], first.as_str());
    assert_eq!(messages[0]["from"], "alice");
    assert_eq!(messages[0]["subject"], "first");
    assert_eq!(messages[1]["id"], second.as_str());
    assert_eq!(messages[1]["to"], serde_json::json!(["bob", "dave"]));
    assert!(messages[1]["created_at"].as_str().is_some());
}

#[test]
fn list_json_messages_carry_no_body_or_parent_while_show_json_does() {
    let home = TempFolder::new("list-json-no-body");
    let root = store(&home, "alice", &["bob"], "root");
    let reply = create(
        home.path(),
        &[
            "--from",
            "bob",
            "--to",
            "alice",
            "--subject",
            "reply",
            "--body",
            "the body",
            "--reply-to",
            &root,
        ],
    );

    let messages = listed_messages(&home, &[]);

    assert_eq!(messages.len(), 2);
    for message in &messages {
        let mut keys: Vec<_> = message.as_object().expect("an object").keys().collect();
        keys.sort();
        assert_eq!(keys, ["created_at", "from", "id", "subject", "to"]);
    }
    let shown = snailbox(home.path(), &["show", &reply, "--json"]);
    assert!(shown.status.success(), "show failed: {}", stderr(&shown));
    let shown: Value = serde_json::from_str(&stdout(&shown)).expect("stdout is JSON");
    assert_eq!(shown["body"], "the body");
    assert_eq!(shown["parent"], root.as_str());
}

#[test]
fn to_matches_a_message_also_addressed_to_others() {
    let home = TempFolder::new("list-to");
    store(&home, "alice", &["bob"], "only bob");
    store(&home, "alice", &["carol", "bob"], "carol and bob");
    store(&home, "alice", &["carol"], "only carol");

    assert_eq!(
        listed_subjects(&home, &["--to", "bob"]),
        ["only bob", "carol and bob"]
    );
}

#[test]
fn from_matches_only_that_sender() {
    let home = TempFolder::new("list-from");
    store(&home, "alice", &["bob"], "from alice");
    store(&home, "carol", &["bob"], "from carol");
    store(&home, "alice", &["dave"], "from alice again");

    assert_eq!(
        listed_subjects(&home, &["--from", "alice"]),
        ["from alice", "from alice again"]
    );
}

#[test]
fn to_and_from_together_return_only_messages_satisfying_both() {
    let home = TempFolder::new("list-both");
    store(&home, "alice", &["bob"], "alice to bob");
    store(&home, "carol", &["bob"], "carol to bob");
    store(&home, "alice", &["dave"], "alice to dave");

    assert_eq!(
        listed_subjects(&home, &["--to", "bob", "--from", "alice"]),
        ["alice to bob"]
    );
}

#[test]
fn a_filter_matching_nothing_prints_nothing_and_succeeds() {
    let home = TempFolder::new("list-no-match");
    store(&home, "alice", &["bob"], "hello");

    assert!(listed_subjects(&home, &["--to", "nobody"]).is_empty());
}

#[test]
fn list_help_states_sequence_number_order_as_a_guarantee() {
    let home = TempFolder::new("list-help");

    let output = snailbox(home.path(), &["list", "--help"]);

    assert!(output.status.success());
    let help = stdout(&output);
    assert!(help.contains("sequence-number order"), "help was {help}");
    assert!(help.contains("oldest first"), "help was {help}");
    assert!(!help.contains("--sort"), "help was {help}");
}

#[test]
fn the_short_help_of_list_and_of_snailbox_says_oldest_first() {
    let home = TempFolder::new("list-short-help");

    for args in [&["list", "-h"][..], &["--help"][..]] {
        let output = snailbox(home.path(), args);

        assert!(output.status.success());
        let help = stdout(&output);
        assert!(
            help.contains("oldest first"),
            "help for {args:?} was {help}"
        );
    }
}
