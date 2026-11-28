//! Which folder holds the Store, and that `create`, `show` and `list` each create it on first
//! use for its owner only. A Message created under one setting is shown again with `SNAILBOX_HOME`
//! pointing at the folder that setting should have chosen. Other tests read modes: a folder
//! snailbox creates is 700, and in a folder that already exists with mode 755 the database
//! file snailbox creates is 600 while the folder keeps 755.

mod common;

use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use common::{
    bare_command, create, create_with, snailbox, stderr, stdout, TempFolder, DATABASE_FILE_NAME,
};

const NEW_MESSAGE: [&str; 6] = ["--from", "alice", "--to", "bob", "--subject", "hello"];

fn create_with_env(env: &[(&str, &Path)]) -> String {
    let mut command = bare_command();
    for (name, value) in env {
        command.env(name, value);
    }
    create_with(command, &NEW_MESSAGE)
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
    let scratch = TempFolder::new("folder-snailbox-home");
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
    let scratch = TempFolder::new("folder-xdg");
    let xdg = scratch.path().join("xdg");

    let id = create_with_env(&[
        ("XDG_DATA_HOME", &xdg),
        ("HOME", &scratch.path().join("home")),
    ]);

    assert_shown_from(&xdg.join("snailbox"), &id);
}

#[test]
fn an_empty_snailbox_home_counts_as_unset() {
    let scratch = TempFolder::new("folder-empty-snailbox-home");
    let xdg = scratch.path().join("xdg");

    let id = create_with_env(&[
        ("SNAILBOX_HOME", Path::new("")),
        ("XDG_DATA_HOME", &xdg),
        ("HOME", &scratch.path().join("home")),
    ]);

    assert_shown_from(&xdg.join("snailbox"), &id);
}

#[test]
fn local_share_under_home_is_used_when_neither_variable_is_set() {
    let scratch = TempFolder::new("folder-home");
    let home = scratch.path().join("home");

    let id = create_with_env(&[("HOME", &home)]);

    assert_shown_from(&home.join(".local/share/snailbox"), &id);
}

#[test]
fn an_empty_xdg_data_home_counts_as_unset() {
    let scratch = TempFolder::new("folder-empty-xdg");
    let home = scratch.path().join("home");

    let id = create_with_env(&[("XDG_DATA_HOME", Path::new("")), ("HOME", &home)]);

    assert_shown_from(&home.join(".local/share/snailbox"), &id);
}

#[test]
fn create_makes_the_folder_for_its_owner_only() {
    let scratch = TempFolder::new("folder-create-mode");
    let folder = scratch.path().join("new");

    create(&folder, &NEW_MESSAGE);

    let mode = std::fs::metadata(&folder)
        .expect("create made the Store folder")
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o700, "mode was {:o}", mode & 0o777);
}

fn mode_of(path: &Path) -> u32 {
    std::fs::metadata(path)
        .expect("the path exists")
        .permissions()
        .mode()
        & 0o777
}

#[test]
fn a_fresh_home_gets_a_store_sqlite3_database_for_its_owner_only() {
    let scratch = TempFolder::new("folder-fresh-database");
    let folder = scratch.path().join("new");

    create(&folder, &NEW_MESSAGE);

    let database = mode_of(&folder.join("store.sqlite3"));
    assert_eq!(database, 0o600, "database mode was {database:o}");
}

#[test]
fn create_in_an_existing_755_folder_makes_the_database_for_its_owner_only() {
    let scratch = TempFolder::new("folder-existing-755");
    let folder = scratch.path().join("existing");
    std::fs::create_dir(&folder).expect("the test can make the folder");
    std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o755))
        .expect("the test can set the folder's mode");

    create(&folder, &NEW_MESSAGE);

    let database = mode_of(&folder.join(DATABASE_FILE_NAME));
    assert_eq!(database, 0o600, "database mode was {database:o}");
    let folder_mode = mode_of(&folder);
    assert_eq!(folder_mode, 0o755, "folder mode was {folder_mode:o}");
}

#[test]
fn show_makes_the_folder_for_its_owner_only() {
    let scratch = TempFolder::new("folder-show-mode");
    let folder = scratch.path().join("new");
    let unknown = "00000000-0000-4000-8000-000000000000";

    let output = snailbox(&folder, &["show", unknown]);

    assert!(output.stdout.is_empty(), "stdout was {}", stdout(&output));
    assert!(
        stderr(&output).starts_with(&format!(
            "snailbox: no Message has an id starting with {unknown};"
        )),
        "show did not reach the new Store: stderr was {}",
        stderr(&output)
    );
    let mode = std::fs::metadata(&folder)
        .expect("show made the Store folder")
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o700, "mode was {:o}", mode & 0o777);
}

#[test]
fn list_makes_the_folder_for_its_owner_only() {
    let scratch = TempFolder::new("folder-list-mode");
    let folder = scratch.path().join("new");

    let output = snailbox(&folder, &["list"]);

    assert!(output.status.success(), "list failed: {}", stderr(&output));
    assert!(output.stdout.is_empty(), "stdout was {}", stdout(&output));
    assert!(output.stderr.is_empty(), "stderr was {}", stderr(&output));
    let mode = std::fs::metadata(&folder)
        .expect("list made the Store folder")
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o700, "mode was {:o}", mode & 0o777);
}
