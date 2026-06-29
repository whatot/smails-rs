use serde::Deserialize;
use smails_core::{
    CapabilityJson, CreateMailboxRequest, DEFAULT_DOMAIN, DeliverMessage, MailboxCreated,
    MessageDetail, MessageSummary, OkJson, PATH_DOMAINS, PATH_MAILBOX, PATH_MESSAGES,
    authorization_header, mailbox_name_from_address, mailbox_name_from_token, preview_text,
};
use wasm_bindgen::prelude::*;
use worker::{
    Context, DurableObject, Env, ForwardableEmailMessage, Method, Request, RequestInit, Response,
    Result, State, WebSocket, WebSocketIncomingMessage, WebSocketPair, durable_object, event,
};

const MAILBOX_BINDING: &str = "MAILBOX";
const EXPIRY_MS: i64 = 7 * 24 * 60 * 60 * 1000;

#[derive(Deserialize)]
struct TestEmail {
    to: String,
    from: String,
    subject: String,
    body: String,
}

#[derive(Deserialize)]
struct StoredMessage {
    id: String,
    from_addr: String,
    from_name: String,
    subject: String,
    body: String,
    received_at: i64,
    read: i64,
}

fn bearer(req: &Request) -> Option<String> {
    req.headers()
        .get("authorization")
        .ok()
        .flatten()
        .and_then(|value| value.strip_prefix("Bearer ").map(str::to_owned))
}

fn token(req: &Request) -> Option<String> {
    bearer(req).or_else(|| {
        req.url().ok().and_then(|url| {
            url.query_pairs()
                .find(|(key, _)| key == "token")
                .map(|(_, value)| value.into_owned())
        })
    })
}

fn random_hex(bytes_len: usize) -> String {
    (0..bytes_len)
        .map(|_| (js_sys::Math::random() * 256.0) as u8)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn random_mailbox_name() -> String {
    format!("mail-{}", random_hex(4))
}

async fn deliver(env: &Env, to: &str, from: &str, subject: &str, body: &str) -> Result<()> {
    let namespace = env.durable_object(MAILBOX_BINDING)?;
    let stub = namespace.get_by_name(mailbox_name_from_address(to))?;
    let preview = preview_text(body);

    let mut init = RequestInit::new();
    init.with_method(Method::Post);
    init.with_body(Some(JsValue::from_str(&serde_json::to_string(
        &DeliverMessage {
            from_addr: from.to_owned(),
            from_name: from.to_owned(),
            subject: subject.to_owned(),
            preview,
            body: body.to_owned(),
        },
    )?)));
    let req = Request::new_with_init("https://do.internal/deliver", &init)?;
    stub.fetch_with_request(req).await?;
    Ok(())
}

#[event(fetch, respond_with_errors)]
pub async fn fetch(mut req: Request, env: Env, _ctx: Context) -> Result<Response> {
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
        (Method::Get, PATH_DOMAINS) => Response::from_json(&vec![DEFAULT_DOMAIN]),
        (Method::Post, PATH_MAILBOX) => {
            let body = req
                .json::<CreateMailboxRequest>()
                .await
                .unwrap_or(CreateMailboxRequest {
                    domain: None,
                    address: None,
                    token: None,
                });
            let address = body.address.unwrap_or_else(random_mailbox_name);
            let domain = body.domain.unwrap_or_else(|| DEFAULT_DOMAIN.to_owned());
            let token = body
                .token
                .unwrap_or_else(|| format!("{address}.{}", random_hex(16)));
            let namespace = env.durable_object(MAILBOX_BINDING)?;
            let stub = namespace.get_by_name(&address)?;

            let mut init = RequestInit::new();
            init.with_method(Method::Post);
            init.with_body(Some(JsValue::from_str(&token)));
            let create_req = Request::new_with_init("https://do.internal/create", &init)?;
            stub.fetch_with_request(create_req).await?;

            Response::from_json(&MailboxCreated {
                address: format!("{address}@{domain}"),
                token,
            })
        }
        (Method::Get, PATH_MESSAGES) => {
            let Some(token) = bearer(&req) else {
                return Response::error("missing bearer token", 401);
            };
            let Some(address) = mailbox_name_from_token(&token) else {
                return Response::error("invalid bearer token", 401);
            };
            let namespace = env.durable_object(MAILBOX_BINDING)?;
            let stub = namespace.get_by_name(&address)?;

            let mut list_req = Request::new("https://do.internal/messages", Method::Get)?;
            list_req
                .headers_mut()?
                .set("authorization", &authorization_header(&token))?;
            stub.fetch_with_request(list_req).await
        }
        (Method::Get, path) if path.starts_with(&message_prefix) => {
            let Some(token) = bearer(&req) else {
                return Response::error("missing bearer token", 401);
            };
            let Some(address) = mailbox_name_from_token(&token) else {
                return Response::error("invalid bearer token", 401);
            };
            let namespace = env.durable_object(MAILBOX_BINDING)?;
            let stub = namespace.get_by_name(&address)?;

            let id = &path[message_prefix.len()..];
            let mut detail_req =
                Request::new(&format!("https://do.internal/messages/{id}"), Method::Get)?;
            detail_req
                .headers_mut()?
                .set("authorization", &authorization_header(&token))?;
            stub.fetch_with_request(detail_req).await
        }
        (Method::Delete, path) if path.starts_with(&message_prefix) => {
            let Some(token) = bearer(&req) else {
                return Response::error("missing bearer token", 401);
            };
            let Some(address) = mailbox_name_from_token(&token) else {
                return Response::error("invalid bearer token", 401);
            };
            let namespace = env.durable_object(MAILBOX_BINDING)?;
            let stub = namespace.get_by_name(&address)?;

            let id = &path[message_prefix.len()..];
            let mut delete_req = Request::new(
                &format!("https://do.internal/messages/{id}"),
                Method::Delete,
            )?;
            delete_req
                .headers_mut()?
                .set("authorization", &authorization_header(&token))?;
            stub.fetch_with_request(delete_req).await
        }
        (Method::Get, "/api/mailbox/connect") => {
            let Some(token) = token(&req) else {
                return Response::error("missing token", 401);
            };
            let Some(address) = mailbox_name_from_token(&token) else {
                return Response::error("invalid token", 401);
            };
            let namespace = env.durable_object(MAILBOX_BINDING)?;
            let stub = namespace.get_by_name(&address)?;
            stub.fetch_with_request(req).await
        }
        (Method::Post, "/__test/email") => {
            let body = req.json::<TestEmail>().await?;
            deliver(&env, &body.to, &body.from, &body.subject, &body.body).await?;
            Response::from_json(&OkJson { ok: true })
        }
        _ => Response::error("not found", 404),
    }
}

