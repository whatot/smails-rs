use serde::{Deserialize, Serialize, de::DeserializeOwned};
use smails_core::{
    CONFIG_FILE, CreateMailboxRequest, DEFAULT_BASE_URL, MailboxCreated, MessageDetail,
    MessageSummary, OkJson, PATH_DOMAINS, PATH_MAILBOX, PATH_MESSAGES, authorization_header,
    message_path,
};
use std::{
    env, fs,
    path::{Path, PathBuf},
    time::Duration,
};

pub type Result<T> = std::result::Result<T, String>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Config {
    pub address: String,
    pub token: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreateResult {
    Created { address: String },
    Existing { address: String },
}

#[derive(Clone)]
pub struct ApiClient {
    agent: ureq::Agent,
    base_url: String,
    token: Option<String>,
}

impl ApiClient {
    pub fn anonymous() -> Self {
        Self::new(None)
    }

    pub fn with_token(token: impl Into<String>) -> Self {
        Self::new(Some(token.into()))
    }

    fn new(token: Option<String>) -> Self {
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(15)))
            .http_status_as_error(false)
            .build()
            .new_agent();
        let base_url = env::var("SMAILS_API_URL").unwrap_or_else(|_| DEFAULT_BASE_URL.to_owned());
        Self {
            agent,
            base_url: base_url.trim_end_matches('/').to_owned(),
            token,
        }
    }

    pub fn get_domains(&self) -> Result<Vec<String>> {
        self.request_json(Method::Get, PATH_DOMAINS, None)
    }

    pub fn create_mailbox(&self, domain: Option<String>) -> Result<MailboxCreated> {
        let body = serde_json::to_value(CreateMailboxRequest {
            domain,
            address: None,
            token: None,
        })
        .map_err(|err| err.to_string())?;
        self.request_json(Method::Post, PATH_MAILBOX, Some(body))
    }

    pub fn list_messages(&self) -> Result<Vec<MessageSummary>> {
        self.request_json(Method::Get, PATH_MESSAGES, None)
    }

    pub fn get_message(&self, id: &str) -> Result<MessageDetail> {
        self.request_json(Method::Get, &message_path(id), None)
    }

    pub fn delete_message(&self, id: &str) -> Result<()> {
        let _: OkJson = self.request_json(Method::Delete, &message_path(id), None)?;
        Ok(())
    }

    fn request_json<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        let response = match method {
            Method::Get => {
                let mut request = self.agent.get(&url);
                if let Some(token) = &self.token {
                    request = request.header("Authorization", &authorization_header(token));
                }
                request.call()
            }
            Method::Post => {
                let mut request = self
                    .agent
                    .post(&url)
                    .header("Content-Type", "application/json");
                if let Some(token) = &self.token {
                    request = request.header("Authorization", &authorization_header(token));
                }
                request.send_json(body.unwrap_or_else(|| serde_json::json!({})))
            }
            Method::Delete => {
                let mut request = self.agent.delete(&url);
                if let Some(token) = &self.token {
                    request = request.header("Authorization", &authorization_header(token));
                }
                request.force_send_body().send_empty()
            }
        };
        let mut response = response.map_err(|err| format!("Network error: {err}"))?;
        if !response.status().is_success() {
            return Err(error_message(&mut response));
        }
        response
            .body_mut()
            .read_json()
            .map_err(|err| format!("Invalid JSON response: {err}"))
    }
}

fn error_message(response: &mut ureq::http::Response<ureq::Body>) -> String {
    let status = response.status().as_u16();
    let text = response.body_mut().read_to_string().unwrap_or_default();
    serde_json::from_str::<serde_json::Value>(&text)
        .ok()
        .and_then(|value| {
            value
                .get("error")
                .and_then(|error| error.as_str())
                .map(str::to_owned)
        })
        .filter(|error| !error.is_empty())
        .unwrap_or_else(|| format!("HTTP {status}"))
}

