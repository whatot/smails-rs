use serde::{Deserialize, Serialize};
use wasm_bindgen::JsValue;
use worker::{
    DurableObject, Env, Method, Request, RequestInit, Response, Result, SqlStorageValue, State,
};

use crate::{
    Admin,
    admin_schema::init_schema,
    support::{ADMIN_BINDING, ADMIN_TOKEN, constant_time_eq, json_error},
};

const ADMIN_INSTANCE: &str = "global";
const COUNTER_MAILBOXES_CREATED: &str = "total_mailboxes_created";

#[derive(Deserialize, Serialize)]
struct CounterRecord {
    name: String,
}

#[derive(Deserialize)]
struct CountRow {
    value: i64,
}

#[derive(Serialize)]
struct AdminStats {
    total_mailboxes_created: i64,
}

impl DurableObject for Admin {
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

        match (req.method(), req.path().as_str()) {
            (Method::Post, "/counter") => self.increment_counter(req).await,
            (Method::Get, "/admin/stats") => self.stats(),
            _ => json_error("Not found", 404),
        }
    }
}

impl Admin {
    fn ensure_schema(&self) -> Result<Option<Response>> {
        if self.schema_ready.get() {
            return Ok(None);
        }
        let sql = self.state.storage().sql();
        if init_schema(&sql).is_err() {
            return json_error("Admin schema migration failed", 500).map(Some);
        }
        self.schema_ready.set(true);
        Ok(None)
    }

    async fn increment_counter(&self, mut req: Request) -> Result<Response> {
        let body = req.json::<CounterRecord>().await?;
        self.bump(&body.name)?;
        Response::empty()
    }

    fn stats(&self) -> Result<Response> {
        Response::from_json(&AdminStats {
            total_mailboxes_created: self.counter(COUNTER_MAILBOXES_CREATED)?,
        })
    }

    fn bump(&self, name: &str) -> Result<()> {
        self.state.storage().sql().exec(
            "INSERT INTO counters (name, value)
             VALUES (?, 1)
             ON CONFLICT(name) DO UPDATE SET value = value + 1",
            vec![name.into()],
        )?;
        Ok(())
    }

    fn counter(&self, name: &str) -> Result<i64> {
        self.count(
            "SELECT COALESCE((SELECT value FROM counters WHERE name = ?), 0) AS value",
            vec![name.into()],
        )
    }

    fn count(&self, sql: &str, params: Vec<SqlStorageValue>) -> Result<i64> {
        let rows = self
            .state
            .storage()
            .sql()
            .exec(sql, params)?
            .to_array::<CountRow>()?;
        Ok(rows.first().map(|row| row.value).unwrap_or_default())
    }
}

pub(crate) async fn record_mailbox_created(env: &Env) -> Result<()> {
    increment(env, COUNTER_MAILBOXES_CREATED).await
}

pub(crate) async fn handle_fetch(req: Request, env: &Env) -> Result<Response> {
    if !is_authorized(&req, env) {
        return json_error("Unauthorized", 401);
    }
    admin_stub(env)?.fetch_with_request(req).await
}

async fn increment(env: &Env, name: &str) -> Result<()> {
    post(
        env,
        "/counter",
        &CounterRecord {
            name: name.to_owned(),
        },
    )
    .await
}

async fn post<T: Serialize>(env: &Env, path: &str, body: &T) -> Result<()> {
    let mut init = RequestInit::new();
    init.with_method(Method::Post);
    init.with_body(Some(JsValue::from_str(&serde_json::to_string(body)?)));
    let req = Request::new_with_init(&format!("https://admin.internal{path}"), &init)?;
    let response = admin_stub(env)?.fetch_with_request(req).await?;
    let status = response.status_code();
    if !(200..300).contains(&status) {
        return Err(format!("admin request failed: HTTP {status}").into());
    }
    Ok(())
}

fn admin_stub(env: &Env) -> Result<worker::Stub> {
    env.durable_object(ADMIN_BINDING)?
        .get_by_name(ADMIN_INSTANCE)
}

fn is_authorized(req: &Request, env: &Env) -> bool {
    let Ok(expected) = env.secret(ADMIN_TOKEN) else {
        return false;
    };
    req.headers()
        .get("authorization")
        .ok()
        .flatten()
        .and_then(|value| value.strip_prefix("Bearer ").map(str::to_owned))
        .is_some_and(|actual| constant_time_eq(&expected.to_string(), &actual))
}