#[wasm_bindgen]
pub async fn email(
    message: ForwardableEmailMessage,
    env: Env,
    _ctx: JsValue,
) -> std::result::Result<(), JsValue> {
    let subject = message
        .headers()
        .get("subject")
        .ok()
        .flatten()
        .unwrap_or_default();
    let raw = match message.raw_bytes().await {
        Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
        Err(err) => return Err(JsValue::from_str(&format!("raw read failed: {err}"))),
    };

    if let Err(err) = deliver(&env, &message.to(), &message.from(), &subject, &raw).await {
        return Err(JsValue::from_str(&format!("delivery failed: {err}")));
    }

    Ok(())
}

#[durable_object]
pub struct Mailbox {
    state: State,
}

impl Mailbox {
    async fn touch(&self) -> Result<()> {
        self.state.storage().set_alarm(EXPIRY_MS).await
    }

    fn init_schema(&self) -> Result<()> {
        self.state.storage().sql().exec(
            "CREATE TABLE IF NOT EXISTS messages (
                id TEXT PRIMARY KEY,
                from_addr TEXT NOT NULL,
                from_name TEXT NOT NULL,
                subject TEXT NOT NULL,
                preview TEXT NOT NULL,
                body TEXT NOT NULL,
                received_at INTEGER NOT NULL,
                read INTEGER NOT NULL DEFAULT 0
            )",
            None,
        )?;
        Ok(())
    }

    async fn auth(&self, req: &Request) -> Result<bool> {
        let expected = self.state.storage().get::<String>("token").await?;
        Ok(expected.is_some() && expected == token(req))
    }
}

impl DurableObject for Mailbox {
    fn new(state: State, _env: Env) -> Self {
        let mailbox = Self { state };
        mailbox.init_schema().expect("create schema");
        mailbox
    }

