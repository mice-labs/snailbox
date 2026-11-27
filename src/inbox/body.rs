//! Where `create` takes a Message's body from, and the rules on it: at most one source, and a
//! file or standard input that is not blank.
//!
//! The command-line layer hands `BodySource::FLAGS` and `BodySource::conflicts_with` to its argument parser, so a
//! caller who names two sources is refused by the parser, with its own wording, before
//! `from_flags` runs; `from_flags` decides the same rule for any other caller.

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
    /// The ids of the flags that each name a Body source, in the order `conflicts_with` ranks
    /// them: the arguments `--body`, `--body-file` and `--stdin` of `create`.
    pub const FLAGS: [&'static str; 3] = ["body", "body_file", "stdin"];

    /// The Body source flags `flag` cannot be given with, among those that rank after it in
    /// `FLAGS`; empty for any other argument. A parser told each pair once, from the earlier
    /// flag, names the pair in the order the caller typed it.
    pub fn conflicts_with(flag: &str) -> &'static [&'static str] {
        match Self::FLAGS.iter().position(|source| *source == flag) {
            Some(position) => &Self::FLAGS[position + 1..],
            None => &[],
        }
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_body_sources_are_refused_naming_the_first_two_in_flag_order() {
        let file = Some(PathBuf::from("body.txt"));
        let cases = [
            (
                (Some("a".to_owned()), file.clone(), false),
                ("--body", "--body-file"),
            ),
            ((Some("a".to_owned()), None, true), ("--body", "--stdin")),
            ((None, file.clone(), true), ("--body-file", "--stdin")),
            (
                (Some("a".to_owned()), file, true),
                ("--body", "--body-file"),
            ),
        ];

        for ((text, file, stdin), (first, second)) in cases {
            let error = BodySource::from_flags(text, file, stdin)
                .err()
                .expect("more than one source was accepted");

            assert_eq!(
                error.to_string(),
                format!("the argument '{first}' cannot be used with '{second}'")
            );
        }
    }

    #[test]
    fn no_body_source_means_no_body() {
        assert!(matches!(
            BodySource::from_flags(None, None, false),
            Ok(None)
        ));
    }

    #[test]
    fn each_source_flag_conflicts_with_the_ones_after_it() {
        assert_eq!(BodySource::conflicts_with("body"), ["body_file", "stdin"]);
        assert_eq!(BodySource::conflicts_with("body_file"), ["stdin"]);
        assert!(BodySource::conflicts_with("stdin").is_empty());
        assert!(BodySource::conflicts_with("subject").is_empty());
    }

    #[test]
    fn a_body_file_of_only_whitespace_is_refused() {
        let file = std::env::temp_dir().join(format!("snailbox-blank-{}", std::process::id()));
        std::fs::write(&file, "\n \t\n").expect("the file is written");

        let source = BodySource::from_flags(None, Some(file.clone()), false)
            .expect("one source")
            .expect("a source");
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
