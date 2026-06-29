use serde::de::DeserializeOwned;
use serde_json::json;
use smails_core::{
    CreateMailboxRequest, MailboxCreated, MessageDetail, MessageSummary, OkJson, PATH_MAILBOX,
    PATH_MESSAGES, authorization_header, message_path,
};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{Request, RequestInit, Response, window};

const VERSION_HEADER: &str = "X-Smails-Version";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiError {
    pub status: Option<u16>,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ApiResponse<T> {
    pub data: T,
    pub version: Option<String>,
}

pub async fn create_mailbox() -> Result<ApiResponse<MailboxCreated>, ApiError> {
    let body = serde_json::to_string(&CreateMailboxRequest {
        domain: None,
        address: None,
        token: None,
    })
    .map_err(error_message)?;
    request_json("POST", PATH_MAILBOX, None, Some(body)).await
}

pub async fn list_messages(token: &str) -> Result<ApiResponse<Vec<MessageSummary>>, ApiError> {
    request_json("GET", PATH_MESSAGES, Some(token), None).await
}

pub async fn get_message(token: &str, id: &str) -> Result<ApiResponse<MessageDetail>, ApiError> {
    request_json("GET", &message_path(id), Some(token), None).await
}

pub async fn delete_message(token: &str, id: &str) -> Result<ApiResponse<OkJson>, ApiError> {
    request_json("DELETE", &message_path(id), Some(token), None).await
}

async fn request_json<T: DeserializeOwned>(
    method: &str,
    path: &str,
    token: Option<&str>,
    body: Option<String>,
) -> Result<ApiResponse<T>, ApiError> {
    let init = RequestInit::new();
    init.set_method(method);
    if let Some(body) = body {
        init.set_body(&JsValue::from_str(&body));
    }

    let request = Request::new_with_str_and_init(path, &init).map_err(error_js)?;
    if method == "POST" {
        request
            .headers()
            .set("Content-Type", "application/json")
            .map_err(error_js)?;
    }
    if let Some(token) = token {
        request
            .headers()
            .set("Authorization", &authorization_header(token))
            .map_err(error_js)?;
    }

    let response = JsFuture::from(
        window()
            .ok_or_else(|| error_message("window unavailable"))?
            .fetch_with_request(&request),
    )
    .await
    .map_err(error_js)?
    .dyn_into::<Response>()
    .map_err(error_js)?;

    let version = response.headers().get(VERSION_HEADER).ok().flatten();
    if !response.ok() {
        return Err(http_error(response).await);
    }

    let json = JsFuture::from(response.json().map_err(error_js)?)
        .await
        .map_err(error_js)?;
    let data = serde_wasm_bindgen::from_value(json).map_err(error_message)?;
    Ok(ApiResponse { data, version })
}

async fn http_error(response: Response) -> ApiError {
    let status = response.status();
    let fallback = format!("HTTP {status}");
    let text = match response.text() {
        Ok(promise) => JsFuture::from(promise)
            .await
            .ok()
            .and_then(|value| value.as_string())
            .unwrap_or_default(),
        Err(_) => String::new(),
    };
    let message = serde_json::from_str::<serde_json::Value>(&text)
        .ok()
        .and_then(|value| {
            value
                .get("error")
                .and_then(|error| error.as_str())
                .map(str::to_owned)
        })
        .filter(|error| !error.is_empty())
        .unwrap_or(fallback);

    ApiError {
        status: Some(status),
        message,
    }
}

fn error_js(value: JsValue) -> ApiError {
    error_message(value.as_string().unwrap_or_else(|| format!("{value:?}")))
}

fn error_message(message: impl ToString) -> ApiError {
    ApiError {
        status: None,
        message: message.to_string(),
    }
}

pub fn new_message_event(value: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(value)
        .ok()
        .and_then(|value| (value.get("type") == Some(&json!("new_message"))).then_some(()))
        .is_some()
}
