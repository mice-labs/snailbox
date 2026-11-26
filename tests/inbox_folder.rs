//! Which folder holds the Inbox, and that any command creates it on first use for its owner
//! only. A Message created under one setting is shown again with `SNAILBOX_HOME` pointing at
//! the folder that setting should have chosen, so the tests never open the database file.

mod common;

use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use common::{bare_command, create, snailbox, stderr, TempHome};

const NEW_MESSAGE: [&str; 6] = ["--from", "alice", "--to", "bob", "--subject", "hello"];

fn create_with_env(env: &[(&str, &Path)]) -> String {
    let mut command = bare_command();
    for (name, value) in env {
        command.env(name, value);
    }
    let output = command
        .arg("create")
        .args(NEW_MESSAGE)
        .output()
        .expect("the snailbox binary runs");
    assert!(
        output.status.success(),
        "create failed: {}",
        stderr(&output)
    );
    common::stdout(&output).trim_end().to_owned()
}

fn assert_shown_from(folder: &Path, id: &str) {
    let output = snailbox(folder, &["show", id]);
    assert!(
        output.status.success(),
        "{id} is not in {}: {}",
        folder.display(),
        stderr(&output)
    );
}

#[test]
fn snailbox_home_wins_over_xdg_data_home_and_home() {
    let scratch = TempHome::new("folder-snailbox-home");
    let chosen = scratch.path().join("chosen");

    let id = create_with_env(&[
        ("SNAILBOX_HOME", &chosen),
        ("XDG_DATA_HOME", &scratch.path().join("xdg")),
        ("HOME", &scratch.path().join("home")),
    ]);

    assert_shown_from(&chosen, &id);
}

#[test]
fn xdg_data_home_is_used_when_snailbox_home_is_unset() {
    let scratch = TempHome::new("folder-xdg");
    let xdg = scratch.path().join("xdg");

    let id = create_with_env(&[
        ("XDG_DATA_HOME", &xdg),
        ("HOME", &scratch.path().join("home")),
    ]);

    assert_shown_from(&xdg.join("snailbox"), &id);
}

#[test]
fn local_share_under_home_is_used_when_neither_variable_is_set() {
    let scratch = TempHome::new("folder-home");
    let home = scratch.path().join("home");

    let id = create_with_env(&[("HOME", &home)]);

    assert_shown_from(&home.join(".local/share/snailbox"), &id);
}

#[test]
fn create_makes_the_folder_for_its_owner_only() {
    let scratch = TempHome::new("folder-create-mode");
    let folder = scratch.path().join("new");

    create(&folder, &NEW_MESSAGE);

    let mode = std::fs::metadata(&folder)
        .expect("create made the Inbox folder")
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o700, "mode was {:o}", mode & 0o777);
}

#[test]
fn show_makes_the_folder_for_its_owner_only() {
    let scratch = TempHome::new("folder-show-mode");
    let folder = scratch.path().join("new");

    snailbox(&folder, &["show", "00000000-0000-4000-8000-000000000000"]);

    let mode = std::fs::metadata(&folder)
        .expect("show made the Inbox folder")
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o700, "mode was {:o}", mode & 0o777);
}
