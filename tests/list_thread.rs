//! `list --upthread`, `--downthread` and `--thread`: each is relative to the named Message,
//! always includes it, leaves out Replies on other branches, keeps arrival order and combines
//! with every other filter. Each test runs the built binary against its own temporary
//! `SNAILBOX_HOME`.

mod common;

use common::{create, listed_subjects, snailbox, stderr, stdout, store, TempFolder};

/// A home holding a branching tree, stored in the order m1 to m7:
///
/// ```text
/// m1 ─ m2 ─ m4 ─ m5
///   └─ m3 ─ m6
/// m7
/// ```
///
/// Returns the ids in that order.
fn tree(name: &str) -> (TempFolder, Vec<String>) {
    let home = TempFolder::new(name);
    let parents = [None, Some(0), Some(0), Some(1), Some(3), Some(2), None];
    let mut ids: Vec<String> = Vec::new();
    for (index, parent) in parents.iter().enumerate() {
        let subject = format!("m{}", index + 1);
        let id = match parent {
            None => store(&home, "alice", &["bob"], &subject),
            Some(parent) => create(
                home.path(),
                &[
                    "--from",
                    if index % 2 == 0 { "alice" } else { "bob" },
                    "--to",
                    "carol",
                    "--subject",
                    &subject,
                    "--reply-to",
                    &ids[*parent],
                ],
            ),
        };
        ids.push(id);
    }
    (home, ids)
}

fn failed_list(home: &TempFolder, args: &[&str]) -> String {
    let output = snailbox(home.path(), &[&["list"], args].concat());
    assert!(
        !output.status.success(),
        "list succeeded: {}",
        stdout(&output)
    );
    assert!(output.stdout.is_empty(), "stdout was {}", stdout(&output));
    stderr(&output)
}

#[test]
fn upthread_returns_the_message_and_the_chain_it_replies_to() {
    let (home, ids) = tree("up-leaf");

    assert_eq!(
        listed_subjects(&home, &["--upthread", &ids[4]]),
        ["m1", "m2", "m4", "m5"]
    );
    assert_eq!(
        listed_subjects(&home, &["--upthread", &ids[2]]),
        ["m1", "m3"]
    );
}

#[test]
fn upthread_of_a_message_that_is_not_a_reply_is_that_message_alone() {
    let (home, ids) = tree("up-first");

    assert_eq!(listed_subjects(&home, &["--upthread", &ids[0]]), ["m1"]);
}

#[test]
fn upthread_of_a_middle_message_stops_at_it() {
    let (home, ids) = tree("up-middle");

    assert_eq!(
        listed_subjects(&home, &["--upthread", &ids[3]]),
        ["m1", "m2", "m4"]
    );
}

#[test]
fn downthread_returns_the_message_and_every_reply_beneath_it() {
    let (home, ids) = tree("down-first");

    assert_eq!(
        listed_subjects(&home, &["--downthread", &ids[0]]),
        ["m1", "m2", "m3", "m4", "m5", "m6"]
    );
}

#[test]
fn downthread_of_a_middle_message_leaves_out_other_branches() {
    let (home, ids) = tree("down-middle");

    assert_eq!(
        listed_subjects(&home, &["--downthread", &ids[1]]),
        ["m2", "m4", "m5"]
    );
}

#[test]
fn downthread_of_a_leaf_is_that_message_alone() {
    let (home, ids) = tree("down-leaf");

    assert_eq!(listed_subjects(&home, &["--downthread", &ids[4]]), ["m5"]);
}

#[test]
fn thread_of_the_first_message_is_the_whole_tree_it_roots() {
    let (home, ids) = tree("thread-first");

    assert_eq!(
        listed_subjects(&home, &["--thread", &ids[0]]),
        ["m1", "m2", "m3", "m4", "m5", "m6"]
    );
}

#[test]
fn thread_of_a_middle_message_leaves_out_other_branches() {
    let (home, ids) = tree("thread-middle");

    assert_eq!(
        listed_subjects(&home, &["--thread", &ids[1]]),
        ["m1", "m2", "m4", "m5"]
    );
}

