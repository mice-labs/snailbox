//! Where `create` takes a Message's body from, and the rules on it: at most one source, and a
//! file or standard input that is not blank.

use std::io::Read;
use std::path::PathBuf;

use anyhow::{bail, Context, Result};

/// The one place a body comes from, when the caller names one.
pub enum BodySource {
    Text(String),
    File(PathBuf),
    Stdin,
}

/// Two Body source flags were given; a Message has at most one.
#[derive(Debug)]
pub struct BodyConflict {
    pub first: &'static str,
    pub second: &'static str,
}

impl std::fmt::Display for BodyConflict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "the argument '{}' cannot be used with '{}'",
            self.first, self.second
        )
    }
}

impl std::error::Error for BodyConflict {}

impl BodySource {
    /// The source the caller's `--body`, `--body-file` and `--stdin` flags name, or `None` when
    /// they name none. Naming more than one is an error, which names the first two in flag
    /// order.
    pub fn from_flags(
        body: Option<String>,
        body_file: Option<PathBuf>,
        stdin: bool,
    ) -> Result<Option<Self>, BodyConflict> {
        let mut given = Vec::new();
        let mut source = None;
        if let Some(text) = body {
            given.push("--body");
            source = Some(Self::Text(text));
        }
        if let Some(path) = body_file {
            given.push("--body-file");
            source = Some(Self::File(path));
        }
        if stdin {
            given.push("--stdin");
            source = Some(Self::Stdin);
        }
        match given[..] {
            [first, second, ..] => Err(BodyConflict { first, second }),
            _ => Ok(source),
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
