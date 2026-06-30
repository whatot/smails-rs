use smails_core::{DeliverMessage, mailbox_name_from_address};
use wasm_bindgen::prelude::*;
use worker::{Env, ForwardableEmailMessage, Method, Request, RequestInit, Result};

use crate::{
    mime::{display_fields, parse_mail},
    support::{MAILBOX_BINDING, MAX_RAW_SIZE},
};

pub(crate) async fn deliver(env: &Env, to: &str, from: &str, raw_bytes: &[u8]) -> Result<()> {
    let mailbox = mailbox_name_from_address(to).to_ascii_lowercase();
    let namespace = env.durable_object(MAILBOX_BINDING)?;
    let stub = namespace.get_by_name(&mailbox)?;
    let display = display_fields(raw_bytes, from);
    let parts = parse_mail(raw_bytes);

    let mut init = RequestInit::new();
    init.with_method(Method::Post);
    init.with_body(Some(JsValue::from_str(&serde_json::to_string(
        &DeliverMessage {
            from_addr: from.to_owned(),
            from_name: display.from_name,
            subject: display.subject,
            preview: display.preview,
            html: parts.html,
            text: parts.text,
            attachments: parts.attachments,
        },
    )?)));
    let req = Request::new_with_init("https://do.internal/deliver", &init)?;
    let response = stub.fetch_with_request(req).await?;
    let status = response.status_code();
    if !(200..300).contains(&status) {
        return Err(format!("mailbox deliver failed: HTTP {status}").into());
    }
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
