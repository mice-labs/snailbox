use clap::{Parser, Subcommand};

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
    Create,
    /// List Messages in arrival order
    List,
    /// Print one Message in full
    Show,
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Command::Create | Command::List | Command::Show => {
            eprintln!("snailbox: not implemented yet");
            std::process::exit(1);
        }
    }
}
