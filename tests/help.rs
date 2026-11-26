//! Runs the built `snailbox` binary with `--help` and `create --help` and checks stdout, the
//! exit code and, for `--help`, stderr.

mod common;

use common::{snailbox, stdout, TempFolder};

/// The command names listed under the `Commands:` heading of clap's help.
fn listed_commands(help: &str) -> Vec<&str> {
    help.lines()
        .skip_while(|line| *line != "Commands:")
        .skip(1)
        .take_while(|line| !line.trim().is_empty())
        .filter_map(|line| line.split_whitespace().next())
        .collect()
}

#[test]
fn help_exits_zero_and_lists_the_three_commands() {
    let home = TempFolder::new("help");
    let output = snailbox(home.path(), &["--help"]);

    let stdout = stdout(&output);
    assert!(output.status.success(), "exit status was {}", output.status);
    let listed = listed_commands(&stdout);
    for command in ["create", "list", "show"] {
        assert!(
            listed.contains(&command),
            "Commands lacks {command}: {stdout}"
        );
    }
    assert!(output.stderr.is_empty(), "stderr was {:?}", output.stderr);
}

#[test]
fn create_help_describes_every_option() {
    let home = TempFolder::new("create-help");
    let output = snailbox(home.path(), &["create", "--help"]);

    let stdout = stdout(&output);
    assert!(output.status.success(), "exit status was {}", output.status);
    for option in [
        "--from",
        "--to",
        "--subject",
        "--body ",
        "--body-file",
        "--stdin",
    ] {
        let line = stdout
            .lines()
            .map(str::trim_start)
            .find(|line| line.starts_with(option))
            .unwrap_or_else(|| panic!("create --help lacks {option}: {stdout}"));
        let description = line
            .split_once("  ")
            .map(|(_, rest)| rest.trim())
            .unwrap_or_default();
        assert!(
            !description.is_empty(),
            "{option} has no description: {line:?}"
        );
    }
}
