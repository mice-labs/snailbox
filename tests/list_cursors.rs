//! `list --after`, `--before` and `--limit`: cursors are Message ids (ADR 0001), the limit
//! keeps the oldest or the newest Messages by which cursor is given, and a limit that left
//! Messages out says so. Each test runs the built binary against its own temporary
//! `SNAILBOX_HOME`.

mod common;

use common::{
    failed_list, listed_subjects, snailbox, stdout, store, two_ids_sharing_a_first_character,
    TempFolder,
};
use serde_json::Value;

/// A home holding Messages "m1" to "mN" from alice to bob, and their ids in order.
fn home_with(count: usize, name: &str) -> (TempFolder, Vec<String>) {
    let home = TempFolder::new(name);
    let ids = (1..=count)
        .map(|n| store(&home, "alice", &["bob"], &format!("m{n}")))
        .collect();
    (home, ids)
}

fn subjects(range: std::ops::RangeInclusive<usize>) -> Vec<String> {
    range.map(|n| format!("m{n}")).collect()
}

#[test]
fn after_returns_the_messages_stored_after_the_named_one() {
    let (home, ids) = home_with(4, "after");

    assert_eq!(listed_subjects(&home, &["--after", &ids[1]]), ["m3", "m4"]);
}

#[test]
fn before_returns_the_messages_stored_before_the_named_one() {
    let (home, ids) = home_with(4, "before");

    assert_eq!(listed_subjects(&home, &["--before", &ids[2]]), ["m1", "m2"]);
}

#[test]
fn after_and_before_together_bound_a_range() {
    let (home, ids) = home_with(5, "range");

    assert_eq!(
        listed_subjects(&home, &["--after", &ids[0], "--before", &ids[4]]),
        ["m2", "m3", "m4"]
    );
}

#[test]
fn a_cursor_accepts_a_unique_prefix() {
    let (home, ids) = home_with(3, "prefix");
    let prefix = &ids[0][..8];

    assert_eq!(listed_subjects(&home, &["--after", prefix]), ["m2", "m3"]);
    assert_eq!(
        listed_subjects(&home, &["--before", &ids[2][..8]]),
        ["m1", "m2"]
    );
}

#[test]
fn a_cursor_matching_no_message_is_an_error() {
    let (home, _) = home_with(2, "no-match");

    for flag in ["--after", "--before"] {
        let error = failed_list(&home, &[flag, "ffffffff-ffff"]);

        assert!(
            error.contains("no Message has an id starting with ffffffff-ffff"),
            "stderr for {flag} was {error}"
        );
    }
}

#[test]
fn an_ambiguous_cursor_prefix_is_an_error_listing_the_matches() {
    let home = TempFolder::new("ambiguous");
    let (shared, _, _) = two_ids_sharing_a_first_character(&home);

    for flag in ["--after", "--before"] {
        let error = failed_list(&home, &[flag, &shared]);
        assert!(
            error.contains(&format!("the id prefix {shared} matches several Messages")),
            "stderr was {error}"
        );
    }
}

#[test]
fn a_date_or_time_cursor_is_an_error_saying_the_flag_takes_a_message_id() {
    let (home, ids) = home_with(1, "date");

    for flag in ["--after", "--before"] {
        // An all-digit value is a valid hex id prefix, so one that this run's random id
        // happens to start with is a real cursor, not a date; leave it out.
        for value in [
            "2026-11-26",
            "2026-11-26T10:00:00Z",
            "10:30",
            "11/26/2026",
            "2026/11/26",
            "26.11.2026",
            "2026-1-5",
            "1764150000",
            "20261126",
            "2026",
            "202611",
            "00000000-0000-0000-0000-000000000000a",
        ]
        .into_iter()
        .filter(|value| !ids[0].starts_with(value))
        {
            let error = failed_list(&home, &[flag, value]);
            assert!(
                error.contains(&format!("{flag} takes a Message id")),
                "stderr for {flag} {value} was {error}"
            );
        }
    }
}

#[test]
fn the_help_says_after_and_before_take_a_message_id() {
    let home = TempFolder::new("cursor-help");

    let output = snailbox(home.path(), &["list", "--help"]);

    let help = stdout(&output);
    for flag in ["--after <ID>", "--before <ID>"] {
        let entry: Vec<&str> = help
            .lines()
            .skip_while(|line| !line.contains(flag))
            .enumerate()
            .take_while(|(n, line)| *n == 0 || !line.trim_start().starts_with('-'))
            .map(|(_, line)| line)
            .collect();
        assert!(!entry.is_empty(), "no {flag} in {help}");
        assert!(
            entry.iter().any(|line| line.contains("Message id")),
            "help for {flag} was {entry:?}"
        );
    }
}

