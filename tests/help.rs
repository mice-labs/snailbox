//! Runs the built `snailbox` binary against a temporary `SNAILBOX_HOME` and checks stdout,
//! stderr and the exit code, the pattern every command test follows.

use std::path::PathBuf;
use std::process::{Command, Output};

fn temp_home(name: &str) -> PathBuf {
    let home = std::env::temp_dir().join(format!("snailbox-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&home).expect("the temporary SNAILBOX_HOME can be created");
    home
}

fn snailbox(home: &PathBuf, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_snailbox"))
        .env("SNAILBOX_HOME", home)
        .args(args)
        .output()
        .expect("the snailbox binary runs")
}

#[test]
fn help_exits_zero_and_names_the_three_commands() {
    let home = temp_home("help");
    let output = snailbox(&home, &["--help"]);
    std::fs::remove_dir_all(&home).expect("the temporary SNAILBOX_HOME can be removed");

    let stdout = String::from_utf8(output.stdout).expect("stdout is UTF-8");
    assert!(output.status.success(), "exit status was {}", output.status);
    for command in ["create", "list", "show"] {
        assert!(stdout.contains(command), "stdout lacks {command}: {stdout}");
    }
    assert!(output.stderr.is_empty(), "stderr was {:?}", output.stderr);
}
