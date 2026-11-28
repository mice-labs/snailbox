//! `create` rejects a Message missing its Sender, Receiver or subject, or giving one of them a
//! blank name or subject, takes its body from `--body`, `--body-file` or `--stdin` (at most
//! one), and refuses an empty or blank body file or standard input. Each test runs the built
//! binary against its own temporary `SNAILBOX_HOME`.

mod common;

use std::io::Write;
use std::path::Path;
use std::process::{Output, Stdio};

use common::{command_in, create, stderr, stdout, TempFolder};
use serde_json::Value;

/// Asserts that `output` is a failure with nothing on standard output and `expected` in the
/// error text.
fn assert_rejected(output: &Output, expected: &str) {
    assert!(!output.status.success(), "create succeeded");
    assert!(output.stdout.is_empty(), "stdout was {}", stdout(output));
    assert!(
        stderr(output).contains(expected),
        "stderr lacks {expected:?}: {}",
        stderr(output)
    );
}

/// Asserts that the error text tells the caller where to look next: the usage line and the
/// `--help` pointer.
fn assert_says_what_to_do_next(output: &Output) {
    let error = stderr(output);
    assert!(error.contains("Usage:"), "stderr lacks Usage: {error}");
    assert!(error.contains("--help"), "stderr lacks --help: {error}");
}

/// Runs `snailbox create` with `args` and `input` on standard input.
fn run_create(home: &Path, args: &[&str], input: &[u8]) -> Output {
    let mut child = command_in(home)
        .arg("create")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the snailbox binary runs");
    // A command that rejects its arguments exits without reading, closing the pipe early.
    let _ = child.stdin.take().expect("stdin is piped").write_all(input);
    child.wait_with_output().expect("the binary finishes")
}

fn shown_json(home: &Path, id: &str) -> Value {
    let output = common::snailbox(home, &["show", id, "--json"]);
    assert!(output.status.success(), "show failed: {}", stderr(&output));
    serde_json::from_str(&stdout(&output)).expect("show --json prints JSON")
}

const REQUIRED: [&str; 6] = ["--from", "alice", "--to", "bob", "--subject", "hi"];

/// The required flags without `flag` and its value.
fn without(flag: &str) -> Vec<&'static str> {
    let at = REQUIRED
        .iter()
        .position(|arg| *arg == flag)
        .expect("the flag is one of the required ones");
    let mut args = REQUIRED.to_vec();
    args.drain(at..at + 2);
    args
}

/// The required flags followed by `extra`.
fn with<'a>(extra: &[&'a str]) -> Vec<&'a str> {
    [&REQUIRED[..], extra].concat()
}

#[test]
fn create_without_from_fails_and_says_what_to_do_next() {
    let home = TempFolder::new("no-from");

    let output = run_create(home.path(), &without("--from"), b"");

    assert_rejected(&output, "--from");
    assert_says_what_to_do_next(&output);
}

#[test]
fn create_without_from_ignores_the_operating_system_user() {
    let home = TempFolder::new("no-from-env");
    let mut command = command_in(home.path());
    command.env("USER", "alice").env("LOGNAME", "alice");
    command.env("GIT_AUTHOR_NAME", "alice");
    command.arg("create");
    command.args(without("--from"));

    let output = command.output().expect("the snailbox binary runs");

    assert_rejected(&output, "--from");
}

#[test]
fn create_without_to_fails_and_says_what_to_do_next() {
    let home = TempFolder::new("no-to");

    let output = run_create(home.path(), &without("--to"), b"");

    assert_rejected(&output, "--to");
    assert_says_what_to_do_next(&output);
}

#[test]
fn create_without_subject_fails_and_says_what_to_do_next() {
    let home = TempFolder::new("no-subject");

    let output = run_create(home.path(), &without("--subject"), b"");

    assert_rejected(&output, "--subject");
    assert_says_what_to_do_next(&output);
}

#[test]
fn create_rejects_a_blank_sender_or_receiver() {
    let home = TempFolder::new("blank-name");
    let cases: [(&str, [&str; 6]); 4] = [
        ("--from", ["--from", "", "--to", "bob", "--subject", "hi"]),
        ("--from", ["--from", "  ", "--to", "bob", "--subject", "hi"]),
        ("--to", ["--from", "alice", "--to", "", "--subject", "hi"]),
        (
            "--to",
            ["--from", "alice", "--to", " \t", "--subject", "hi"],
        ),
    ];

    for (flag, args) in cases {
        let output = run_create(home.path(), &args, b"");

        assert_rejected(&output, flag);
        assert_rejected(&output, "the name is blank; give a name");
        assert_rejected(&output, "--help");
    }
}

#[test]
fn create_rejects_a_blank_subject_and_stores_nothing() {
    let home = TempFolder::new("blank-subject");

    for blank in ["", "  ", " \t"] {
        let args = ["--from", "alice", "--to", "bob", "--subject", blank];

        let output = run_create(home.path(), &args, b"");

        assert_rejected(&output, "the subject is blank; give a subject");
        assert_rejected(&output, "--subject");
        assert_rejected(&output, "--help");
    }
    let listing = common::snailbox(home.path(), &["list"]);
    assert!(stdout(&listing).is_empty(), "stored: {}", stdout(&listing));
}

