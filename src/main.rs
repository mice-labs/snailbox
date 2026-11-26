mod inbox;
mod location;
mod message;

use std::process::ExitCode;

use anyhow::Result;
use clap::{Parser, Subcommand};

use inbox::{Inbox, ListFilter};
use message::{Message, NewMessage};

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
        #[arg(long)]
        from: String,
        /// A Receiver's name; repeat for each Receiver
        #[arg(long, required = true)]
        to: Vec<String>,
        /// The Message's subject
        #[arg(long)]
        subject: String,
        /// The Message's body; without it the Message has none
        #[arg(long)]
        body: Option<String>,
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
        /// Print the Messages as a JSON array
        #[arg(long)]
        json: bool,
    },
    /// Print one Message in full
    Show {
        /// The Message's full id
        id: String,
        /// Print the Message as JSON
        #[arg(long)]
        json: bool,
    },
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
        } => {
            let mut inbox = Inbox::open(&location::inbox_folder()?)?;
            let id = inbox.create(&NewMessage {
                sender: from,
                receivers: to,
                subject,
                body,
            })?;
            println!("{id}");
        }
        Command::List { to, from, json } => {
            let inbox = Inbox::open(&location::inbox_folder()?)?;
            let messages = inbox.list(&ListFilter { to, from })?;
            if json {
                let messages: Vec<_> = messages.iter().map(Message::to_json_value).collect();
                println!("{}", serde_json::Value::Array(messages));
            } else {
                for message in &messages {
                    println!("{}", message.to_list_line());
                }
            }
        }
        Command::Show { id, json } => {
            let inbox = Inbox::open(&location::inbox_folder()?)?;
            let message = inbox.show(&id)?;
            if json {
                println!("{}", message.to_json());
            } else {
                print!("{}", message.to_text());
            }
        }
    }
    Ok(())
}
