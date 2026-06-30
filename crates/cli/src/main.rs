use clap::{ArgAction, Parser, Subcommand};
use smails_core::short_id;
use smails_native::{
    CreateResult, api_from_config, create_mailbox, require_config, resolve_message_id,
};
use std::process;

#[derive(Parser)]
#[command(
    name = "smails",
    version,
    disable_version_flag = true,
    about = "Disposable email for humans and agents",
    arg_required_else_help = true,
    after_help = "Env:\n  SMAILS_API_URL  Override the API base URL (default https://smails.dev)\n  SMAILS_CONFIG   Override the config path (default ~/.smails)"
)]
struct Cli {
    #[arg(short = 'v', long = "version", action = ArgAction::Version, help = "Print version")]
    _version: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Create a new mailbox.
    Create {
        /// Domain to use for the mailbox.
        #[arg(long)]
        domain: Option<String>,
        /// Replace the current mailbox with a fresh one.
        #[arg(long)]
        force: bool,
    },
    /// List messages.
    Inbox,
    /// Read a message by full id or short prefix.
    Read { id: String },
    /// Delete a message by full id or short prefix.
    #[command(alias = "rm")]
    Delete { id: String },
    /// Show the current mailbox address.
    Whoami,
    /// Start the MCP server over stdio.
    Mcp,
}

fn main() {
    if let Err(err) = run(Cli::parse()) {
        eprintln!("{err}");
        process::exit(1);
    }
}

fn run(cli: Cli) -> Result<(), String> {
    match cli.command {
        Command::Create { domain, force } => create(domain, force),
        Command::Inbox => inbox(),
        Command::Read { id } => read(&id),
        Command::Delete { id } => delete(&id),
        Command::Whoami => whoami(),
        Command::Mcp => smails_mcp::run_stdio(),
    }
}

fn create(domain: Option<String>, force: bool) -> Result<(), String> {
    match create_mailbox(domain, force)? {
        CreateResult::Created { address } => println!("Mailbox created: {address}"),
        CreateResult::Existing { address } => {
            println!("You already have a mailbox: {address}");
            println!("Use `smails create --force` to create a new one.");
        }
    }
    Ok(())
}

fn inbox() -> Result<(), String> {
    let api = api_from_config()?;
    let messages = api.list_messages()?;
    if messages.is_empty() {
        println!("Inbox is empty.");
        return Ok(());
    }
    for message in messages {
        let mark = if message.read == 0 { "*" } else { " " };
        println!(
            "{} {:8}  {:28}  {:48}  {}",
            mark,
            short_id(&message.id),
            truncate(&message.from_addr, 28),
            truncate(&message.subject, 48),
            message.received_at
        );
    }
    Ok(())
}

fn read(id: &str) -> Result<(), String> {
    let api = api_from_config()?;
    let id = resolve_message_id(&api, id)?;
    let message = api.get_message(&id)?;
    println!("From:    {} <{}>", message.from_name, message.from_addr);
    println!("Subject: {}", message.subject);
    println!("Date:    {}", message.received_at);
    println!("---");
    println!(
        "{}",
        message
            .text
            .as_deref()
            .or(message.html.as_deref())
            .unwrap_or("(empty)")
    );
    Ok(())
}

fn delete(id: &str) -> Result<(), String> {
    let api = api_from_config()?;
    let id = resolve_message_id(&api, id)?;
    api.delete_message(&id)?;
    println!("Deleted {}.", short_id(&id));
    Ok(())
}

fn whoami() -> Result<(), String> {
    println!("{}", require_config()?.address);
    Ok(())
}

fn truncate(value: &str, width: usize) -> String {
    let truncated: String = value.chars().take(width).collect();
    format!("{truncated:width$}")
}
