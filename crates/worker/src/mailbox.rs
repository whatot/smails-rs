use serde::Deserialize;
use smails_core::{DeliverMessage, MessageDetail, MessageSummary, OkJson};
use worker::{
    DurableObject, Env, Method, Request, Response, Result, State, WebSocket,
    WebSocketIncomingMessage, WebSocketPair,
};

use crate::{
    Mailbox,
    mailbox_schema::init_schema,
    mime::{display_fields, parse_mail},
    support::{
        EXPIRY_MS, MAX_MESSAGES_PER_MAILBOX, ONE_DAY_MS, constant_time_eq, json_error, random_hex,
        token,
    },
};

#[derive(Deserialize)]
struct StoredMessage {
    id: String,
    from_addr: String,
    from_name: String,
    subject: String,
    html: String,
    text: String,
    attachments: String,
    received_at: i64,
    read: i64,
}

#[derive(serde::Serialize)]
struct DeliverResult {
    stored: bool,
}

#[derive(serde::Serialize)]
struct CreateResult {
    created: bool,
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
        Self {
            state,
            schema_ready: std::cell::Cell::new(false),
        }
    }

    async fn fetch(&self, req: Request) -> Result<Response> {
        if let Some(response) = self.ensure_schema()? {
            return Ok(response);
        }

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
        self.schema_ready.set(false);
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
    fn ensure_schema(&self) -> Result<Option<Response>> {
        if self.schema_ready.get() {
            return Ok(None);
        }
        let sql = self.state.storage().sql();
        if init_schema(&sql).is_err() {
            return json_error("Schema migration failed", 500).map(Some);
        }
        self.schema_ready.set(true);
        Ok(None)
    }

    async fn create(&self, mut req: Request) -> Result<Response> {
        let new_token = req.text().await?;
        let existing = self.state.storage().get::<String>("token").await?;
        if existing
            .as_deref()
            .is_some_and(|token| !constant_time_eq(token, &new_token))
        {
            return json_error("Mailbox already exists", 409);
        }
        let created = existing.is_none();
        self.state.storage().put("token", new_token).await?;
        self.touch().await?;
        Response::from_json(&CreateResult { created })
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
        let attachments = serde_json::from_str(&row.attachments).unwrap_or_default();
        Response::from_json(&MessageDetail {
            id: row.id,
            from_addr: row.from_addr,
            from_name: row.from_name,
            subject: row.subject,
            received_at: row.received_at,
            read: row.read,
            html: present(row.html),
            text: present(row.text),
            attachments,
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
        self.touch().await?;
        if self.state.storage().get::<String>("token").await?.is_none() {
            return Response::from_json(&DeliverResult { stored: false });
        }

        let from = req.headers().get("x-smails-from")?.unwrap_or_default();
        let raw = req.bytes().await?;
        let display = display_fields(&raw, &from);
        let parts = parse_mail(&raw);
        let body = DeliverMessage {
            from_addr: from,
            from_name: display.from_name,
            subject: display.subject,
            preview: display.preview,
            html: parts.html,
            text: parts.text,
            attachments: parts.attachments,
        };

        let received_at = worker::Date::now().as_millis() as i64;
        let id = format!("msg-{}", random_hex(16));
        let attachments = serde_json::to_string(&body.attachments)?;
        self.state.storage().sql().exec(
            "INSERT INTO messages (id, from_addr, from_name, subject, preview, html, text, attachments, received_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            vec![
                id.clone().into(),
                body.from_addr.into(),
                body.from_name.into(),
                body.subject.into(),
                body.preview.into(),
                body.html.unwrap_or_default().into(),
                body.text.unwrap_or_default().into(),
                attachments.into(),
                received_at.into(),
            ],
        )?;
        self.state.storage().sql().exec(
            "DELETE FROM messages
             WHERE id NOT IN (
                SELECT id FROM messages ORDER BY received_at DESC LIMIT ?
             )",
            vec![MAX_MESSAGES_PER_MAILBOX.into()],
        )?;

        let event = serde_json::json!({ "type": "new_message", "id": id }).to_string();
        for ws in self.state.get_websockets() {
            ws.send_with_str(&event)?;
        }

        Response::from_json(&DeliverResult { stored: true })
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

fn present(value: String) -> Option<String> {
    (!value.is_empty()).then_some(value)
}
