//! Runs the built `snailbox` binary with `--help` and checks stdout, stderr and the exit code.

mod common;

use common::{snailbox, stdout, TempHome};

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
    let home = TempHome::new("help");
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
