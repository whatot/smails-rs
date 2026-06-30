use std::{
    cell::Cell,
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
};

use serde::{Deserialize, Serialize};
use wasm_bindgen::JsValue;
use worker::{DurableObject, Env, Method, Request, RequestInit, Response, Result, State};

use crate::{
    RateLimit,
    fixed_window::{Limit, Limiter, Window},
    support::{RATE_LIMIT_BINDING, now_ms, rate_limited},
};

pub(crate) const MAILBOX_CREATE_LIMIT: Limit = Limit {
    max: 5,
    window_ms: 60_000,
};

pub(crate) const MAIL_DELIVER_LIMIT: Limit = Limit {
    max: 5,
    window_ms: 60_000,
};

const RATE_LIMIT_SHARDS: u64 = 16;
const MAX_KEYS_PER_SHARD: usize = 128;

#[derive(Deserialize, Serialize)]
struct CheckRequest {
    key: String,
}

#[derive(Deserialize, Serialize)]
struct CheckResponse {
    allowed: bool,
    retry_after_seconds: i64,
}

impl DurableObject for RateLimit {
    fn new(_state: State, _env: Env) -> Self {
        Self {
            limiter: std::cell::RefCell::new(Limiter::new(
                MAILBOX_CREATE_LIMIT,
                MAX_KEYS_PER_SHARD,
            )),
        }
    }

    async fn fetch(&self, mut req: Request) -> Result<Response> {
        if req.method() != Method::Post {
            return Response::error("Not found", 404);
        }

        let check = req.json::<CheckRequest>().await?;
        let decision = self.limiter.borrow_mut().hit(&check.key, now_ms());

        Response::from_json(&CheckResponse {
            allowed: decision.allowed,
            retry_after_seconds: decision.retry_after_seconds,
        })
    }
}

pub(crate) async fn check_mailbox_create(req: &Request, env: &Env) -> Result<Option<Response>> {
    // Counts are kept in the sharded RateLimit Durable Object memory.
    let client = client_key(req)?;
    let key = format!("mailbox-create:{client}");
    let decision = check(env, &key).await?;
    if decision.allowed {
        Ok(None)
    } else {
        rate_limited(
            "Too many mailbox create requests",
            decision.retry_after_seconds,
        )
        .map(Some)
    }
}

pub(crate) fn check_mail_deliver(started_at_ms: &Cell<i64>, count: &Cell<i64>) -> bool {
    // Counts are kept in the current Mailbox Durable Object memory.
    let decision = Window {
        started_at_ms: started_at_ms.get(),
        count: count.get(),
    }
    .hit(now_ms(), MAIL_DELIVER_LIMIT);
    started_at_ms.set(decision.window.started_at_ms);
    count.set(decision.window.count);
    decision.allowed
}

async fn check(env: &Env, key: &str) -> Result<CheckResponse> {
    let namespace = env.durable_object(RATE_LIMIT_BINDING)?;
    let stub = namespace.get_by_name(&shard_name(key))?;
    let mut init = RequestInit::new();
    init.with_method(Method::Post);
    init.with_body(Some(JsValue::from_str(&serde_json::to_string(
        &CheckRequest {
            key: key.to_owned(),
        },
    )?)));
    let request = Request::new_with_init("https://rate-limit.internal/check", &init)?;
    let mut response = stub.fetch_with_request(request).await?;
    if !(200..300).contains(&response.status_code()) {
        return Err(format!("rate limit check failed: HTTP {}", response.status_code()).into());
    }
    response.json::<CheckResponse>().await
}

fn shard_name(key: &str) -> String {
    let mut hasher = DefaultHasher::new();
    key.hash(&mut hasher);
    format!("create-{}", hasher.finish() % RATE_LIMIT_SHARDS)
}

fn client_key(req: &Request) -> Result<String> {
    let value = req
        .headers()
        .get("cf-connecting-ip")?
        .or(req.headers().get("x-real-ip")?)
        .or(req.headers().get("x-forwarded-for")?)
        .unwrap_or_else(|| "unknown".to_owned());
    let first = value.split(',').next().unwrap_or("unknown").trim();
    Ok(key_part(first))
}

fn key_part(value: &str) -> String {
    let key: String = value.chars().take(96).collect();
    if key.is_empty() {
        "unknown".to_owned()
    } else {
        key
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_part_is_short() {
        assert_eq!(key_part("2001:db8::1"), "2001:db8::1");
        assert_eq!(key_part(""), "unknown");
        assert_eq!(key_part(&"x".repeat(120)).len(), 96);
    }

    #[test]
    fn shard_name_is_bounded_and_stable() {
        assert_eq!(
            shard_name("mailbox-create:127.0.0.1"),
            shard_name("mailbox-create:127.0.0.1")
        );
        assert!(shard_name("mailbox-create:127.0.0.1").starts_with("create-"));
    }
}