#[test]
fn create_stores_a_non_blank_subject_as_given() {
    let home = TempFolder::new("subject-as-given");

    let id = create(
        home.path(),
        &["--from", "alice", "--to", "bob", "--subject", "  hi there "],
    );

    assert_eq!(shown_json(home.path(), &id)["subject"], "  hi there ");
}

#[test]
fn create_takes_the_body_from_a_file_verbatim() {
    let home = TempFolder::new("body-file");
    let scratch = TempFolder::new("body-file-source");
    let file = scratch.path().join("body.txt");
    std::fs::write(&file, "line one\n'quoted' \"text\" $HOME\n").expect("the file is written");

    let id = create(
        home.path(),
        &with(&["--body-file", file.to_str().expect("the path is UTF-8")]),
    );

    assert_eq!(
        shown_json(home.path(), &id)["body"],
        "line one\n'quoted' \"text\" $HOME\n"
    );
}

#[test]
fn create_takes_the_body_from_standard_input() {
    let home = TempFolder::new("body-stdin");

    let output = run_create(
        home.path(),
        &with(&["--stdin"]),
        b"piped text\nsecond line\n",
    );

    assert!(
        output.status.success(),
        "create failed: {}",
        stderr(&output)
    );
    let id = stdout(&output).trim_end().to_owned();
    assert_eq!(
        shown_json(home.path(), &id)["body"],
        "piped text\nsecond line\n"
    );
}

#[test]
fn create_without_a_body_source_stores_no_body() {
    let home = TempFolder::new("no-body");

    let id = create(home.path(), &with(&[]));

    assert!(shown_json(home.path(), &id)["body"].is_null());
}

#[test]
fn create_rejects_two_body_sources() {
    let home = TempFolder::new("two-sources");
    let scratch = TempFolder::new("two-sources-file");
    let file = scratch.path().join("body.txt");
    std::fs::write(&file, "text").expect("the file is written");
    let file = file.to_str().expect("the path is UTF-8");
    let cases: [&[&str]; 3] = [
        &["--body", "a", "--body-file", file],
        &["--body", "a", "--stdin"],
        &["--body-file", file, "--stdin"],
    ];

    for extra in cases {
        let output = run_create(home.path(), &with(extra), b"piped");

        assert_rejected(&output, "cannot be used with");
    }
}

#[test]
fn create_rejects_an_empty_body_file() {
    let home = TempFolder::new("empty-file");
    let scratch = TempFolder::new("empty-file-source");
    let file = scratch.path().join("body.txt");
    std::fs::write(&file, "").expect("the file is written");

    let output = run_create(
        home.path(),
        &with(&["--body-file", file.to_str().expect("the path is UTF-8")]),
        b"",
    );

    assert_rejected(&output, "is empty or blank");
}

#[test]
fn create_rejects_a_body_file_of_only_whitespace() {
    let home = TempFolder::new("blank-file");
    let scratch = TempFolder::new("blank-file-source");
    let file = scratch.path().join("body.txt");
    std::fs::write(&file, "\n \t\n").expect("the file is written");

    let output = run_create(
        home.path(),
        &with(&["--body-file", file.to_str().expect("the path is UTF-8")]),
        b"",
    );

    assert_rejected(&output, "is empty or blank");
}

#[test]
fn create_rejects_empty_standard_input() {
    let home = TempFolder::new("empty-stdin");

    let output = run_create(home.path(), &with(&["--stdin"]), b"");

    assert_rejected(&output, "standard input is empty or blank");
}

#[test]
fn create_rejects_standard_input_of_only_whitespace() {
    let home = TempFolder::new("blank-stdin");

    let output = run_create(home.path(), &with(&["--stdin"]), b"\n \t\n");

    assert_rejected(&output, "standard input is empty or blank");
}

#[test]
fn create_rejects_a_missing_body_file_and_says_which() {
    let home = TempFolder::new("missing-file");

    let output = run_create(
        home.path(),
        &with(&["--body-file", "/nonexistent/snailbox-body.txt"]),
        b"",
    );

    assert_rejected(&output, "/nonexistent/snailbox-body.txt");
}

#[test]
fn create_keeps_a_long_subject_and_body_whole() {
    let home = TempFolder::new("long");
    let subject = "s".repeat(100_000);
    let body = "b".repeat(1_000_000);

    let output = run_create(
        home.path(),
        &[
            "--from",
            "alice",
            "--to",
            "bob",
            "--subject",
            &subject,
            "--stdin",
        ],
        body.as_bytes(),
    );

    assert!(
        output.status.success(),
        "create failed: {}",
        stderr(&output)
    );
    let id = stdout(&output).trim_end().to_owned();
    let shown = shown_json(home.path(), &id);
    assert_eq!(shown["subject"], subject.as_str());
    assert_eq!(shown["body"], body.as_str());
}
