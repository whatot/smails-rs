use smails_core::mailbox_name_from_address;
use wasm_bindgen::prelude::*;
use worker::{Env, ForwardableEmailMessage, Method, Request, RequestInit, Result};

use crate::support::{MAILBOX_BINDING, MAX_RAW_SIZE};

#[derive(serde::Deserialize)]
struct DeliverResult {
    stored: bool,
}

pub(crate) async fn deliver(env: &Env, to: &str, from: &str, raw_bytes: &[u8]) -> Result<bool> {
    let mailbox = mailbox_name_from_address(to).to_ascii_lowercase();
    let namespace = env.durable_object(MAILBOX_BINDING)?;
    let stub = namespace.get_by_name(&mailbox)?;

    let mut init = RequestInit::new();
    init.with_method(Method::Post);
    init.with_body(Some(js_sys::Uint8Array::from(raw_bytes).into()));
    let mut req = Request::new_with_init("https://do.internal/deliver", &init)?;
    req.headers_mut()?.set("x-smails-from", from)?;
    let mut response = stub.fetch_with_request(req).await?;
    let status = response.status_code();
    if !(200..300).contains(&status) {
        return Err(format!("mailbox deliver failed: HTTP {status}").into());
    }
    Ok(response.json::<DeliverResult>().await?.stored)
}

pub(crate) async fn handle_email(
    message: ForwardableEmailMessage,
    env: Env,
) -> std::result::Result<(), JsValue> {
    if message.raw_size() > MAX_RAW_SIZE {
        message.set_reject("Message too large (max 512KB)");
        return Ok(());
    }

    let raw = match message.raw_bytes().await {
        Ok(bytes) => bytes,
        Err(err) => return Err(JsValue::from_str(&format!("raw read failed: {err}"))),
    };

    match deliver(&env, &message.to(), &message.from(), &raw).await {
        Ok(true) => {}
        Ok(false) => {}
        Err(err) => return Err(JsValue::from_str(&format!("delivery failed: {err}"))),
    }

    Ok(())
}
