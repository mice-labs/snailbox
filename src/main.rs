mod inbox;
mod listing;
mod location;
mod message;

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::Result;
use clap::{Args, Parser, Subcommand};

use inbox::{BodySource, IdArgument, Inbox, Limit, ListFilter, NoMessageFor};
use listing::{AFTER_FLAG, BEFORE_FLAG};
use message::{IdPrefix, Name, NewMessage};

#[derive(Parser)]
#[command(
    version,
    about = "A local Inbox of Messages between Senders and Receivers"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

/// The flags that each name where a Message's body comes from. clap refuses a caller who gives
/// more than one, in its own wording, so a `BodyArgs` names at most one source.
#[derive(Args)]
#[group(multiple = false)]
struct BodyArgs {
    /// The Message's body, given as text; without a Body source the Message has none
    #[arg(long)]
    body: Option<String>,
    /// Take the body from this file, which must not be empty
    #[arg(long)]
    body_file: Option<PathBuf>,
    /// Take the body from standard input, which must not be empty
    #[arg(long)]
    stdin: bool,
}

impl From<BodyArgs> for Option<BodySource> {
    fn from(args: BodyArgs) -> Self {
        args.body
            .map(BodySource::Text)
            .or(args.body_file.map(BodySource::File))
            .or(args.stdin.then_some(BodySource::Stdin))
    }
}

#[derive(Subcommand)]
enum Command {
    /// Store a Message and print its id
    Create {
        /// The Sender's name
        #[arg(long, value_parser = str::parse::<Name>)]
        from: Name,
        /// A Receiver's name; repeat for each Receiver
        #[arg(long, required = true, value_parser = str::parse::<Name>)]
        to: Vec<Name>,
        /// The Message's subject
        #[arg(long)]
        subject: String,
        #[command(flatten)]
        body_source: BodyArgs,
        /// Store the Message as a Reply to this Message, named by its Message id or a unique
        /// prefix of it (not a date or time)
        #[arg(long, value_name = "ID")]
        reply_to: Option<IdPrefix>,
    },
    /// List Messages in sequence-number order, oldest first
    ///
    /// Messages always come in sequence-number order, oldest first: the order this Inbox stored
    /// them in, never decided by their creation times. There is no sort flag.
    List {
        /// Only Messages with this name among their Receivers
        #[arg(long, value_parser = str::parse::<Name>)]
        to: Option<Name>,
        /// Only Messages from this Sender
        #[arg(long, value_parser = str::parse::<Name>)]
        from: Option<Name>,
        /// Only Messages stored after this one, named by its Message id or a unique prefix of it
        /// (not a date or time)
        #[arg(long, value_name = "ID")]
        after: Option<IdPrefix>,
        /// Only Messages stored before this one, named by its Message id or a unique prefix of it
        /// (not a date or time)
        #[arg(long, value_name = "ID")]
        before: Option<IdPrefix>,
        /// Only this Message, named by its Message id or a unique prefix of it (not a date or
        /// time), and the chain of Messages it replies to, back to one that is not a Reply
        #[arg(long, value_name = "ID")]
        upthread: Option<IdPrefix>,
        /// Only this Message, named by its Message id or a unique prefix of it (not a date or
        /// time), and every Reply beneath it at any depth
        #[arg(long, value_name = "ID")]
        downthread: Option<IdPrefix>,
        /// Only this Message, named by its Message id or a unique prefix of it (not a date or
        /// time), with everything upthread and downthread of it; Replies on other branches are
        /// left out
        #[arg(long, value_name = "ID")]
        thread: Option<IdPrefix>,
        /// Return at most this many Messages, at least 1: the first n after --after, else the
        /// last n before --before (alone), else the most recent n
        #[arg(long, value_name = "N", default_value_t)]
        limit: Limit,
        /// Print one JSON object: `messages` (each with id, creation time, Sender, Receivers
        /// and subject, as in the text; no body or parent, which `show --json` has),
        /// `left_out` (how many the limit left out, 0 for none) and `next` (the flag and id
        /// that read them, null when none were left out)
        #[arg(long)]
        json: bool,
    },
    /// Print one Message in full
    Show {
        /// The Message's id, or a prefix of it matching only that Message (not a date or time)
        #[arg(value_name = "ID")]
        prefix: IdPrefix,
        /// Print the Message as JSON
        #[arg(long)]
        json: bool,
    },
}

