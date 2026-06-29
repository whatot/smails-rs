use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;
use worker::{
    durable_object, event, Context, DurableObject, Env, ForwardableEmailMessage, Method, Request,
    RequestInit, Response, Result, State, WebSocket, WebSocketIncomingMessage, WebSocketPair,
};

const MAILBOX_BINDING: &str = "MAILBOX";
const EXPIRY_MS: i64 = 7 * 24 * 60 * 60 * 1000;

#[derive(Deserialize)]
struct CreateMailbox {
    address: Option<String>,
    token: Option<String>,
}

#[derive(Serialize)]
struct MailboxCreated {
    address: String,
    token: String,
}

#[derive(Deserialize)]
struct TestEmail {
    to: String,
    from: String,
    subject: String,
    body: String,
}

#[derive(Serialize)]
struct OkJson {
    ok: bool,
}

#[derive(Serialize)]
struct CapabilityJson {
    fetch: bool,
    durable_object: bool,
    sqlite_storage: bool,
    websocket: bool,
    alarm: bool,
    email_export: bool,
}

#[derive(Serialize, Deserialize)]
struct MessageRow {
    id: String,
    from_addr: String,
    subject: String,
    body: String,
    received_at: i64,
}

#[derive(Serialize, Deserialize)]
struct DeliverBody {
    from: String,
    subject: String,
    body: String,
}

fn mailbox_name(address: &str) -> &str {
    address.split('@').next().unwrap_or(address)
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

async fn deliver(env: &Env, to: &str, from: &str, subject: &str, body: &str) -> Result<()> {
    let namespace = env.durable_object(MAILBOX_BINDING)?;
    let stub = namespace.get_by_name(mailbox_name(to))?;

    let mut init = RequestInit::new();
    init.with_method(Method::Post);
    init.with_body(Some(JsValue::from_str(&serde_json::to_string(
        &DeliverBody {
            from: from.to_owned(),
            subject: subject.to_owned(),
            body: body.to_owned(),
        },
    )?)));
    let req = Request::new_with_init("https://do.internal/deliver", &init)?;
    stub.fetch_with_request(req).await?;
    Ok(())
}

#[event(fetch, respond_with_errors)]
pub async fn fetch(mut req: Request, env: Env, _ctx: Context) -> Result<Response> {
    match (req.method(), req.path().as_str()) {
        (Method::Get, "/health") => Response::from_json(&CapabilityJson {
            fetch: true,
            durable_object: true,
            sqlite_storage: true,
            websocket: true,
            alarm: true,
            email_export: true,
        }),
        (Method::Post, "/api/mailbox") => {
            let body = req.json::<CreateMailbox>().await.unwrap_or(CreateMailbox {
                address: None,
                token: None,
            });
            let address = body.address.unwrap_or_else(|| "demo".to_owned());
            let token = body.token.unwrap_or_else(|| "demo.secret".to_owned());
            let namespace = env.durable_object(MAILBOX_BINDING)?;
            let stub = namespace.get_by_name(&address)?;

            let mut init = RequestInit::new();
            init.with_method(Method::Post);
            init.with_body(Some(JsValue::from_str(&token)));
            let create_req = Request::new_with_init("https://do.internal/create", &init)?;
            stub.fetch_with_request(create_req).await?;

            Response::from_json(&MailboxCreated {
                address: format!("{address}@smails.dev"),
                token,
            })
        }
        (Method::Get, "/api/mailbox/messages") => {
            let Some(token) = bearer(&req) else {
                return Response::error("missing bearer token", 401);
            };
            let namespace = env.durable_object(MAILBOX_BINDING)?;
            let stub = namespace.get_by_name("demo")?;

            let mut list_req = Request::new("https://do.internal/messages", Method::Get)?;
            list_req
                .headers_mut()?
                .set("authorization", &format!("Bearer {token}"))?;
            stub.fetch_with_request(list_req).await
        }
        (Method::Get, "/api/mailbox/connect") => {
            let namespace = env.durable_object(MAILBOX_BINDING)?;
            let stub = namespace.get_by_name("demo")?;
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
                subject TEXT NOT NULL,
                body TEXT NOT NULL,
                received_at INTEGER NOT NULL
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
        match (req.method(), req.path().as_str()) {
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
                        "SELECT id, from_addr, subject, body, received_at
                         FROM messages
                         ORDER BY received_at DESC
                         LIMIT 100",
                        None,
                    )?
                    .to_array::<MessageRow>()?;
                Response::from_json(&rows)
            }
            (Method::Post, "/deliver") => {
                let body = req.json::<DeliverBody>().await?;
                self.touch().await?;
                if self.state.storage().get::<String>("token").await?.is_none() {
                    return Response::from_json(&OkJson { ok: true });
                }

                let received_at = worker::Date::now().as_millis() as i64;
                let id = format!("msg-{received_at}");
                self.state.storage().sql().exec(
                    "INSERT INTO messages (id, from_addr, subject, body, received_at)
                     VALUES (?, ?, ?, ?, ?)",
                    vec![
                        id.clone().into(),
                        body.from.into(),
                        body.subject.into(),
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
