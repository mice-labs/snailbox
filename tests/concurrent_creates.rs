//! Many `create` commands run at once against one Store, as separate processes: none fails and
//! none is lost, and paging with `--after` and a small `--limit` returns each Message exactly
//! once (ADR 0001). Each test runs the built binary against its own temporary `SNAILBOX_HOME`.

mod common;

use std::collections::HashSet;
use std::process::Child;

use common::{command_in, id_from, snailbox, stderr, stdout, store, TempFolder};
use serde_json::Value;

const WRITERS: usize = 32;

/// Starts `WRITERS` `create` processes at once, each with its own subject, and returns the
/// subjects and the ids they printed, failing the test if any failed.
fn create_at_once(home: &TempFolder) -> (HashSet<String>, Vec<String>) {
    let subjects: Vec<String> = (1..=WRITERS).map(|n| format!("w{n}")).collect();
    let children: Vec<Child> = subjects
        .iter()
        .map(|subject| {
            command_in(home.path())
                .args(["create", "--from", "alice", "--to", "bob"])
                .args(["--subject", subject])
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .expect("the snailbox binary starts")
        })
        .collect();
    let ids = children
        .into_iter()
        .map(|child| id_from(&child.wait_with_output().expect("the snailbox binary runs")))
        .collect();
    (subjects.into_iter().collect(), ids)
}

/// One page of `list --json --after <after> --limit <limit>`: its Messages as (id, subject),
/// and whether the limit left newer ones out.
fn page(home: &TempFolder, after: &str, limit: usize) -> (Vec<(String, String)>, bool) {
    let output = snailbox(
        home.path(),
        &[
            "list",
            "--json",
            "--after",
            after,
            "--limit",
            &limit.to_string(),
        ],
    );
    assert!(output.status.success(), "list failed: {}", stderr(&output));
    let parsed: Value = serde_json::from_str(&stdout(&output)).expect("list prints JSON");
    let more = parsed["left_out"].as_u64().expect("a left_out count") > 0;
    let messages = parsed["messages"].as_array().expect("a messages array");
    let messages = messages
        .iter()
        .map(|message| {
            (
                message["id"].as_str().expect("an id").to_owned(),
                message["subject"].as_str().expect("a subject").to_owned(),
            )
        })
        .collect();
    (messages, more)
}

#[test]
fn concurrent_creates_all_succeed_and_each_message_is_kept() {
    let home = TempFolder::new("concurrent-kept");
    // The Store exists before the writers race, so they contend on writes, not on setup.
    store(&home, "alice", &["bob"], "start");

    let (_, ids) = create_at_once(&home);

    for id in &ids {
        let output = snailbox(home.path(), &["show", "--json", id]);
        assert!(
            output.status.success(),
            "show {id} failed: {}",
            stderr(&output)
        );
    }
    let distinct: HashSet<&String> = ids.iter().collect();
    assert_eq!(distinct.len(), WRITERS, "ids printed: {ids:?}");
}

#[test]
fn concurrent_creates_on_a_new_store_all_succeed() {
    let home = TempFolder::new("concurrent-new");

    // create_at_once fails the test if any writer exits non-zero.
    create_at_once(&home);
}

#[test]
fn paging_after_concurrent_creates_returns_each_message_exactly_once() {
    let home = TempFolder::new("concurrent-paging");
    let start = store(&home, "alice", &["bob"], "start");
    let (subjects, ids) = create_at_once(&home);

    let mut seen_ids = Vec::new();
    let mut seen_subjects = Vec::new();
    let mut cursor = start;
    loop {
        let (messages, more) = page(&home, &cursor, 3);
        assert!(!messages.is_empty(), "an empty page after {cursor}");
        cursor = messages.last().expect("a Message").0.clone();
        for (id, subject) in messages {
            seen_ids.push(id);
            seen_subjects.push(subject);
        }
        if !more {
            break;
        }
    }

    let distinct_ids: HashSet<&String> = seen_ids.iter().collect();
    assert_eq!(
        distinct_ids.len(),
        seen_ids.len(),
        "a repeat in {seen_ids:?}"
    );
    assert_eq!(distinct_ids, ids.iter().collect::<HashSet<_>>());
    assert_eq!(
        seen_subjects.into_iter().collect::<HashSet<_>>(),
        subjects,
        "a skipped or foreign Message"
    );
}