/// Adds to a no-match error from an argument or flag that takes a Message id the hint that it
/// takes one, and where to find one. A date or time of any format reaches this error unless its
/// characters are the start of a stored id (an all-digit value such as 20261126 is a valid hex
/// prefix, and ids are random UUIDs, ADR 0001).
fn hint_id_error(error: anyhow::Error) -> anyhow::Error {
    let Some(NoMessageFor {
        argument,
        no_message,
    }) = error.downcast_ref::<NoMessageFor>()
    else {
        return error;
    };
    let named = match argument {
        IdArgument::After => AFTER_FLAG,
        IdArgument::Before => BEFORE_FLAG,
        IdArgument::Upthread => "--upthread",
        IdArgument::Downthread => "--downthread",
        IdArgument::Thread => "--thread",
        IdArgument::Show => "show's ID argument",
        IdArgument::ReplyTo => "--reply-to",
    };
    anyhow::anyhow!(
        "{no_message}; {named} takes a Message id (or a unique prefix of one), not a date or \
         time; run `snailbox list` to find one"
    )
}

fn main() -> ExitCode {
    match run(Cli::parse().command) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("snailbox: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(command: Command) -> Result<()> {
    match command {
        Command::Create {
            from,
            to,
            subject,
            body_source,
            reply_to,
        } => {
            let body = Option::<BodySource>::from(body_source)
                .map(BodySource::read)
                .transpose()?;
            let mut inbox = Inbox::open(&location::inbox_folder()?)?;
            let id = inbox
                .create(&NewMessage {
                    sender: from,
                    receivers: to,
                    subject,
                    body,
                    reply_to,
                })
                .map_err(hint_id_error)?;
            println!("{id}");
        }
        Command::List {
            to,
            from,
            after,
            before,
            upthread,
            downthread,
            thread,
            limit,
            json,
        } => {
            let inbox = Inbox::open(&location::inbox_folder()?)?;
            let filter = ListFilter {
                to,
                from,
                after,
                before,
                upthread,
                downthread,
                thread,
                limit,
            };
            let listing = inbox.list(&filter).map_err(hint_id_error)?;
            if json {
                println!("{}", listing.to_json());
            } else {
                print!("{}", listing.to_text());
            }
        }
        Command::Show { prefix, json } => {
            let inbox = Inbox::open(&location::inbox_folder()?)?;
            let message = inbox.show(&prefix).map_err(hint_id_error)?;
            if json {
                println!("{}", message.to_json());
            } else {
                print!("{}", message.to_text());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_body_flag_means_no_body_source() {
        let args = BodyArgs {
            body: None,
            body_file: None,
            stdin: false,
        };

        assert!(Option::<BodySource>::from(args).is_none());
    }

    #[test]
    fn the_body_flag_names_a_text_source() {
        let text = BodyArgs {
            body: Some("hi".into()),
            body_file: None,
            stdin: false,
        };

        assert!(matches!(Option::<BodySource>::from(text), Some(BodySource::Text(t)) if t == "hi"));
    }

    #[test]
    fn the_body_file_flag_names_a_file_source() {
        let file = BodyArgs {
            body: None,
            body_file: Some("f.txt".into()),
            stdin: false,
        };

        assert!(
            matches!(Option::<BodySource>::from(file), Some(BodySource::File(p)) if p.ends_with("f.txt"))
        );
    }

    #[test]
    fn the_stdin_flag_names_a_stdin_source() {
        let stdin = BodyArgs {
            body: None,
            body_file: None,
            stdin: true,
        };

        assert!(matches!(
            Option::<BodySource>::from(stdin),
            Some(BodySource::Stdin)
        ));
    }
}
