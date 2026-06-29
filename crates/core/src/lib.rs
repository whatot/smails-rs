use serde::{Deserialize, Serialize};

pub const DEFAULT_BASE_URL: &str = "https://smails.dev";
pub const DEFAULT_DOMAIN: &str = "smails.dev";
pub const CONFIG_FILE: &str = ".smails";

pub const PATH_DOMAINS: &str = "/api/domains";
pub const PATH_MAILBOX: &str = "/api/mailbox";
pub const PATH_MESSAGES: &str = "/api/mailbox/messages";
pub const PATH_CONNECT: &str = "/api/mailbox/connect";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CreateMailboxRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MailboxCreated {
    pub address: String,
    pub token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MessageSummary {
    pub id: String,
    pub from_addr: String,
    pub from_name: String,
    pub subject: String,
    pub preview: String,
    pub received_at: i64,
    pub read: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MessageDetail {
    pub id: String,
    pub from_addr: String,
    pub from_name: String,
    pub subject: String,
    pub received_at: i64,
    pub read: i64,
    pub html: Option<String>,
    pub text: Option<String>,
    pub attachments: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeliverMessage {
    pub from_addr: String,
    pub from_name: String,
    pub subject: String,
    pub preview: String,
    pub body: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OkJson {
    pub ok: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CapabilityJson {
    pub fetch: bool,
    pub durable_object: bool,
    pub sqlite_storage: bool,
    pub websocket: bool,
    pub alarm: bool,
    pub email_export: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedToken {
    pub address: String,
    pub secret: String,
}

pub fn authorization_header(token: &str) -> String {
    format!("Bearer {token}")
}

pub fn parse_auth_header(header: Option<&str>) -> Option<String> {
    header?.strip_prefix("Bearer ").map(str::to_owned)
}

pub fn parse_mailbox_token(token: &str) -> Option<ParsedToken> {
    let (address, secret) = token.split_once('.')?;
    if !is_mailbox_name(address) || secret.is_empty() {
        return None;
    }
    Some(ParsedToken {
        address: address.to_owned(),
        secret: secret.to_owned(),
    })
}

pub fn mailbox_name_from_address(address: &str) -> &str {
    address.split('@').next().unwrap_or(address)
}

pub fn mailbox_name_from_token(token: &str) -> Option<String> {
    parse_mailbox_token(token).map(|parsed| parsed.address)
}

pub fn is_mailbox_name(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

pub fn message_path(id: &str) -> String {
    format!("{PATH_MESSAGES}/{id}")
}

pub fn short_id(id: &str) -> &str {
    id.get(..8).unwrap_or(id)
}

pub fn preview_text(text: &str) -> String {
    text.chars()
        .take(200)
        .map(|c| if c == '\n' { ' ' } else { c })
        .collect()
}

pub fn initials(name: &str) -> String {
    name.trim()
        .chars()
        .next()
        .map(|c| c.to_uppercase().collect())
        .unwrap_or_else(|| "?".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_mailbox_token() {
        let parsed = parse_mailbox_token("demo.secret").unwrap();
        assert_eq!(parsed.address, "demo");
        assert_eq!(parsed.secret, "secret");
        assert_eq!(
            mailbox_name_from_token("demo.secret").as_deref(),
            Some("demo")
        );
    }

    #[test]
    fn rejects_bad_mailbox_names() {
        assert!(parse_mailbox_token("demo@example.secret").is_none());
        assert!(parse_mailbox_token("Demo.secret").is_none());
        assert!(parse_mailbox_token("demo.").is_none());
    }
}
