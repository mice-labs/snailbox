mod inbox;
mod location;
mod message;

use std::process::ExitCode;

use anyhow::Result;
use clap::{Parser, Subcommand};

use inbox::Inbox;
use message::{IdPrefix, NewMessage};

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
        /// Store the Message as a Reply to this Message, named by its id or a unique prefix
        #[arg(long)]
        reply_to: Option<IdPrefix>,
    },
    /// List Messages in arrival order
    List,
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
            reply_to,
        } => {
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
        Command::List => anyhow::bail!("not implemented yet"),
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