    async fn fetch(&self, mut req: Request) -> Result<Response> {
        let path = req.path();
        let message_prefix = "/messages/";

        match (req.method(), path.as_str()) {
            (Method::Post, "/create") => {
                let token = req.text().await?;
                self.state.storage().put("token", token).await?;
                self.touch().await?;
                Response::from_json(&OkJson { ok: true })
            }
            (Method::Get, "/messages") => {
                if !self.auth(&req).await? {
                    return Response::error("unauthorized", 401);
                }
                self.touch().await?;
                let rows = self
                    .state
                    .storage()
                    .sql()
                    .exec(
                        "SELECT id, from_addr, from_name, subject, preview, received_at, read
                         FROM messages
                         ORDER BY received_at DESC
                         LIMIT 100",
                        None,
                    )?
                    .to_array::<MessageSummary>()?;
                Response::from_json(&rows)
            }
            (Method::Get, path) if path.starts_with(message_prefix) => {
                if !self.auth(&req).await? {
                    return Response::error("unauthorized", 401);
                }
                self.touch().await?;
                let id = &path[message_prefix.len()..];
                self.state
                    .storage()
                    .sql()
                    .exec("UPDATE messages SET read = 1 WHERE id = ?", vec![id.into()])?;
                let rows = self
                    .state
                    .storage()
                    .sql()
                    .exec("SELECT * FROM messages WHERE id = ?", vec![id.into()])?
                    .to_array::<StoredMessage>()?;
                let Some(row) = rows.into_iter().next() else {
                    return Response::error("message not found", 404);
                };
                Response::from_json(&MessageDetail {
                    id: row.id,
                    from_addr: row.from_addr,
                    from_name: row.from_name,
                    subject: row.subject,
                    received_at: row.received_at,
                    read: row.read,
                    html: None,
                    text: Some(row.body),
                    attachments: Vec::new(),
                })
            }
            (Method::Delete, path) if path.starts_with(message_prefix) => {
                if !self.auth(&req).await? {
                    return Response::error("unauthorized", 401);
                }
                self.touch().await?;
                let id = &path[message_prefix.len()..];
                self.state
                    .storage()
                    .sql()
                    .exec("DELETE FROM messages WHERE id = ?", vec![id.into()])?;
                Response::from_json(&OkJson { ok: true })
            }
            (Method::Post, "/deliver") => {
                let body = req.json::<DeliverMessage>().await?;
                self.touch().await?;
                if self.state.storage().get::<String>("token").await?.is_none() {
                    return Response::from_json(&OkJson { ok: true });
                }

                let received_at = worker::Date::now().as_millis() as i64;
                let id = format!("msg-{received_at}");
                self.state.storage().sql().exec(
                    "INSERT INTO messages (id, from_addr, from_name, subject, preview, body, received_at)
                     VALUES (?, ?, ?, ?, ?, ?, ?)",
                    vec![
                        id.clone().into(),
                        body.from_addr.into(),
                        body.from_name.into(),
                        body.subject.into(),
                        body.preview.into(),
                        body.body.into(),
                        received_at.into(),
                    ],
                )?;

                let event = serde_json::json!({ "type": "new_message", "id": id }).to_string();
                for ws in self.state.get_websockets() {
                    ws.send_with_str(&event)?;
                }

                Response::from_json(&OkJson { ok: true })
            }
            (Method::Get, "/api/mailbox/connect") => {
                if !self.auth(&req).await? {
                    return Response::error("unauthorized", 401);
                }
                let pair = WebSocketPair::new()?;
                self.state.accept_web_socket(&pair.server);
                self.touch().await?;
                Response::from_websocket(pair.client)
            }
            _ => Response::error("not found", 404),
        }
    }

    async fn alarm(&self) -> Result<Response> {
        for ws in self.state.get_websockets() {
            ws.close(Some(1000), Some("expired"))?;
        }
        self.state.storage().delete_all().await?;
        Response::empty()
    }

    async fn websocket_message(
        &self,
        ws: WebSocket,
        message: WebSocketIncomingMessage,
    ) -> Result<()> {
        if matches!(message, WebSocketIncomingMessage::String(value) if value == "ping") {
            ws.send_with_str("pong")?;
        }
        Ok(())
    }

    async fn websocket_close(
        &self,
        _ws: WebSocket,
        _code: usize,
        _reason: String,
        _was_clean: bool,
    ) -> Result<()> {
        Ok(())
    }

    async fn websocket_error(&self, _ws: WebSocket, _error: worker::Error) -> Result<()> {
        Ok(())
    }
}
