//! Helpers every command-line test shares: a throwaway folder to point `SNAILBOX_HOME` (or
//! another location variable) at, and a way to run the built binary against it.

// Each test file compiles its own copy of this module and uses only some of the helpers.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

/// A temporary folder, unique per call and removed on drop, so a panicking test still cleans
/// up.
pub struct TempFolder(PathBuf);

impl TempFolder {
    pub fn new(name: &str) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let folder = std::env::temp_dir().join(format!(
            "snailbox-{name}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&folder).expect("the temporary folder can be created");
        Self(folder)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempFolder {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The name of the Store's database file, inside the Store folder.
pub const DATABASE_FILE_NAME: &str = "store.sqlite3";

/// The binary with no Store location in its environment, so each test sets the one it means.
pub fn bare_command() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_snailbox"));
    command
        .env_remove("SNAILBOX_HOME")
        .env_remove("XDG_DATA_HOME");
    command
}

/// The binary with `SNAILBOX_HOME` set to `home`.
pub fn command_in(home: &Path) -> Command {
    let mut command = bare_command();
    command.env("SNAILBOX_HOME", home);
    command
}

/// Runs the binary with `SNAILBOX_HOME` set to `home`.
pub fn snailbox(home: &Path, args: &[&str]) -> Output {
    command_in(home)
        .args(args)
        .output()
        .expect("the snailbox binary runs")
}

pub fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("stdout is UTF-8")
}

pub fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("stderr is UTF-8")
}

/// Runs `create` with `SNAILBOX_HOME` set to `home` and returns the id it printed, failing the
/// test if the command failed.
pub fn create(home: &Path, args: &[&str]) -> String {
    create_with(command_in(home), args)
}

/// Stores a Message from `from` to `to` with `subject` and returns its id.
pub fn store(home: &TempFolder, from: &str, to: &[&str], subject: &str) -> String {
    let mut args = vec!["--from", from, "--subject", subject];
    for name in to {
        args.push("--to");
        args.push(name);
    }
    create(home.path(), &args)
}

/// Stores Messages until two ids share their first character, and returns that character with
/// both ids, the earlier stored first. Ids are random, so this takes a handful of Messages, not
/// a fixed number.
pub fn two_ids_sharing_a_first_character(home: &TempFolder) -> (String, String, String) {
    let mut seen: Vec<String> = Vec::new();
    loop {
        let id = store(home, "alice", &["bob"], "m");
        let first = id.chars().next().expect("an id is not empty");
        if let Some(earlier) = seen.iter().find(|other| other.starts_with(first)) {
            return (first.to_string(), earlier.clone(), id);
        }
        seen.push(id);
    }
}

/// The subjects `list` printed with `args`, one per line, in the order printed. Text lines are
/// tab-separated and the subject is the last field; the one line without a tab that is allowed,
/// the notice that the limit left Messages out, is skipped, and any other tab-less line fails
/// the test.
pub fn listed_subjects(home: &TempFolder, args: &[&str]) -> Vec<String> {
    let output = snailbox(home.path(), &[&["list"], args].concat());
    assert!(output.status.success(), "list failed: {}", stderr(&output));
    assert!(output.stderr.is_empty(), "stderr was {}", stderr(&output));
    let text = stdout(&output);
    for line in text.lines().filter(|line| !line.contains('\t')) {
        assert!(
            line.contains("left out by the limit"),
            "a line that is neither a Message nor the notice: {line}"
        );
    }
    text.lines()
        .filter(|line| line.contains('\t'))
        .map(|line| {
            line.rsplit('\t')
                .next()
                .expect("a subject field")
                .to_owned()
        })
        .collect()
}

/// The Messages `list --json` printed with `args`, in the order printed, failing the test unless
/// it succeeded with nothing on standard error.
pub fn listed_messages(home: &TempFolder, args: &[&str]) -> Vec<serde_json::Value> {
    let output = snailbox(home.path(), &[&["list", "--json"], args].concat());
    assert!(output.status.success(), "list failed: {}", stderr(&output));
    assert!(output.stderr.is_empty(), "stderr was {}", stderr(&output));
    let mut listed: serde_json::Value =
        serde_json::from_str(&stdout(&output)).expect("list --json prints JSON");
    match listed["messages"].take() {
        serde_json::Value::Array(messages) => messages,
        other => panic!("messages is not an array: {other}"),
    }
}

/// The full ids of the Messages `listed_messages` returns with `args`, in the order printed.
pub fn listed_ids(home: &TempFolder, args: &[&str]) -> Vec<String> {
    listed_messages(home, args)
        .iter()
        .map(|message| message["id"].as_str().expect("an id").to_owned())
        .collect()
}

/// What `list` printed on standard error with `args`, failing the test unless it exited
/// non-zero with nothing on standard output.
pub fn failed_list(home: &TempFolder, args: &[&str]) -> String {
    let output = snailbox(home.path(), &[&["list"], args].concat());
    assert!(
        !output.status.success(),
        "list succeeded: {}",
        stdout(&output)
    );
    assert!(output.stdout.is_empty(), "stdout was {}", stdout(&output));
    stderr(&output)
}

/// Runs `create` through `command`, whose environment the caller has set, and returns the id
/// it printed, failing the test if the command failed.
pub fn create_with(mut command: Command, args: &[&str]) -> String {
    let output = command
        .arg("create")
        .args(args)
        .output()
        .expect("the snailbox binary runs");
    id_from(&output)
}

/// The id a `create` printed, failing the test if that `create` failed.
pub fn id_from(output: &Output) -> String {
    assert!(
        output.status.success(),
        "create failed with {}: {}",
        output.status,
        stderr(output)
    );
    stdout(output).trim_end_matches('\n').to_owned()
}
