//! Helpers every command-line test shares: a throwaway `SNAILBOX_HOME` and a way to run the
//! built binary against it.

// Each test file compiles its own copy of this module and uses only some of the helpers.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

/// A temporary folder, unique per call and removed on drop, so a panicking test still cleans
/// up.
pub struct TempHome(PathBuf);

impl TempHome {
    pub fn new(name: &str) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let home = std::env::temp_dir().join(format!(
            "snailbox-{name}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&home).expect("the temporary SNAILBOX_HOME can be created");
        Self(home)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempHome {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The binary with no Inbox location in its environment, so each test sets the one it means.
pub fn bare_command() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_snailbox"));
    command
        .env_remove("SNAILBOX_HOME")
        .env_remove("XDG_DATA_HOME");
    command
}

/// Runs the binary with `SNAILBOX_HOME` set to `home`.
pub fn snailbox(home: &Path, args: &[&str]) -> Output {
    bare_command()
        .env("SNAILBOX_HOME", home)
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

/// Runs `create` and returns the id it printed, failing the test if the command failed.
pub fn create(home: &Path, args: &[&str]) -> String {
    let output = snailbox(home, &[&["create"], args].concat());
    assert!(
        output.status.success(),
        "create failed with {}: {}",
        output.status,
        stderr(&output)
    );
    stdout(&output).trim_end_matches('\n').to_owned()
}
