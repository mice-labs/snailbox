//! `create` stores a Message and prints only its full id; `show <full id>` prints that Message
//! as text or, with `--json`, as JSON. Each test runs the built binary against its own
//! temporary `SNAILBOX_HOME`.

mod common;

use std::process::Output;

use chrono::{DateTime, FixedOffset, Timelike, Utc};
use common::{create, snailbox, stderr, stdout, TempHome};
use serde_json::Value;

/// True when `id` is a lowercase hyphenated UUID of version 4 and the RFC 4122 variant.
fn is_uuid_v4(id: &str) -> bool {
    let groups: Vec<&str> = id.split('-').collect();
    let lengths: Vec<usize> = groups.iter().map(|group| group.len()).collect();
    lengths == [8, 4, 4, 4, 12]
        && id
            .chars()
            .all(|c| c == '-' || c.is_ascii_digit() || ('a'..='f').contains(&c))
        && groups[2].starts_with('4')
        && groups[3].starts_with(['8', '9', 'a', 'b'])
}

#[test]
fn create_prints_only_a_random_uuid_v4_id() {
    let home = TempHome::new("create-id");
    let args = [
        "create",
        "--from",
        "alice",
        "--to",
        "bob",
        "--subject",
        "hello",
    ];

    let first = snailbox(home.path(), &args);
    let second = snailbox(home.path(), &args);

    assert!(first.status.success(), "create failed: {}", stderr(&first));
    assert!(first.stderr.is_empty(), "stderr was {}", stderr(&first));
    let first_id = stdout(&first);
    let first_id = first_id
        .strip_suffix('\n')
        .expect("the id ends with a newline");
    assert!(is_uuid_v4(first_id), "not a UUID v4: {first_id:?}");
    let second_id = stdout(&second);
    assert_ne!(first_id, second_id.trim_end(), "two creates gave one id");
}

/// Runs `show` with the time zone fixed nine hours east of UTC, so the local time in the text
/// form is known. POSIX reverses the sign in `Etc/GMT-9`.
fn show_in_utc_plus_nine(home: &TempHome, args: &[&str]) -> Output {
    common::bare_command()
        .env("SNAILBOX_HOME", home.path())
        .env("TZ", "Etc/GMT-9")
        .arg("show")
        .args(args)
        .output()
        .expect("the snailbox binary runs")
}

fn created_message(home: &TempHome) -> (String, DateTime<Utc>, DateTime<Utc>) {
    let before = Utc::now()
        .with_nanosecond(0)
        .expect("zero is a valid nanosecond");
    let id = create(
        home.path(),
        &[
            "--from",
            "alice",
            "--to",
            "bob",
            "--to",
            "carol",
            "--subject",
            "lunch?",
            "--body",
            "Noon at the usual place.",
        ],
    );
    (id, before, Utc::now())
}

#[test]
fn show_json_prints_every_field_with_the_creation_time_in_utc() {
    let home = TempHome::new("show-json");
    let (id, before, after) = created_message(&home);

    let output = show_in_utc_plus_nine(&home, &[&id, "--json"]);

    assert!(output.status.success(), "show failed: {}", stderr(&output));
    assert!(output.stderr.is_empty(), "stderr was {}", stderr(&output));
    let shown: Value = serde_json::from_str(&stdout(&output)).expect("show --json prints JSON");
    assert_eq!(shown["id"], id.as_str());
    assert_eq!(shown["from"], "alice");
    assert_eq!(shown["to"], serde_json::json!(["bob", "carol"]));
    assert_eq!(shown["subject"], "lunch?");
    assert_eq!(shown["body"], "Noon at the usual place.");
    let created_at = shown["created_at"]
        .as_str()
        .expect("created_at is a string");
    assert!(created_at.ends_with('Z'), "not UTC: {created_at}");
    let created_at = DateTime::parse_from_rfc3339(created_at).expect("created_at is RFC 3339");
    assert!(
        before <= created_at && created_at <= after,
        "{created_at} is outside {before}..{after}"
    );
}

#[test]
fn show_text_prints_every_field_with_the_creation_time_in_local_time() {
    let home = TempHome::new("show-text");
    let (id, _, _) = created_message(&home);
    let json = show_in_utc_plus_nine(&home, &[&id, "--json"]);
    let json: Value = serde_json::from_str(&stdout(&json)).expect("show --json prints JSON");
    let created_at = json["created_at"].as_str().expect("created_at is a string");
    let local = DateTime::parse_from_rfc3339(created_at)
        .expect("created_at is RFC 3339")
        .with_timezone(&FixedOffset::east_opt(9 * 3600).expect("+09:00 is a valid offset"))
        .format("%Y-%m-%d %H:%M:%S +09:00");

    let output = show_in_utc_plus_nine(&home, &[&id]);

    assert!(output.status.success(), "show failed: {}", stderr(&output));
    assert!(output.stderr.is_empty(), "stderr was {}", stderr(&output));
    assert_eq!(
        stdout(&output),
        format!(
            "Id:       {id}\n\
             From:     alice\n\
             To:       bob, carol\n\
             Subject:  lunch?\n\
             Created:  {local}\n\
             \n\
             Noon at the usual place.\n"
        )
    );
}

#[test]
fn show_with_an_unknown_id_fails_on_stderr() {
    let home = TempHome::new("show-unknown");
    create(
        home.path(),
        &["--from", "alice", "--to", "bob", "--subject", "hello"],
    );
    let unknown = "00000000-0000-4000-8000-000000000000";

    let output = snailbox(home.path(), &["show", unknown]);

    assert!(!output.status.success(), "show of an unknown id succeeded");
    assert!(output.stdout.is_empty(), "stdout was {}", stdout(&output));
    assert_eq!(
        stderr(&output),
        format!("snailbox: no Message has the id {unknown}\n")
    );
}
