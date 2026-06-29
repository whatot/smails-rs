use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use serde::Deserialize;
use smails_core::{DeliverMessage, MessageDetail, MessageSummary, OkJson};
use worker::{
    DurableObject, Env, Method, Request, Response, Result, State, WebSocket,
    WebSocketIncomingMessage, WebSocketPair,
};

use crate::{
    Mailbox,
    mime::parse_body_parts,
    support::{EXPIRY_MS, ONE_DAY_MS, constant_time_eq, json_error, random_hex, token},
};

#[derive(Deserialize)]
struct StoredMessage {
    id: String,
    from_addr: String,
    from_name: String,
    subject: String,
    raw: String,
    received_at: i64,
    read: i64,
}

impl Mailbox {
    async fn touch(&self) -> Result<()> {
        self.state.storage().set_alarm(EXPIRY_MS).await
    }

    async fn refresh_if_stale(&self) -> Result<()> {
        let current = self.state.storage().get_alarm().await?;
        let now = worker::Date::now().as_millis() as i64;
        if current.is_none_or(|alarm| alarm - now < EXPIRY_MS - ONE_DAY_MS) {
            self.touch().await?;
        }
        Ok(())
    }

    fn init_schema(&self) -> Result<()> {
        self.state.storage().sql().exec(
            "CREATE TABLE IF NOT EXISTS messages (
                id TEXT PRIMARY KEY,
                from_addr TEXT NOT NULL,
                from_name TEXT NOT NULL,
                subject TEXT NOT NULL,
                preview TEXT NOT NULL,
                raw TEXT NOT NULL,
                received_at INTEGER NOT NULL,
                read INTEGER NOT NULL DEFAULT 0
            )",
            None,
        )?;
        // ponytail: local spike migration only; replace with a real DO data migration before prod reuse.
        let _ = self.state.storage().sql().exec(
            "ALTER TABLE messages ADD COLUMN raw TEXT NOT NULL DEFAULT ''",
            None,
        );
        Ok(())
    }

    async fn auth(&self, req: &Request) -> Result<bool> {
        let expected = self.state.storage().get::<String>("token").await?;
        Ok(expected
            .as_deref()
            .zip(token(req).as_deref())
            .is_some_and(|(expected, actual)| constant_time_eq(expected, actual)))
    }
}

impl DurableObject for Mailbox {
    fn new(state: State, _env: Env) -> Self {
        let mailbox = Self { state };
        mailbox.init_schema().expect("create schema");
        mailbox
    }

    async fn fetch(&self, req: Request) -> Result<Response> {
        let path = req.path();
        let message_prefix = "/messages/";

        match (req.method(), path.as_str()) {
            (Method::Post, "/create") => self.create(req).await,
            (Method::Get, "/messages") => self.list(req).await,
            (Method::Get, path) if path.starts_with(message_prefix) => {
                self.read(req, &path[message_prefix.len()..]).await
            }
            (Method::Delete, path) if path.starts_with(message_prefix) => {
                self.delete(req, &path[message_prefix.len()..]).await
            }
            (Method::Post, "/deliver") => self.deliver(req).await,
            (Method::Get, "/api/mailbox/connect") => self.connect(req).await,
            _ => json_error("Not found", 404),
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
            self.refresh_if_stale().await?;
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

impl Mailbox {
    async fn create(&self, mut req: Request) -> Result<Response> {
        let new_token = req.text().await?;
        let existing = self.state.storage().get::<String>("token").await?;
        if existing
            .as_deref()
            .is_some_and(|token| !constant_time_eq(token, &new_token))
        {
            return json_error("Mailbox already exists", 409);
        }
        self.state.storage().put("token", new_token).await?;
        self.touch().await?;
        Response::from_json(&OkJson { ok: true })
    }

    async fn list(&self, req: Request) -> Result<Response> {
        if !self.auth(&req).await? {
            return json_error("Unauthorized", 401);
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

    async fn read(&self, req: Request, id: &str) -> Result<Response> {
        if !self.auth(&req).await? {
            return json_error("Unauthorized", 401);
        }
        self.touch().await?;
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
            return json_error("Message not found", 404);
        };
        let raw = BASE64
            .decode(row.raw)
            .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
            .unwrap_or_default();
        let parts = parse_body_parts(&raw);
        Response::from_json(&MessageDetail {
            id: row.id,
            from_addr: row.from_addr,
            from_name: row.from_name,
            subject: row.subject,
            received_at: row.received_at,
            read: row.read,
            html: parts.html,
            text: parts.text,
            attachments: Vec::new(),
        })
    }

    async fn delete(&self, req: Request, id: &str) -> Result<Response> {
        if !self.auth(&req).await? {
            return json_error("Unauthorized", 401);
        }
        self.touch().await?;
        self.state
            .storage()
            .sql()
            .exec("DELETE FROM messages WHERE id = ?", vec![id.into()])?;
        Response::from_json(&OkJson { ok: true })
    }

    async fn deliver(&self, mut req: Request) -> Result<Response> {
        let body = req.json::<DeliverMessage>().await?;
        self.touch().await?;
        if self.state.storage().get::<String>("token").await?.is_none() {
            return Response::from_json(&OkJson { ok: true });
        }

        let received_at = worker::Date::now().as_millis() as i64;
        let id = format!("msg-{}", random_hex(16));
        self.state.storage().sql().exec(
            "INSERT INTO messages (id, from_addr, from_name, subject, preview, raw, received_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
            vec![
                id.clone().into(),
                body.from_addr.into(),
                body.from_name.into(),
                body.subject.into(),
                body.preview.into(),
                body.raw.into(),
                received_at.into(),
            ],
        )?;

        let event = serde_json::json!({ "type": "new_message", "id": id }).to_string();
        for ws in self.state.get_websockets() {
            ws.send_with_str(&event)?;
        }

        Response::from_json(&OkJson { ok: true })
    }

    async fn connect(&self, req: Request) -> Result<Response> {
        if !self.auth(&req).await? {
            return json_error("Unauthorized", 401);
        }
        let pair = WebSocketPair::new()?;
        self.state.accept_web_socket(&pair.server);
        self.touch().await?;
        Response::from_websocket(pair.client)
    }
}
