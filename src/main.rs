mod body;
mod inbox;
mod location;
mod message;

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::Result;
use clap::{Parser, Subcommand};

use body::BodySource;
use inbox::Inbox;
use message::NewMessage;

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
        /// The Message's body, given as text; without a body source the Message has none
        #[arg(long, conflicts_with_all = ["body_file", "stdin"])]
        body: Option<String>,
        /// Take the body from this file, which must not be empty
        #[arg(long, conflicts_with = "stdin")]
        body_file: Option<PathBuf>,
        /// Take the body from standard input, which must not be empty
        #[arg(long)]
        stdin: bool,
    },
    /// List Messages in arrival order
    List,
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
            body_file,
            stdin,
        } => {
            let source = match (body, body_file, stdin) {
                (Some(text), _, _) => Some(BodySource::Text(text)),
                (None, Some(path), _) => Some(BodySource::File(path)),
                (None, None, true) => Some(BodySource::Stdin),
                (None, None, false) => None,
            };
            let body = source.map(BodySource::read).transpose()?;
            let mut inbox = Inbox::open(&location::inbox_folder()?)?;
            let id = inbox.create(&NewMessage {
                sender: from,
                receivers: to,
                subject,
                body,
            })?;
            println!("{id}");
        }
        Command::List => anyhow::bail!("not implemented yet"),
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
