mod admin;
mod admin_schema;
mod http;
mod mail;
mod mailbox;
mod mailbox_schema;
mod migration;
mod mime;
mod support;

use std::cell::Cell;

use wasm_bindgen::prelude::*;
use worker::{
    Context, Env, ForwardableEmailMessage, Request, Response, Result, State, durable_object, event,
};

#[durable_object]
pub struct Mailbox {
    pub(crate) state: State,
    pub(crate) schema_ready: Cell<bool>,
}

#[durable_object]
pub struct Admin {
    pub(crate) state: State,
    pub(crate) schema_ready: Cell<bool>,
}

#[event(fetch, respond_with_errors)]
pub async fn fetch(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    let mut response = http::handle_fetch(req, &env).await?;
    support::add_version_header(&mut response, &env)?;
    Ok(response)
}

#[wasm_bindgen]
pub async fn email(
    message: ForwardableEmailMessage,
    env: Env,
    _ctx: JsValue,
) -> std::result::Result<(), JsValue> {
    mail::handle_email(message, env).await
}
