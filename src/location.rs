//! Where the Inbox lives on this machine, and making that folder on first use.

use std::ffi::OsString;
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// The Inbox folder: `SNAILBOX_HOME` when set, else `$XDG_DATA_HOME/snailbox` when that is
/// set, else `~/.local/share/snailbox`. An empty variable counts as unset, as the XDG Base
/// Directory specification has it.
pub fn inbox_folder() -> Result<PathBuf> {
    if let Some(home) = non_empty_var("SNAILBOX_HOME") {
        return Ok(PathBuf::from(home));
    }
    if let Some(data_home) = non_empty_var("XDG_DATA_HOME") {
        return Ok(PathBuf::from(data_home).join("snailbox"));
    }
    let home = non_empty_var("HOME")
        .context("cannot find the Inbox folder: SNAILBOX_HOME, XDG_DATA_HOME and HOME are unset")?;
    Ok(PathBuf::from(home).join(".local/share/snailbox"))
}

/// Creates `folder` if it is missing, readable and writable by its owner only. Missing
/// parents get the default permissions, since they are shared folders such as
/// `~/.local/share`; an existing folder keeps whatever permissions it has.
pub fn ensure_folder(folder: &Path) -> Result<()> {
    if let Some(parent) = folder.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("cannot create {}", parent.display()))?;
    }
    match std::fs::DirBuilder::new().mode(0o700).create(folder) {
        Err(error) if error.kind() != std::io::ErrorKind::AlreadyExists => {
            Err(error).with_context(|| format!("cannot create {}", folder.display()))
        }
        _ => Ok(()),
    }
}

fn non_empty_var(name: &str) -> Option<OsString> {
    std::env::var_os(name).filter(|value| !value.is_empty())
}
