use clap::{ArgAction, Parser, Subcommand};
use smails_core::{attachment_filename, format_bytes, short_id};
use smails_native::{
    CreateResult, api_from_config, create_mailbox, require_config, resolve_message_id,
};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process,
};

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
    /// Download an attachment by message id and attachment index.
    Download {
        id: String,
        index: usize,
        output: Option<PathBuf>,
    },
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
        Command::Download { id, index, output } => download(&id, index, output),
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
    if !message.attachments.is_empty() {
        println!("---");
        println!("Attachments:");
        for attachment in &message.attachments {
            println!(
                "  [{}] {}  {}  {}",
                attachment.index,
                attachment.filename.as_deref().unwrap_or("attachment"),
                attachment.content_type,
                format_bytes(attachment.size)
            );
        }
    }
    Ok(())
}

fn delete(id: &str) -> Result<(), String> {
    let api = api_from_config()?;
    let id = resolve_message_id(&api, id)?;
    api.delete_message(&id)?;
    println!("Deleted {}.", short_id(&id));
    Ok(())
}

fn download(id: &str, index: usize, output: Option<PathBuf>) -> Result<(), String> {
    let api = api_from_config()?;
    let id = resolve_message_id(&api, id)?;
    let message = api.get_message(&id)?;
    let attachment = message
        .attachments
        .iter()
        .find(|attachment| attachment.index == index)
        .ok_or_else(|| format!("No attachment {index} on message {}.", short_id(&id)))?;
    let path = output.unwrap_or_else(|| PathBuf::from(attachment_filename(attachment)));
    let bytes = api.download_attachment(&id, index)?;
    write_new_file(&path, &bytes)?;
    println!("Saved {} ({}).", path.display(), format_bytes(bytes.len()));
    Ok(())
}

fn whoami() -> Result<(), String> {
    println!("{}", require_config()?.address);
    Ok(())
}

fn write_new_file(path: &Path, data: &[u8]) -> Result<(), String> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|err| format!("Cannot create {}: {err}", path.display()))?;
    file.write_all(data)
        .map_err(|err| format!("Cannot write {}: {err}", path.display()))
}

fn truncate(value: &str, width: usize) -> String {
    let truncated: String = value.chars().take(width).collect();
    format!("{truncated:width$}")
}
