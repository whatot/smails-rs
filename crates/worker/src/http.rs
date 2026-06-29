use serde::Deserialize;
use smails_core::{
    CapabilityJson, CreateMailboxRequest, MailboxCreated, OkJson, PATH_DOMAINS, PATH_MAILBOX,
    PATH_MESSAGES, authorization_header, mailbox_name_from_token,
};
use wasm_bindgen::JsValue;
use worker::{Env, Method, Request, RequestInit, Response, Result};

use crate::{
    mail::{deliver, raw_email},
    support::{MAILBOX_BINDING, bearer, domains, json_error, random_hex, token},
};

#[derive(Deserialize)]
struct TestEmail {
    to: String,
    from: String,
    subject: String,
    body: String,
}

fn random_mailbox_name() -> String {
    format!("mail-{}", random_hex(4))
}

pub(crate) async fn handle_fetch(mut req: Request, env: &Env) -> Result<Response> {
    let path = req.path();
    let message_prefix = format!("{PATH_MESSAGES}/");

    match (req.method(), path.as_str()) {
        (Method::Get, "/health") => Response::from_json(&CapabilityJson {
            fetch: true,
            durable_object: true,
            sqlite_storage: true,
            websocket: true,
            alarm: true,
            email_export: true,
        }),
        (Method::Get, PATH_DOMAINS) => Response::from_json(&domains(env)),
        (Method::Post, PATH_MAILBOX) => create_mailbox(req, env).await,
        (Method::Get, PATH_MESSAGES) => forward_authed(req, env, "messages", Method::Get).await,
        (Method::Get, path) if path.starts_with(&message_prefix) => {
            let id = &path[message_prefix.len()..];
            forward_authed(req, env, &format!("messages/{id}"), Method::Get).await
        }
        (Method::Delete, path) if path.starts_with(&message_prefix) => {
            let id = &path[message_prefix.len()..];
            forward_authed(req, env, &format!("messages/{id}"), Method::Delete).await
        }
        (Method::Get, "/api/mailbox/connect") => {
            let Some(token) = token(&req) else {
                return json_error("Unauthorized", 401);
            };
            let Some(address) = mailbox_name_from_token(&token) else {
                return json_error("Invalid token", 401);
            };
            let namespace = env.durable_object(MAILBOX_BINDING)?;
            let stub = namespace.get_by_name(&address)?;
            stub.fetch_with_request(req).await
        }
        (Method::Post, "/__test/email") => {
            let body = req.json::<TestEmail>().await?;
            deliver(
                env,
                &body.to,
                &body.from,
                &raw_email(&body.from, &body.subject, &body.body),
            )
            .await?;
            Response::from_json(&OkJson { ok: true })
        }
        _ => json_error("Not found", 404),
    }
}

async fn create_mailbox(mut req: Request, env: &Env) -> Result<Response> {
    let body = req
        .json::<CreateMailboxRequest>()
        .await
        .unwrap_or(CreateMailboxRequest {
            domain: None,
            address: None,
            token: None,
        });
    let domains = domains(env);
    let domain = body
        .domain
        .filter(|domain| domains.contains(domain))
        .unwrap_or_else(|| domains[0].clone());
    let address = body.address.unwrap_or_else(random_mailbox_name);
    let token = body
        .token
        .unwrap_or_else(|| format!("{address}.{}", random_hex(16)));

    if mailbox_name_from_token(&token).as_deref() != Some(address.as_str()) {
        return json_error("invalid mailbox token", 400);
    }

    let namespace = env.durable_object(MAILBOX_BINDING)?;
    let stub = namespace.get_by_name(&address)?;

    let mut init = RequestInit::new();
    init.with_method(Method::Post);
    init.with_body(Some(JsValue::from_str(&token)));
    let create_req = Request::new_with_init("https://do.internal/create", &init)?;
    let create_response = stub.fetch_with_request(create_req).await?;
    if create_response.status_code() == 409 {
        return Ok(create_response);
    }

    Response::from_json(&MailboxCreated {
        address: format!("{address}@{domain}"),
        token,
    })
    .map(|response| response.with_status(201))
}

async fn forward_authed(
    req: Request,
    env: &Env,
    do_path: &str,
    method: Method,
) -> Result<Response> {
    let Some(token) = bearer(&req) else {
        return json_error("Unauthorized", 401);
    };
    let Some(address) = mailbox_name_from_token(&token) else {
        return json_error("Invalid token", 401);
    };
    let namespace = env.durable_object(MAILBOX_BINDING)?;
    let stub = namespace.get_by_name(&address)?;

    let mut forwarded = Request::new(&format!("https://do.internal/{do_path}"), method)?;
    forwarded
        .headers_mut()?
        .set("authorization", &authorization_header(&token))?;
    stub.fetch_with_request(forwarded).await
}
