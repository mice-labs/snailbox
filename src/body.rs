//! Where `create` takes a Message's body from.

use std::io::Read;
use std::path::PathBuf;

use anyhow::{bail, Context, Result};

/// The one place a body comes from, when the caller names one.
pub enum BodySource {
    Text(String),
    File(PathBuf),
    Stdin,
}

impl BodySource {
    /// The source the caller's `--body`, `--body-file` and `--stdin` flags name, or `None` when
    /// they name none. The command line allows at most one of them, so the order here only
    /// settles what cannot happen.
    pub fn from_flags(
        body: Option<String>,
        body_file: Option<PathBuf>,
        stdin: bool,
    ) -> Option<Self> {
        match (body, body_file, stdin) {
            (Some(text), _, _) => Some(Self::Text(text)),
            (None, Some(path), _) => Some(Self::File(path)),
            (None, None, true) => Some(Self::Stdin),
            (None, None, false) => None,
        }
    }

    /// The body this source gives. A file or standard input that is empty or holds only
    /// whitespace is an error, so an upstream command that failed before printing anything
    /// cannot leave a blank Message; one that failed after printing some text still gives that
    /// text as the body. `--body` is taken as given.
    pub fn read(self) -> Result<String> {
        match self {
            Self::Text(text) => Ok(text),
            Self::File(path) => {
                let text = std::fs::read_to_string(&path)
                    .with_context(|| format!("cannot read the body file {}", path.display()))?;
                not_blank(
                    text,
                    &format!("the body file {} is empty or blank", path.display()),
                )
            }
            Self::Stdin => {
                let mut text = String::new();
                std::io::stdin()
                    .read_to_string(&mut text)
                    .context("cannot read the body from standard input")?;
                not_blank(text, "standard input is empty or blank")
            }
        }
    }
}

fn not_blank(text: String, complaint: &str) -> Result<String> {
    if text.trim().is_empty() {
        bail!("{complaint}; pass --body for a short body or leave the body out for none");
    }
    Ok(text)
}
