mod body;
mod inbox;
mod location;
mod message;

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::Result;
use clap::{Parser, Subcommand};

use body::BodySource;
use inbox::{Inbox, ListFilter};
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
        /// Return at most this many Messages, 0 for no limit: the first after --after, else the
        /// most recent
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

/// The Message id prefix a cursor flag was given; a value shaped like a date or time (a year
/// and month, or a colon) can never start a UUID, so it is refused with a hint (ADR 0001).
fn cursor(flag: &str, text: &str) -> Result<IdPrefix, String> {
    let bytes = text.as_bytes();
    let starts_like_a_date = bytes.len() >= 7
        && bytes[..4].iter().all(u8::is_ascii_digit)
        && bytes[4] == b'-'
        && bytes[5..7].iter().all(u8::is_ascii_digit);
    if starts_like_a_date || text.contains(':') {
        return Err(format!(
            "{flag} takes a Message id (or a unique prefix of one), not a date or time"
        ));
    }
    text.parse()
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
                    println!("{}", left_out.to_notice());
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