#[test]
fn thread_of_a_leaf_is_the_chain_back_to_the_first_message() {
    let (home, ids) = tree("thread-leaf");

    assert_eq!(
        listed_subjects(&home, &["--thread", &ids[5]]),
        ["m1", "m3", "m6"]
    );
}

#[test]
fn a_thread_filter_accepts_a_unique_prefix() {
    let (home, ids) = tree("prefix");

    for flag in ["--upthread", "--downthread", "--thread"] {
        assert_eq!(
            listed_subjects(&home, &[flag, &ids[1]]),
            listed_subjects(&home, &[flag, &ids[1][..8]]),
            "{flag} with a prefix"
        );
    }
}

#[test]
fn a_thread_filter_naming_no_message_is_an_error() {
    let (home, _) = tree("no-match");

    for flag in ["--upthread", "--downthread", "--thread"] {
        let error = failed_list(&home, &[flag, "ffffffff-ffff"]);

        assert!(
            error.contains("no Message has an id starting with ffffffff-ffff"),
            "stderr for {flag} was {error}"
        );
        assert!(
            error.contains(flag),
            "stderr for {flag} does not name the flag: {error}"
        );
    }
}

#[test]
fn a_thread_filter_naming_several_messages_lists_them() {
    let home = TempFolder::new("ambiguous");
    // Ids are random, so this takes a handful of Messages, not a fixed number.
    let mut seen: Vec<String> = Vec::new();
    let (first, one, other) = loop {
        let id = store(&home, "alice", &["bob"], "m");
        let first = id.chars().next().expect("an id is not empty");
        if let Some(earlier) = seen.iter().find(|other| other.starts_with(first)) {
            break (first.to_string(), earlier.clone(), id);
        }
        seen.push(id);
    };

    for flag in ["--upthread", "--downthread", "--thread"] {
        let error = failed_list(&home, &[flag, &first]);

        assert!(
            error.contains(&format!("the id prefix {first} matches several Messages:")),
            "stderr for {flag} was {error}"
        );
        assert!(error.contains(&one) && error.contains(&other), "{error}");
    }
}

#[test]
fn a_thread_filter_combines_with_to_and_from() {
    let (home, ids) = tree("to-from");

    // Replies m2, m4 and m6 are from bob; m1 is from alice to bob.
    assert_eq!(
        listed_subjects(&home, &["--thread", &ids[0], "--from", "bob"]),
        ["m2", "m4", "m6"]
    );
    assert_eq!(
        listed_subjects(&home, &["--downthread", &ids[1], "--to", "carol"]),
        ["m2", "m4", "m5"]
    );
    assert_eq!(
        listed_subjects(&home, &["--thread", &ids[0], "--to", "bob"]),
        ["m1"]
    );
}

#[test]
fn downthread_with_after_returns_only_the_new_replies() {
    let (home, ids) = tree("down-after");

    assert_eq!(
        listed_subjects(&home, &["--downthread", &ids[0], "--after", &ids[3]]),
        ["m5", "m6"]
    );
}

#[test]
fn a_thread_filter_combines_with_before() {
    let (home, ids) = tree("before");

    assert_eq!(
        listed_subjects(&home, &["--thread", &ids[0], "--before", &ids[3]]),
        ["m1", "m2", "m3"]
    );
}

#[test]
fn a_thread_filter_combines_with_limit_and_the_notice_names_the_cut() {
    let (home, ids) = tree("limit");

    let output = snailbox(
        home.path(),
        &["list", "--downthread", &ids[0], "--limit", "2"],
    );
    assert!(output.status.success());
    let text = stdout(&output);
    let lines: Vec<_> = text.lines().collect();
    assert_eq!(lines.len(), 3, "{text}");
    assert!(lines[0].ends_with("\tm5"), "{text}");
    assert!(lines[1].ends_with("\tm6"), "{text}");
    assert!(lines[2].starts_with("4 older Messages left out"), "{text}");
}

#[test]
fn thread_filters_given_together_all_apply() {
    let (home, ids) = tree("together");

    assert_eq!(
        listed_subjects(&home, &["--upthread", &ids[4], "--downthread", &ids[1]]),
        ["m2", "m4", "m5"]
    );
}
