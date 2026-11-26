//! `create` rejects a Message missing its Sender, Receiver or subject, takes its body from
//! `--body`, `--body-file` or `--stdin` (at most one), and refuses an empty body file or
//! standard input. Each test runs the built binary against its own temporary `SNAILBOX_HOME`.

mod common;

use std::io::Write;
use std::path::Path;
use std::process::{Output, Stdio};

use common::{command_in, create, snailbox, stderr, stdout, TempFolder};
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

/// Runs the binary with `args` and `input` on standard input.
fn create_with_stdin(home: &Path, args: &[&str], input: &[u8]) -> Output {
    let mut child = command_in(home)
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
    let output = snailbox(home, &["show", id, "--json"]);
    assert!(output.status.success(), "show failed: {}", stderr(&output));
    serde_json::from_str(&stdout(&output)).expect("show --json prints JSON")
}

const REQUIRED: [&str; 6] = ["--from", "alice", "--to", "bob", "--subject", "hi"];

fn without(flag: &str) -> Vec<&'static str> {
    let at = REQUIRED
        .iter()
        .position(|arg| *arg == flag)
        .expect("the flag is one of the required ones");
    let mut args = REQUIRED.to_vec();
    args.drain(at..at + 2);
    args
}

fn create_args<'a>(extra: &[&'a str]) -> Vec<&'a str> {
    let mut args = vec!["create"];
    args.extend(REQUIRED);
    args.extend(extra);
    args
}

#[test]
fn create_without_from_fails_and_names_the_flag() {
    let home = TempFolder::new("no-from");
    let mut args = vec!["create"];
    args.extend(without("--from"));

    let output = snailbox(home.path(), &args);

    assert_rejected(&output, "--from");
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
fn create_without_to_fails_and_names_the_flag() {
    let home = TempFolder::new("no-to");
    let mut args = vec!["create"];
    args.extend(without("--to"));

    let output = snailbox(home.path(), &args);

    assert_rejected(&output, "--to");
}

#[test]
fn create_without_subject_fails_and_names_the_flag() {
    let home = TempFolder::new("no-subject");
    let mut args = vec!["create"];
    args.extend(without("--subject"));

    let output = snailbox(home.path(), &args);

    assert_rejected(&output, "--subject");
}

#[test]
fn create_takes_the_body_from_a_file_verbatim() {
    let home = TempFolder::new("body-file");
    let scratch = TempFolder::new("body-file-source");
    let file = scratch.path().join("body.txt");
    std::fs::write(&file, "line one\n'quoted' \"text\" $HOME\n").expect("the file is written");

    let id = create(
        home.path(),
        &create_args(&["--body-file", file.to_str().expect("the path is UTF-8")])[1..],
    );

    assert_eq!(
        shown_json(home.path(), &id)["body"],
        "line one\n'quoted' \"text\" $HOME\n"
    );
}

#[test]
fn create_takes_the_body_from_standard_input() {
    let home = TempFolder::new("body-stdin");

    let output = create_with_stdin(
        home.path(),
        &create_args(&["--stdin"]),
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

    let id = create(home.path(), &create_args(&[])[1..]);

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
        let output = create_with_stdin(home.path(), &create_args(extra), b"piped");

        assert_rejected(&output, "cannot be used with");
    }
}

#[test]
fn create_rejects_an_empty_body_file() {
    let home = TempFolder::new("empty-file");
    let scratch = TempFolder::new("empty-file-source");
    let file = scratch.path().join("body.txt");
    std::fs::write(&file, "").expect("the file is written");

    let output = snailbox(
        home.path(),
        &create_args(&["--body-file", file.to_str().expect("the path is UTF-8")]),
    );

    assert_rejected(&output, "is empty");
}

#[test]
fn create_rejects_empty_standard_input() {
    let home = TempFolder::new("empty-stdin");

    let output = create_with_stdin(home.path(), &create_args(&["--stdin"]), b"");

    assert_rejected(&output, "standard input is empty");
}

#[test]
fn create_rejects_standard_input_of_only_whitespace() {
    let home = TempFolder::new("blank-stdin");

    let output = create_with_stdin(home.path(), &create_args(&["--stdin"]), b"\n \t\n");

    assert_rejected(&output, "standard input is empty");
}

#[test]
fn create_rejects_a_missing_body_file_and_says_which() {
    let home = TempFolder::new("missing-file");

    let output = snailbox(
        home.path(),
        &create_args(&["--body-file", "/nonexistent/snailbox-body.txt"]),
    );

    assert_rejected(&output, "/nonexistent/snailbox-body.txt");
}

#[test]
fn create_keeps_a_long_subject_and_body_whole() {
    let home = TempFolder::new("long");
    let subject = "s".repeat(100_000);
    let body = "b".repeat(1_000_000);

    let output = create_with_stdin(
        home.path(),
        &[
            "create",
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
