use smails_core::{
    CapabilityJson, CreateMailboxRequest, MailboxCreated, PATH_DOMAINS, PATH_MAILBOX,
    PATH_MESSAGES, authorization_header, is_mailbox_name, mailbox_name_from_token,
};
use wasm_bindgen::JsValue;
use worker::{Env, Method, Request, RequestInit, Response, Result};

use crate::{
    admin,
    support::{
        MAILBOX_BINDING, MAX_CREATE_BODY_SIZE, bearer, domains, json_error, random_hex, token,
    },
};

fn random_mailbox_name() -> String {
    format!("mail-{}", random_hex(4))
}

pub(crate) async fn handle_fetch(req: Request, env: &Env) -> Result<Response> {
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
        (Method::Get, PATH_DOMAINS) => match domains(env) {
            Some(domains) => Response::from_json(&domains),
            None => json_error("MAILBOX_DOMAINS is not configured", 500),
        },
        (Method::Get, "/admin/stats") => admin::handle_fetch(req, env).await,
        (Method::Post, PATH_MAILBOX) => create_mailbox(req, env).await,
        (Method::Get, PATH_MESSAGES) => forward_authed(req, env, "messages", Method::Get).await,
        (Method::Get, path) if path.starts_with(&message_prefix) => {
            let id = &path[message_prefix.len()..];
            if id.contains('/') {
                return json_error("Not found", 404);
            }
            forward_authed(req, env, &format!("messages/{id}"), Method::Get).await
        }
        (Method::Delete, path) if path.starts_with(&message_prefix) => {
            let id = &path[message_prefix.len()..];
            if id.contains('/') {
                return json_error("Not found", 404);
            }
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
        _ => json_error("Not found", 404),
    }
}

async fn create_mailbox(mut req: Request, env: &Env) -> Result<Response> {
    let body = match create_mailbox_body(&mut req).await? {
        Ok(body) => body,
        Err(response) => return Ok(response),
    };
    let Some(domains) = domains(env) else {
        return json_error("MAILBOX_DOMAINS is not configured", 500);
    };
    let domain = match mailbox_domain(body.domain, &domains) {
        Ok(domain) => domain,
        Err(()) => return json_error("invalid domain", 400),
    };
    let address = body.address.unwrap_or_else(random_mailbox_name);
    if !is_mailbox_name(&address) {
        return json_error("invalid mailbox address", 400);
    }
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
    if !(200..300).contains(&create_response.status_code()) {
        return Ok(create_response);
    }

    let mut create_response = create_response;
    if create_response.json::<CreateMailboxResult>().await?.created {
        admin::record_mailbox_created(env).await?;
    }
    Response::from_json(&MailboxCreated {
        address: format!("{address}@{domain}"),
        token,
    })
    .map(|response| response.with_status(201))
}

#[derive(serde::Deserialize)]
struct CreateMailboxResult {
    created: bool,
}

async fn create_mailbox_body(
    req: &mut Request,
) -> Result<std::result::Result<CreateMailboxRequest, Response>> {
    if req
        .headers()
        .get("content-length")?
        .and_then(|value| value.parse::<usize>().ok())
        .is_some_and(|size| size > MAX_CREATE_BODY_SIZE)
    {
        return Ok(Err(json_error("Request body too large", 413)?));
    }
    let text = req.text().await?;
    if text.len() > MAX_CREATE_BODY_SIZE {
        return Ok(Err(json_error("Request body too large", 413)?));
    }
    if text.trim().is_empty() {
        return Ok(Ok(CreateMailboxRequest {
            domain: None,
            address: None,
            token: None,
        }));
    }
    match serde_json::from_str(&text) {
        Ok(body) => Ok(Ok(body)),
        Err(_) => Ok(Err(json_error("Invalid JSON", 400)?)),
    }
}

fn mailbox_domain(
    requested: Option<String>,
    domains: &[String],
) -> std::result::Result<String, ()> {
    match requested {
        Some(domain) if domains.contains(&domain) => Ok(domain),
        Some(_) => Err(()),
        None => Ok(domains[0].clone()),
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unsupported_requested_domain() {
        let domains = vec!["example.com".to_owned()];

        assert_eq!(mailbox_domain(None, &domains).as_deref(), Ok("example.com"));
        assert_eq!(
            mailbox_domain(Some("example.com".to_owned()), &domains).as_deref(),
            Ok("example.com")
        );
        assert_eq!(
            mailbox_domain(Some("other.com".to_owned()), &domains),
            Err(())
        );
    }
}