#[derive(Debug, Clone, Copy)]
enum Method {
    Get,
    Post,
    Delete,
}

pub fn config_path() -> Result<PathBuf> {
    if let Some(path) = env::var_os("SMAILS_CONFIG") {
        return Ok(PathBuf::from(path));
    }
    let home = env::var_os("HOME").ok_or_else(|| "HOME is not set".to_owned())?;
    Ok(PathBuf::from(home).join(CONFIG_FILE))
}

pub fn load_config() -> Result<Option<Config>> {
    let path = config_path()?;
    let data = match fs::read_to_string(&path) {
        Ok(data) => data,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(format!("Cannot read {}: {err}", path.display())),
    };
    serde_json::from_str(&data).map(Some).map_err(|_| {
        format!(
            "Config file {} is corrupt. Remove it or run `smails create --force`.",
            path.display()
        )
    })
}

pub fn save_config(config: &Config) -> Result<()> {
    let path = config_path()?;
    save_config_to_path(&path, config)
}

fn save_config_to_path(path: &Path, config: &Config) -> Result<()> {
    let data = serde_json::to_string_pretty(config).map_err(|err| err.to_string())?;
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)
            .map_err(|err| format!("Cannot create {}: {err}", parent.display()))?;
    }
    write_private_file(path, format!("{data}\n").as_bytes())
        .map_err(|err| format!("Cannot write {}: {err}", path.display()))
}

#[cfg(unix)]
fn write_private_file(path: &Path, data: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(data)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
fn write_private_file(path: &Path, data: &[u8]) -> std::io::Result<()> {
    fs::write(path, data)
}

pub fn require_config() -> Result<Config> {
    load_config()?.ok_or_else(|| "No mailbox found. Run `smails create` first.".to_owned())
}

pub fn create_mailbox(domain: Option<String>, force: bool) -> Result<CreateResult> {
    if !force && let Some(existing) = load_config()? {
        return Ok(CreateResult::Existing {
            address: existing.address,
        });
    }
    let created = ApiClient::anonymous().create_mailbox(domain)?;
    save_config(&Config {
        address: created.address.clone(),
        token: created.token,
    })?;
    Ok(CreateResult::Created {
        address: created.address,
    })
}

pub fn api_from_config() -> Result<ApiClient> {
    let config = require_config()?;
    Ok(ApiClient::with_token(config.token))
}

pub fn resolve_message_id(api: &ApiClient, id_or_prefix: &str) -> Result<String> {
    if is_full_id(id_or_prefix) {
        return Ok(id_or_prefix.to_owned());
    }
    let matches: Vec<_> = api
        .list_messages()?
        .into_iter()
        .filter(|message| message.id.starts_with(id_or_prefix))
        .collect();
    match matches.as_slice() {
        [] => Err(format!(
            "No message found starting with \"{id_or_prefix}\"."
        )),
        [message] => Ok(message.id.clone()),
        _ => Err(format!(
            "\"{id_or_prefix}\" is ambiguous ({} matches). Use more characters.",
            matches.len()
        )),
    }
}

fn is_full_id(value: &str) -> bool {
    let parts: Vec<_> = value.split('-').collect();
    let lens = [8, 4, 4, 4, 12];
    parts.len() == lens.len()
        && parts
            .iter()
            .zip(lens)
            .all(|(part, len)| part.len() == len && part.bytes().all(|b| b.is_ascii_hexdigit()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn saves_config_privately() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = env::temp_dir().join(format!(
            "smails-native-test-{}-{unique}",
            std::process::id()
        ));
        let path = dir.join("config.json");
        let config = Config {
            address: "demo@smails.dev".to_owned(),
            token: "demo.0123456789abcdef0123456789abcdef".to_owned(),
        };

        save_config_to_path(&path, &config).unwrap();
        let saved = fs::read_to_string(&path).unwrap();
        assert!(saved.contains("demo@smails.dev"));

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }

        let _ = fs::remove_file(&path);
        let _ = fs::remove_dir(&dir);
    }
}
