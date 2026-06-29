use serde::{Deserialize, Serialize};
use smails_core::DEFAULT_DOMAIN;
use wasm_bindgen::prelude::*;
use worker::{Env, Request, Response, Result};

pub(crate) const MAILBOX_BINDING: &str = "MAILBOX";
pub(crate) const EXPIRY_MS: i64 = 7 * 24 * 60 * 60 * 1000;
pub(crate) const ONE_DAY_MS: i64 = 24 * 60 * 60 * 1000;
pub(crate) const MAX_RAW_SIZE: f64 = 1.4 * 1024.0 * 1024.0;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = crypto, js_name = getRandomValues)]
    fn get_random_values(array: &js_sys::Uint8Array);
}

#[derive(Deserialize)]
struct VersionMetadata {
    id: String,
}

#[derive(Serialize)]
struct ErrorJson {
    error: String,
}

pub(crate) fn bearer(req: &Request) -> Option<String> {
    req.headers()
        .get("authorization")
        .ok()
        .flatten()
        .and_then(|value| value.strip_prefix("Bearer ").map(str::to_owned))
}

pub(crate) fn token(req: &Request) -> Option<String> {
    bearer(req).or_else(|| {
        req.url().ok().and_then(|url| {
            url.query_pairs()
                .find(|(key, _)| key == "token")
                .map(|(_, value)| value.into_owned())
        })
    })
}

pub(crate) fn json_error(message: &str, status: u16) -> Result<Response> {
    Response::from_json(&ErrorJson {
        error: message.to_owned(),
    })
    .map(|response| response.with_status(status))
}

pub(crate) fn add_version_header(response: &mut Response, env: &Env) -> Result<()> {
    if let Ok(version) = env.object_var::<VersionMetadata>("CF_VERSION") {
        let _ = response.headers_mut().set("X-Smails-Version", &version.id);
    }
    Ok(())
}

pub(crate) fn domains(env: &Env) -> Vec<String> {
    let value = env
        .var("DOMAINS")
        .map(|value| value.to_string())
        .unwrap_or_else(|_| DEFAULT_DOMAIN.to_owned());
    let domains: Vec<_> = value
        .split(',')
        .map(str::trim)
        .filter(|domain| !domain.is_empty())
        .map(str::to_owned)
        .collect();
    if domains.is_empty() {
        vec![DEFAULT_DOMAIN.to_owned()]
    } else {
        domains
    }
}

fn random_bytes(bytes_len: usize) -> Vec<u8> {
    let array = js_sys::Uint8Array::new_with_length(bytes_len as u32);
    get_random_values(&array);
    array.to_vec()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub(crate) fn random_hex(bytes_len: usize) -> String {
    hex(&random_bytes(bytes_len))
}

pub(crate) fn constant_time_eq(a: &str, b: &str) -> bool {
    let a = a.as_bytes();
    let b = b.as_bytes();
    if a.len() != b.len() {
        return false;
    }
    a.iter()
        .zip(b)
        .fold(0, |diff, (left, right)| diff | (left ^ right))
        == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEMO_TOKEN: &str = "demo.0123456789abcdef0123456789abcdef";

    #[test]
    fn compares_tokens_without_early_success() {
        assert!(constant_time_eq(DEMO_TOKEN, DEMO_TOKEN));
        assert!(!constant_time_eq(
            DEMO_TOKEN,
            "demo.0123456789abcdef0123456789abcdee"
        ));
    }
}
