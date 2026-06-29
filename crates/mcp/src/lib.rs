use serde_json::{Value, json};
use smails_core::{Attachment, format_bytes};
use smails_native::{
    CreateResult, api_from_config, create_mailbox, load_config, resolve_message_id,
};
use std::io::{self, BufRead, BufReader, Write};

pub fn run_stdio() -> Result<(), String> {
    let stdin = io::stdin();
    let mut reader = BufReader::new(stdin.lock());
    let stdout = io::stdout();
    let mut writer = stdout.lock();

    while let Some(request) = read_message(&mut reader)? {
        if let Some(response) = handle_request(request) {
            write_message(&mut writer, &response)?;
        }
    }
    Ok(())
}

fn handle_request(request: Value) -> Option<Value> {
    let id = request.get("id").cloned();
    let method = request.get("method").and_then(Value::as_str).unwrap_or("");
    let params = request.get("params").cloned().unwrap_or_else(|| json!({}));

    let result = match method {
        "initialize" => Ok(json!({
            "protocolVersion": "2024-11-05",
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "smails", "version": env!("CARGO_PKG_VERSION") }
        })),
        "notifications/initialized" => return None,
        "tools/list" => Ok(json!({ "tools": tools() })),
        "tools/call" => call_tool(params),
        _ => Err(format!("Unknown method: {method}")),
    };

    id.map(|id| match result {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err(message) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": -32000, "message": message }
        }),
    })
}

fn tools() -> Value {
    json!([
        {
            "name": "create_mailbox",
            "description": "Create a new temporary email mailbox. Pass force=true to replace an existing mailbox.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "domain": { "type": "string" },
                    "force": { "type": "boolean" }
                }
            }
        },
        {
            "name": "list_messages",
            "description": "List messages in the current mailbox",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "read_message",
            "description": "Read a specific message",
            "inputSchema": {
                "type": "object",
                "properties": { "id": { "type": "string" } },
                "required": ["id"]
            }
        },
        {
            "name": "delete_message",
            "description": "Delete a specific message",
            "inputSchema": {
                "type": "object",
                "properties": { "id": { "type": "string" } },
                "required": ["id"]
            }
        },
        {
            "name": "get_address",
            "description": "Get the current mailbox address",
            "inputSchema": { "type": "object", "properties": {} }
        }
    ])
}

fn call_tool(params: Value) -> Result<Value, String> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| "tools/call missing name".to_owned())?;
    let args = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));

    match name {
        "create_mailbox" => {
            let domain = args
                .get("domain")
                .and_then(Value::as_str)
                .map(str::to_owned);
            let force = args.get("force").and_then(Value::as_bool).unwrap_or(false);
            match create_mailbox(domain, force)? {
                CreateResult::Created { address } => content(format!("Mailbox created: {address}")),
                CreateResult::Existing { address } => content(format!(
                    "A mailbox already exists: {address}. Pass force=true to replace it."
                )),
            }
        }
        "list_messages" => {
            let messages = api_from_config()?.list_messages()?;
            if messages.is_empty() {
                return content("Inbox is empty.");
            }
            let text = messages
                .into_iter()
                .map(|message| {
                    let read = if message.read == 0 { "unread" } else { "read" };
                    format!(
                        "[{read}] {} | From: {} | Subject: {} | Preview: {}",
                        message.id, message.from_addr, message.subject, message.preview
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            content(text)
        }
        "read_message" => {
            let id = args
                .get("id")
                .and_then(Value::as_str)
                .ok_or_else(|| "read_message requires id".to_owned())?;
            let api = api_from_config()?;
            let id = resolve_message_id(&api, id)?;
            let message = api.get_message(&id)?;
            let attachments = attachment_lines(&message.attachments);
            content(format!(
                "From: {} <{}>\nSubject: {}\nDate: {}\n---\n{}{}",
                message.from_name,
                message.from_addr,
                message.subject,
                message.received_at,
                message
                    .text
                    .as_deref()
                    .or(message.html.as_deref())
                    .unwrap_or("(empty)"),
                attachments
            ))
        }
        "delete_message" => {
            let id = args
                .get("id")
                .and_then(Value::as_str)
                .ok_or_else(|| "delete_message requires id".to_owned())?;
            let api = api_from_config()?;
            let id = resolve_message_id(&api, id)?;
            api.delete_message(&id)?;
            content(format!("Message {id} deleted."))
        }
        "get_address" => match load_config()? {
            Some(config) => content(config.address),
            None => content("No mailbox configured. Use create_mailbox first."),
        },
        _ => Err(format!("Unknown tool: {name}")),
    }
}

fn content(text: impl Into<String>) -> Result<Value, String> {
    Ok(json!({ "content": [{ "type": "text", "text": text.into() }] }))
}

fn attachment_lines(attachments: &[Attachment]) -> String {
    if attachments.is_empty() {
        return String::new();
    }
    let lines = attachments
        .iter()
        .map(|attachment| {
            format!(
                "[{}] {} | {} | {}",
                attachment.index,
                attachment.filename.as_deref().unwrap_or("attachment"),
                attachment.content_type,
                format_bytes(attachment.size)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!("\n---\nAttachments:\n{lines}")
}

fn read_message(reader: &mut impl BufRead) -> Result<Option<Value>, String> {
    let mut content_length = None;
    let mut line = String::new();

    loop {
        line.clear();
        let n = reader.read_line(&mut line).map_err(|err| err.to_string())?;
        if n == 0 {
            return Ok(None);
        }
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break;
        }
        if let Some(value) = trimmed.strip_prefix("Content-Length:") {
            content_length = Some(
                value
                    .trim()
                    .parse::<usize>()
                    .map_err(|_| "Invalid Content-Length".to_owned())?,
            );
        }
    }

    let len = content_length.ok_or_else(|| "Missing Content-Length".to_owned())?;
    let mut body = vec![0; len];
    reader
        .read_exact(&mut body)
        .map_err(|err| err.to_string())?;
    serde_json::from_slice(&body).map_err(|err| format!("Invalid JSON-RPC body: {err}"))
}

fn write_message(writer: &mut impl Write, value: &Value) -> Result<(), String> {
    let body = value.to_string();
    write!(writer, "Content-Length: {}\r\n\r\n{}", body.len(), body)
        .map_err(|err| err.to_string())?;
    writer.flush().map_err(|err| err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initialize_returns_server_info() {
        let response = handle_request(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize"
        }))
        .unwrap();
        assert_eq!(response["id"], 1);
        assert_eq!(response["result"]["serverInfo"]["name"], "smails");
    }

    #[test]
    fn tools_list_includes_mailbox_tools() {
        let response = handle_request(json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list"
        }))
        .unwrap();
        let tools = response["result"]["tools"].as_array().unwrap();
        assert!(tools.iter().any(|tool| tool["name"] == "create_mailbox"));
        assert!(tools.iter().any(|tool| tool["name"] == "read_message"));
    }
}
