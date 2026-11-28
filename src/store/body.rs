//! Where `create` takes a Message's body from, and the rule on it: a file or standard input that
//! is not blank, and a `--body` that is not empty (`Body`, which clap applies while parsing).
//!
//! This module holds one source. The command-line layer guarantees that at most one is given: it
//! states that to clap, which refuses a caller who names two.

use std::io::Read;
use std::path::PathBuf;

use anyhow::{bail, Context, Result};

/// A `--body` text, which must not be empty: a stored empty body would differ from a Message
/// with no body. Any other text is kept as given, whitespace included.
#[derive(Clone, Debug)]
pub struct Body(String);

impl std::str::FromStr for Body {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        if text.is_empty() {
            return Err("the body is empty; give a body or leave the body out".to_owned());
        }
        Ok(Self(text.to_owned()))
    }
}

/// The one place a body comes from, when the caller names one.
pub enum BodySource {
    Text(Body),
    File(PathBuf),
    Stdin,
}

impl BodySource {
    /// The body this source gives. A file or standard input that is empty or holds only
    /// whitespace is an error, so an upstream command that failed before printing anything
    /// cannot leave a blank Message; one that failed after printing some text still gives that
    /// text as the body. A `--body` is taken as given; `Body` has already refused an empty one.
    pub fn read(self) -> Result<String> {
        match self {
            Self::Text(Body(text)) => Ok(text),
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
    fn an_empty_body_is_refused() {
        let error = "".parse::<Body>().err();

        assert_eq!(
            error.as_deref(),
            Some("the body is empty; give a body or leave the body out")
        );
    }

    #[test]
    fn a_body_is_kept_as_given() {
        let body: Body = " \n".parse().expect("the body is not empty");

        assert_eq!(body.0, " \n");
    }

    #[test]
    fn a_body_file_of_only_whitespace_is_refused() {
        let file = std::env::temp_dir().join(format!("snailbox-blank-{}", std::process::id()));
        std::fs::write(&file, "\n \t\n").expect("the file is written");

        let error = BodySource::File(file.clone())
            .read()
            .expect_err("the body is blank");

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
