//! Where `create` takes a Message's body from, and the rules on it: at most one source, and a
//! file or standard input that is not blank.
//!
//! The command-line layer puts `BodySource::FLAGS` in one clap 4 argument group that allows at
//! most one of them, so clap refuses a caller who names two sources, in its own wording, before
//! `from_flags` runs. So `from_flags` takes flags that name at most one source; it does not
//! check that again.

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
    /// The ids of the flags that each name a Body source: the arguments `--body`, `--body-file`
    /// and `--stdin` of `create`. The one-source rule is that at most one of them is given; the
    /// command-line layer states it as a clap 4 argument group over these ids.
    pub const FLAGS: [&'static str; 3] = ["body", "body_file", "stdin"];

    /// The source the caller's `--body`, `--body-file` and `--stdin` flags name, or `None` when
    /// they name none. The caller has refused more than one (the argument group over `FLAGS`);
    /// with several, the first in `FLAGS` order wins.
    pub fn from_flags(
        body: Option<String>,
        body_file: Option<PathBuf>,
        stdin: bool,
    ) -> Option<Self> {
        body.map(Self::Text)
            .or(body_file.map(Self::File))
            .or(stdin.then_some(Self::Stdin))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_body_source_means_no_body() {
        assert!(BodySource::from_flags(None, None, false).is_none());
    }

    #[test]
    fn a_body_file_of_only_whitespace_is_refused() {
        let file = std::env::temp_dir().join(format!("snailbox-blank-{}", std::process::id()));
        std::fs::write(&file, "\n \t\n").expect("the file is written");

        let source = BodySource::from_flags(None, Some(file.clone()), false).expect("a source");
        let error = source.read().expect_err("the body is blank");

        assert_eq!(
            error.to_string(),
            format!(
                "the body file {} is empty or blank; pass --body for a short body or leave the \
                 body out for none",
                file.display()
            )
        );
        let _ = std::fs::remove_file(file);
    }
}
