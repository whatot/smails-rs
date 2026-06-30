mod admin;
mod admin_schema;
mod fixed_window;
mod http;
mod mail;
mod mailbox;
mod mailbox_schema;
mod migration;
mod mime;
mod rate_limit;
mod support;

use std::cell::{Cell, RefCell};

use wasm_bindgen::prelude::*;
use worker::{
    Context, Env, ForwardableEmailMessage, Request, Response, Result, State, durable_object, event,
};

#[durable_object]
pub struct Mailbox {
    pub(crate) state: State,
    pub(crate) schema_ready: Cell<bool>,
    pub(crate) deliver_window_started_at_ms: Cell<i64>,
    pub(crate) deliver_count: Cell<i64>,
}

#[durable_object]
pub struct Admin {
    pub(crate) state: State,
    pub(crate) schema_ready: Cell<bool>,
}

#[durable_object]
pub struct RateLimit {
    pub(crate) limiter: RefCell<fixed_window::Limiter>,
}

#[event(fetch, respond_with_errors)]
pub async fn fetch(req: Request, env: Env, ctx: Context) -> Result<Response> {
    let mut response = http::handle_fetch(req, &env, &ctx).await?;
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
