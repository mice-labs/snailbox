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

/// A Sender's or Receiver's name, which must hold something other than whitespace.
fn name(text: &str) -> Result<String, String> {
    if text.trim().is_empty() {
        return Err("the name is blank; give a name".to_owned());
    }
    Ok(text.to_owned())
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
            let body = BodySource::from_flags(body, body_file, stdin)
                .map(BodySource::read)
                .transpose()?;
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
