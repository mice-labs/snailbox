//! Runs the built `snailbox` binary with `--help` and each command's `--help` and checks
//! stdout, the exit code and, for `--help`, stderr.

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
        "--reply-to",
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

/// The help text for the argument or option `marker` names, its whitespace collapsed so line
/// wrapping does not matter, up to the next option or section heading.
fn help_entry(help: &str, marker: &str) -> String {
    let words = help.split_whitespace().collect::<Vec<_>>().join(" ");
    let start = words
        .rfind(marker)
        .unwrap_or_else(|| panic!("no {marker} in {help}"));
    let entry = &words[start + marker.len()..];
    let end = [" -", " Options:"]
        .iter()
        .filter_map(|next| entry.find(next))
        .min()
        .unwrap_or(entry.len());
    entry[..end].trim().to_owned()
}

#[test]
fn every_id_taking_argument_and_flag_says_it_takes_a_message_id_not_a_date_or_time() {
    let home = TempFolder::new("id-help");

    for (command, marker, expected) in [
        (
            "show",
            "<ID>",
            "The Message's id, or a prefix of it matching only that Message (not a date or time)",
        ),
        (
            "create",
            "--reply-to <ID>",
            "Store the Message as a Reply to this Message, named by its Message id or a unique \
             prefix of it (not a date or time)",
        ),
        (
            "list",
            "--after <ID>",
            "Only Messages stored after this one, named by its Message id or a unique prefix of \
             it (not a date or time)",
        ),
        (
            "list",
            "--before <ID>",
            "Only Messages stored before this one, named by its Message id or a unique prefix of \
             it (not a date or time)",
        ),
        (
            "list",
            "--upthread <ID>",
            "Only this Message, named by its Message id or a unique prefix of it (not a date or \
             time), and the chain of Messages it replies to, back to one that is not a Reply",
        ),
        (
            "list",
            "--downthread <ID>",
            "Only this Message, named by its Message id or a unique prefix of it (not a date or \
             time), and every Reply beneath it at any depth",
        ),
        (
            "list",
            "--thread <ID>",
            "Only this Message, named by its Message id or a unique prefix of it (not a date or \
             time), with everything upthread and downthread of it; Replies on other branches are \
             left out",
        ),
    ] {
        let output = snailbox(home.path(), &[command, "--help"]);

        assert!(
            output.status.success(),
            "{command} --help exited {}",
            output.status
        );
        assert_eq!(
            help_entry(&stdout(&output), marker),
            expected,
            "{command} --help entry for {marker}"
        );
    }
}
