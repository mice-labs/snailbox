mod body;
mod inbox;
mod location;
mod message;

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::Result;
use clap::{Parser, Subcommand};

use body::BodySource;
use inbox::{Inbox, LeftOut, ListFilter};
use message::{IdPrefix, Message, NewMessage};

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
        #[arg(long, value_name = "ID", value_parser = cursor_after)]
        after: Option<IdPrefix>,
        /// Only Messages stored before this one, named by its Message id or a unique prefix of it
        /// (not a date or time)
        #[arg(long, value_name = "ID", value_parser = cursor_before)]
        before: Option<IdPrefix>,
        /// Return at most this many Messages, 0 for no limit: the first n after --after, else
        /// the last n before --before, else the most recent n
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

fn cursor_after(text: &str) -> Result<IdPrefix, String> {
    cursor("--after", text)
}

fn cursor_before(text: &str) -> Result<IdPrefix, String> {
    cursor("--before", text)
}

/// The Message id prefix a cursor flag was given; a value that cannot start a UUID (hex digits
/// with a hyphen after the 8th, 13th, 18th and 23rd) is refused with a hint, which catches every
/// date or time format, not only ISO ones (ADR 0001).
fn cursor(flag: &str, text: &str) -> Result<IdPrefix, String> {
    let can_start_a_uuid = text.len() <= 36
        && text.bytes().enumerate().all(|(position, byte)| {
            if matches!(position, 8 | 13 | 18 | 23) {
                byte == b'-'
            } else {
                byte.is_ascii_hexdigit()
            }
        });
    if !can_start_a_uuid {
        return Err(format!(
            "{flag} takes a Message id (or a unique prefix of one), not a date or time"
        ));
    }
    text.parse()
}

/// The line text output ends with when a limit left Messages out.
fn left_out_notice(left_out: &LeftOut) -> String {
    let direction = match left_out {
        LeftOut::Older { .. } => "older",
        LeftOut::Newer { .. } => "newer",
    };
    let count = left_out.count();
    let noun = if count == 1 { "Message" } else { "Messages" };
    let cursor = left_out.cursor_flag();
    format!(
        "{count} {direction} {noun} left out by the limit; read them with the same flags but {} in place of any {cursor} (or --limit 0 for no limit)",
        left_out.next_flag()
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
            let listing = inbox.list(&ListFilter {
                to,
                from,
                after,
                before,
                limit: (limit > 0).then_some(limit),
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
                            "next": left_out.next_flag(),
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
