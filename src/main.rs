mod body;
mod inbox;
mod location;
mod message;

use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::Result;
use clap::{Parser, Subcommand};

use body::BodySource;
use inbox::{Inbox, LeftOut, ListFilter, NoMessage};
use message::{IdPrefix, Message, MessageId, NewMessage};

#[derive(Parser)]
#[command(
    version,
    about = "A local Inbox of Messages between Senders and Receivers"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Store a Message and print its id
    Create {
        /// The Sender's name
        #[arg(long, value_parser = name)]
        from: String,
        /// A Receiver's name; repeat for each Receiver
        #[arg(long, required = true, value_parser = name)]
        to: Vec<String>,
        /// The Message's subject
        #[arg(long)]
        subject: String,
        /// The Message's body, given as text; without a Body source the Message has none
        #[arg(long, conflicts_with_all = ["body_file", "stdin"])]
        body: Option<String>,
        /// Take the body from this file, which must not be empty
        #[arg(long, conflicts_with = "stdin")]
        body_file: Option<PathBuf>,
        /// Take the body from standard input, which must not be empty
        #[arg(long)]
        stdin: bool,
        /// Store the Message as a Reply to this Message, named by its id or a unique prefix
        #[arg(long)]
        reply_to: Option<IdPrefix>,
    },
    /// List Messages in arrival order, oldest first
    ///
    /// Messages always come in arrival order, oldest first: the order this Inbox stored them
    /// in, never decided by their creation times. There is no sort flag.
    List {
        /// Only Messages with this name among their Receivers
        #[arg(long)]
        to: Option<String>,
        /// Only Messages from this Sender
        #[arg(long)]
        from: Option<String>,
        /// Only Messages stored after this one, named by its Message id or a unique prefix of it
        /// (not a date or time)
        #[arg(long, value_name = "ID")]
        after: Option<IdPrefix>,
        /// Only Messages stored before this one, named by its Message id or a unique prefix of it
        /// (not a date or time)
        #[arg(long, value_name = "ID")]
        before: Option<IdPrefix>,
        /// Return at most this many Messages, 0 for no limit: the first n after --after, else
        /// the last n before --before (alone), else the most recent n
        #[arg(long, value_name = "N", default_value_t = 50)]
        limit: usize,
        /// Print the Messages as a JSON array; when the limit left Messages out, an object
        /// holding the array as `messages`, the count as `left_out` and the flag that reads
        /// them as `next`
        #[arg(long)]
        json: bool,
    },
    /// Print one Message in full
    Show {
        /// The Message's id, or a prefix of it matching only that Message
        #[arg(value_name = "ID")]
        prefix: IdPrefix,
        /// Print the Message as JSON
        #[arg(long)]
        json: bool,
    },
}

/// A Sender's or Receiver's name, which must hold something other than whitespace.
fn name(text: &str) -> Result<String, String> {
    if text.trim().is_empty() {
        return Err("the name is blank; give a name".to_owned());
    }
    Ok(text.to_owned())
}

/// Adds to a no-match error from a cursor the hint that the flag takes a Message id. Any
/// cursor value matching no stored id, a date or time of any format included, reaches this
/// error, since ids are random UUIDs (ADR 0001); the flag is the one whose prefix failed, `--after`
/// when both were given the same value.
fn hint_cursor_error(
    error: anyhow::Error,
    after: Option<&IdPrefix>,
    before: Option<&IdPrefix>,
) -> anyhow::Error {
    let Some(no_message) = error.downcast_ref::<NoMessage>() else {
        return error;
    };
    let flag = if after.map(IdPrefix::as_str) == Some(no_message.prefix.as_str()) {
        "--after"
    } else {
        debug_assert_eq!(
            before.map(IdPrefix::as_str),
            Some(no_message.prefix.as_str())
        );
        "--before"
    };
    anyhow::anyhow!(
        "{no_message}; {flag} takes a Message id (or a unique prefix of one), not a date or time"
    )
}

/// The cursor flag that reads the Messages a limit left out, with the direction they lie in
/// and the id the flag takes: `--before` for older ones, `--after` for newer ones.
fn left_out_cursor(left_out: &LeftOut) -> (&'static str, &'static str, &MessageId) {
    match left_out {
        LeftOut::Older { before, .. } => ("older", "--before", before),
        LeftOut::Newer { after, .. } => ("newer", "--after", after),
    }
}

/// The flag and id that read the Messages a limit left out.
fn next_flag(left_out: &LeftOut) -> String {
    let (_, flag, id) = left_out_cursor(left_out);
    format!("{flag} {id}")
}

/// The line text output ends with when a limit left Messages out.
fn left_out_notice(left_out: &LeftOut) -> String {
    let (direction, cursor, _) = left_out_cursor(left_out);
    let count = left_out.count();
    let noun = if count == 1 { "Message" } else { "Messages" };
    format!(
        "{count} {direction} {noun} left out by the limit; read them with the same flags but {} in place of any {cursor} (or --limit 0 for no limit)",
        next_flag(left_out)
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
            body,
            body_file,
            stdin,
            reply_to,
        } => {
            let body = BodySource::from_flags(body, body_file, stdin)
                .map(BodySource::read)
                .transpose()?;
            let mut inbox = Inbox::open(&location::inbox_folder()?)?;
            let id = inbox.create(&NewMessage {
                sender: from,
                receivers: to,
                subject,
                body,
                reply_to,
            })?;
            println!("{id}");
        }
        Command::List {
            to,
            from,
            after,
            before,
            limit,
            json,
        } => {
            let inbox = Inbox::open(&location::inbox_folder()?)?;
            let filter = ListFilter {
                to,
                from,
                after,
                before,
                limit: NonZeroUsize::new(limit),
            };
            let listing = inbox.list(&filter).map_err(|error| {
                hint_cursor_error(error, filter.after.as_ref(), filter.before.as_ref())
            })?;
            if json {
                let messages: Vec<_> = listing
                    .messages
                    .iter()
                    .map(Message::to_json_value)
                    .collect();
                let messages = serde_json::Value::Array(messages);
                match &listing.left_out {
                    None => println!("{messages}"),
                    Some(left_out) => println!(
                        "{}",
                        serde_json::json!({
                            "messages": messages,
                            "left_out": left_out.count(),
                            "next": next_flag(left_out),
                        })
                    ),
                }
            } else {
                for message in &listing.messages {
                    println!("{}", message.to_list_line());
                }
                if let Some(left_out) = &listing.left_out {
                    println!("{}", left_out_notice(left_out));
                }
            }
        }
        Command::Show { prefix, json } => {
            let inbox = Inbox::open(&location::inbox_folder()?)?;
            let message = inbox.show(&prefix)?;
            if json {
                println!("{}", message.to_json());
            } else {
                print!("{}", message.to_text());
            }
        }
    }
    Ok(())
}