#[test]
fn limit_defaults_to_fifty_and_the_most_recent_are_kept() {
    let (home, _) = home_with(52, "default-limit");

    assert_eq!(listed_subjects(&home, &[]), subjects(3..=52));
}

#[test]
fn limit_zero_removes_the_limit() {
    let (home, _) = home_with(52, "no-limit");

    assert_eq!(listed_subjects(&home, &["--limit", "0"]), subjects(1..=52));
}

#[test]
fn with_neither_cursor_the_most_recent_n_come_oldest_first() {
    let (home, _) = home_with(5, "limit-recent");

    assert_eq!(listed_subjects(&home, &["--limit", "2"]), ["m4", "m5"]);
}

#[test]
fn with_after_the_first_n_after_it_are_returned() {
    let (home, ids) = home_with(6, "limit-after");

    assert_eq!(
        listed_subjects(&home, &["--after", &ids[1], "--limit", "2"]),
        ["m3", "m4"]
    );
}

#[test]
fn with_after_and_before_the_first_n_after_the_lower_bound_are_returned() {
    let (home, ids) = home_with(7, "limit-range");

    assert_eq!(
        listed_subjects(
            &home,
            &["--after", &ids[0], "--before", &ids[6], "--limit", "2"]
        ),
        ["m2", "m3"]
    );
}

#[test]
fn with_before_alone_the_last_n_before_it_are_returned() {
    let (home, ids) = home_with(6, "limit-before");

    assert_eq!(
        listed_subjects(&home, &["--before", &ids[4], "--limit", "2"]),
        ["m3", "m4"]
    );
}

#[test]
fn text_output_ends_with_a_line_saying_how_many_were_left_out_and_how_to_get_them() {
    let (home, ids) = home_with(5, "notice-recent");

    let output = snailbox(home.path(), &["list", "--limit", "2"]);

    let text = stdout(&output);
    let last = text.lines().last().expect("a last line");
    assert_eq!(text.lines().count(), 3, "output was {text}");
    assert!(
        last.contains("3 older Messages left out"),
        "last was {last}"
    );
    assert!(
        last.contains(&format!("--before {}", ids[3])),
        "last was {last}"
    );
}

#[test]
fn the_notice_after_a_first_n_page_points_at_the_last_message_returned() {
    let (home, ids) = home_with(6, "notice-after");

    let output = snailbox(home.path(), &["list", "--after", &ids[0], "--limit", "2"]);

    let text = stdout(&output);
    let last = text.lines().last().expect("a last line");
    assert!(
        last.contains("3 newer Messages left out"),
        "last was {last}"
    );
    assert!(
        last.contains(&format!("--after {}", ids[2])),
        "last was {last}"
    );
}

#[test]
fn the_notice_with_before_alone_points_at_the_first_message_returned() {
    let (home, ids) = home_with(6, "notice-before");

    let output = snailbox(home.path(), &["list", "--before", &ids[5], "--limit", "2"]);

    let text = stdout(&output);
    let last = text.lines().last().expect("a last line");
    assert!(
        last.contains("3 older Messages left out"),
        "last was {last}"
    );
    assert!(
        last.contains(&format!("--before {}", ids[3])),
        "last was {last}"
    );
}

#[test]
fn the_notice_says_to_replace_only_the_cursor_and_keep_the_other_flags() {
    let (home, _) = home_with(5, "notice-keeps-flags");

    let output = snailbox(home.path(), &["list", "--limit", "2"]);

    let text = stdout(&output);
    let last = text.lines().last().expect("a last line");
    assert!(
        last.contains("same flags") && last.contains("in place of any --before"),
        "last was {last}"
    );
}

#[test]
fn the_notice_counts_a_single_message_in_the_singular() {
    let (home, _) = home_with(2, "notice-singular");

    let output = snailbox(home.path(), &["list", "--limit", "1"]);

    let text = stdout(&output);
    let last = text.lines().last().expect("a last line");
    assert!(last.contains("1 older Message left out"), "last was {last}");
    assert!(!last.contains("Messages"), "last was {last}");
}

#[test]
fn no_notice_when_nothing_was_left_out() {
    let (home, _) = home_with(3, "no-notice");

    for args in [&["--limit", "3"][..], &["--limit", "0"][..], &[][..]] {
        let output = snailbox(home.path(), &[&["list"], args].concat());
        let text = stdout(&output);
        assert_eq!(text.lines().count(), 3, "output for {args:?} was {text}");
        assert!(!text.contains("left out"), "output for {args:?} was {text}");
    }
}

