//! Runs the built `snailbox` binary with `--help` and checks stdout, stderr and the exit code.
//! The binary does not read `SNAILBOX_HOME` yet; the variable is set to a throwaway folder so
//! this test cannot touch a real home once it does.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

/// A temporary `SNAILBOX_HOME`, unique per call and removed on drop, so a panicking test
/// still cleans up.
struct TempHome(PathBuf);

impl TempHome {
    fn new(name: &str) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let home = std::env::temp_dir().join(format!(
            "snailbox-{name}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&home).expect("the temporary SNAILBOX_HOME can be created");
        Self(home)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempHome {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn snailbox(home: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_snailbox"))
        .env("SNAILBOX_HOME", home)
        .args(args)
        .output()
        .expect("the snailbox binary runs")
}

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

    let stdout = String::from_utf8(output.stdout).expect("stdout is UTF-8");
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
