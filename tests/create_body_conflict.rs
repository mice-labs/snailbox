//! `create` refuses two or more of `--body`, `--body-file` and `--stdin` with clap's own error
//! text: the flags named in the order they were typed, each with its value placeholder, and a
//! usage line that starts `snailbox create` and ends with the first of them. Each test runs the
//! built binary against its own temporary `SNAILBOX_HOME`.

mod common;

use common::{snailbox, stderr, stdout, TempFolder};

const REQUIRED: [&str; 6] = ["--from", "a", "--to", "b", "--subject", "s"];

/// The text clap prints for a conflict worded `lines`, with a usage line ending `usage_tail`.
fn refusal(lines: &str, usage_tail: &str) -> String {
    format!(
        "error: the argument {lines}\n\nUsage: snailbox create --from <FROM> --to <TO> \
         --subject <SUBJECT> {usage_tail}\n\nFor more information, try '--help'.\n"
    )
}

fn assert_refused(args: &[&str], expected: &str) {
    let home = TempFolder::new("body-conflict");
    let mut all = vec!["create"];
    all.extend(args);

    let output = snailbox(home.path(), &all);

    assert_eq!(output.status.code(), Some(2), "for {args:?}");
    assert!(output.stdout.is_empty(), "stdout was {}", stdout(&output));
    assert_eq!(stderr(&output), expected, "for {args:?}");
}

/// The flags `create` needs besides a Body source, then `extra`.
fn with_required<'a>(extra: &[&'a str]) -> Vec<&'a str> {
    let mut args = REQUIRED.to_vec();
    args.extend(extra);
    args
}

#[test]
fn body_then_stdin_names_body_first() {
    assert_refused(
        &with_required(&["--body", "a", "--stdin"]),
        &refusal(
            "'--body <BODY>' cannot be used with '--stdin'",
            "--body <BODY>",
        ),
    );
}

#[test]
fn stdin_then_body_names_stdin_first() {
    assert_refused(
        &with_required(&["--stdin", "--body", "a"]),
        &refusal("'--stdin' cannot be used with '--body <BODY>'", "--stdin"),
    );
}

#[test]
fn body_then_body_file_names_body_first() {
    assert_refused(
        &with_required(&["--body", "a", "--body-file", "f.txt"]),
        &refusal(
            "'--body <BODY>' cannot be used with '--body-file <BODY_FILE>'",
            "--body <BODY>",
        ),
    );
}

#[test]
fn body_file_then_body_names_body_file_first() {
    assert_refused(
        &with_required(&["--body-file", "f.txt", "--body", "a"]),
        &refusal(
            "'--body-file <BODY_FILE>' cannot be used with '--body <BODY>'",
            "--body-file <BODY_FILE>",
        ),
    );
}

#[test]
fn body_file_then_stdin_names_body_file_first() {
    assert_refused(
        &with_required(&["--body-file", "f.txt", "--stdin"]),
        &refusal(
            "'--body-file <BODY_FILE>' cannot be used with '--stdin'",
            "--body-file <BODY_FILE>",
        ),
    );
}

#[test]
fn stdin_then_body_file_names_stdin_first() {
    assert_refused(
        &with_required(&["--stdin", "--body-file", "f.txt"]),
        &refusal(
            "'--stdin' cannot be used with '--body-file <BODY_FILE>'",
            "--stdin",
        ),
    );
}

#[test]
fn all_three_sources_list_both_conflicts() {
    assert_refused(
        &with_required(&["--body", "a", "--body-file", "f.txt", "--stdin"]),
        &refusal(
            "'--body <BODY>' cannot be used with:\n  --body-file <BODY_FILE>\n  --stdin",
            "--body <BODY>",
        ),
    );
    assert_refused(
        &with_required(&["--stdin", "--body-file", "f.txt", "--body", "a"]),
        &refusal(
            "'--stdin' cannot be used with:\n  --body-file <BODY_FILE>\n  --body <BODY>",
            "--stdin",
        ),
    );
}

#[test]
fn a_conflict_is_reported_before_a_missing_receiver() {
    assert_refused(
        &["--from", "a", "--subject", "s", "--body", "a", "--stdin"],
        &refusal(
            "'--body <BODY>' cannot be used with '--stdin'",
            "--body <BODY>",
        ),
    );
}