#[test]
fn json_carries_what_the_limit_left_out_as_a_field() {
    let (home, ids) = home_with(5, "json-notice");

    let output = snailbox(home.path(), &["list", "--limit", "2", "--json"]);

    let value: Value = serde_json::from_str(&stdout(&output)).expect("stdout is JSON");
    assert_eq!(value["left_out"], 3);
    assert_eq!(value["next"], format!("--before {}", ids[3]));
    let messages = value["messages"].as_array().expect("a messages array");
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0]["subject"], "m4");
}

#[test]
fn json_with_after_names_the_after_flag_as_next() {
    let (home, ids) = home_with(6, "json-after");

    let output = snailbox(
        home.path(),
        &["list", "--after", &ids[0], "--limit", "2", "--json"],
    );

    let value: Value = serde_json::from_str(&stdout(&output)).expect("stdout is JSON");
    assert_eq!(value["left_out"], 3);
    assert_eq!(value["next"], format!("--after {}", ids[2]));
    assert_eq!(value["messages"][0]["subject"], "m2");
}

#[test]
fn json_with_before_alone_names_the_before_flag_as_next() {
    let (home, ids) = home_with(6, "json-before");

    let output = snailbox(
        home.path(),
        &["list", "--before", &ids[5], "--limit", "2", "--json"],
    );

    let value: Value = serde_json::from_str(&stdout(&output)).expect("stdout is JSON");
    assert_eq!(value["left_out"], 3);
    assert_eq!(value["next"], format!("--before {}", ids[3]));
    assert_eq!(value["messages"][0]["subject"], "m4");
}

#[test]
fn text_notice_with_both_cursors_points_after_the_last_message_returned() {
    let (home, ids) = home_with(7, "notice-range");

    let output = snailbox(
        home.path(),
        &[
            "list", "--after", &ids[0], "--before", &ids[6], "--limit", "2",
        ],
    );

    let text = stdout(&output);
    let last = text.lines().last().expect("a last line");
    assert!(
        last.contains("3 newer Messages left out"),
        "last was {last}"
    );
    assert!(
        last.contains(&format!("--after {}", ids[2])),
        "last was {last}"
    );
}

#[test]
fn json_with_both_cursors_names_the_after_flag_as_next() {
    let (home, ids) = home_with(7, "json-range");

    let output = snailbox(
        home.path(),
        &[
            "list", "--after", &ids[0], "--before", &ids[6], "--limit", "2", "--json",
        ],
    );

    let value: Value = serde_json::from_str(&stdout(&output)).expect("stdout is JSON");
    assert_eq!(value["left_out"], 3);
    assert_eq!(value["next"], format!("--after {}", ids[2]));
    assert_eq!(value["messages"][0]["subject"], "m2");
}

#[test]
fn json_stays_a_plain_array_when_nothing_was_left_out() {
    let (home, _) = home_with(3, "json-complete");

    let output = snailbox(home.path(), &["list", "--limit", "3", "--json"]);

    let value: Value = serde_json::from_str(&stdout(&output)).expect("stdout is JSON");
    assert_eq!(value.as_array().expect("a JSON array").len(), 3);
}

#[test]
fn cursors_and_limit_combine_with_to_and_from() {
    let home = TempFolder::new("combine");
    let first = store(&home, "alice", &["bob"], "a1");
    store(&home, "carol", &["bob"], "c1");
    store(&home, "alice", &["dave"], "a-dave");
    store(&home, "alice", &["bob"], "a2");
    store(&home, "alice", &["bob"], "a3");

    assert_eq!(
        listed_subjects(
            &home,
            &["--after", &first, "--from", "alice", "--to", "bob", "--limit", "1"]
        ),
        ["a2"]
    );
}

#[test]
fn before_and_limit_combine_with_to_and_from() {
    let home = TempFolder::new("combine-before");
    store(&home, "alice", &["bob"], "a1");
    store(&home, "carol", &["bob"], "c1");
    store(&home, "alice", &["bob"], "a2");
    store(&home, "alice", &["dave"], "a-dave");
    store(&home, "alice", &["bob"], "a3");
    let last = store(&home, "alice", &["bob"], "a4");

    assert_eq!(
        listed_subjects(
            &home,
            &["--before", &last, "--from", "alice", "--to", "bob", "--limit", "2"]
        ),
        ["a2", "a3"]
    );
}

#[test]
fn a_cursor_message_need_not_match_the_filters() {
    let home = TempFolder::new("cursor-outside-filter");
    let carol = store(&home, "carol", &["bob"], "c1");
    store(&home, "alice", &["bob"], "a1");

    assert_eq!(
        listed_subjects(&home, &["--after", &carol, "--from", "alice"]),
        ["a1"]
    );
}
