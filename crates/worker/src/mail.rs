use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use smails_core::{DeliverMessage, mailbox_name_from_address};
use wasm_bindgen::prelude::*;
use worker::{Env, ForwardableEmailMessage, Method, Request, RequestInit, Result};

use crate::{
    mime::display_fields,
    support::{MAILBOX_BINDING, MAX_RAW_SIZE},
};

pub(crate) fn raw_email(from: &str, subject: &str, body: &str) -> Vec<u8> {
    format!(
        "From: {from}\r\nSubject: {subject}\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n{body}"
    )
    .into_bytes()
}

pub(crate) async fn deliver(env: &Env, to: &str, from: &str, raw_bytes: &[u8]) -> Result<()> {
    let mailbox = mailbox_name_from_address(to).to_ascii_lowercase();
    let namespace = env.durable_object(MAILBOX_BINDING)?;
    let stub = namespace.get_by_name(&mailbox)?;
    let raw_text = String::from_utf8_lossy(raw_bytes);
    let display = display_fields(&raw_text, from);

    let mut init = RequestInit::new();
    init.with_method(Method::Post);
    init.with_body(Some(JsValue::from_str(&serde_json::to_string(
        &DeliverMessage {
            from_addr: from.to_owned(),
            from_name: display.from_name,
            subject: display.subject,
            preview: display.preview,
            raw: BASE64.encode(raw_bytes),
        },
    )?)));
    let req = Request::new_with_init("https://do.internal/deliver", &init)?;
    stub.fetch_with_request(req).await?;
    Ok(())
}

pub(crate) async fn handle_email(
    message: ForwardableEmailMessage,
    env: Env,
) -> std::result::Result<(), JsValue> {
    if message.raw_size() > MAX_RAW_SIZE {
        message.set_reject("Message too large (max 1.4MB)");
        return Ok(());
    }

    let raw = match message.raw_bytes().await {
        Ok(bytes) => bytes,
        Err(err) => return Err(JsValue::from_str(&format!("raw read failed: {err}"))),
    };

    if let Err(err) = deliver(&env, &message.to(), &message.from(), &raw).await {
        return Err(JsValue::from_str(&format!("delivery failed: {err}")));
    }

    Ok(())
}
