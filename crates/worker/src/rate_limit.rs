use serde::{Deserialize, Serialize};
use wasm_bindgen::JsValue;
use worker::{DurableObject, Env, Method, Request, RequestInit, Response, Result, State};

use crate::{
    RateLimit,
    support::{RATE_LIMIT_BINDING, rate_limited},
};

pub(crate) const MAILBOX_CREATE_LIMIT: i64 = 10;
pub(crate) const MAILBOX_CREATE_WINDOW_MS: i64 = 60_000;
pub(crate) const MAIL_DELIVER_LIMIT: i64 = 30;
pub(crate) const MAIL_DELIVER_WINDOW_MS: i64 = 60_000;
const RATE_LIMIT_SHARDS: u64 = 16;
const MAX_KEYS_PER_SHARD: usize = 512;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Window {
    pub(crate) started_at_ms: i64,
    pub(crate) count: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Decision {
    pub(crate) allowed: bool,
    pub(crate) window: Window,
    pub(crate) retry_after_seconds: i64,
}

#[derive(Deserialize, Serialize)]
struct CheckRequest {
    key: String,
    limit: i64,
    window_ms: i64,
}

#[derive(Deserialize, Serialize)]
struct CheckResponse {
    allowed: bool,
    retry_after_seconds: i64,
}

impl DurableObject for RateLimit {
    fn new(_state: State, _env: Env) -> Self {
        Self {
            windows: std::cell::RefCell::new(std::collections::HashMap::new()),
            last_pruned_at_ms: std::cell::Cell::new(0),
        }
    }

    async fn fetch(&self, mut req: Request) -> Result<Response> {
        if req.method() != Method::Post {
            return Response::error("Not found", 404);
        }

        let check = req.json::<CheckRequest>().await?;
        if check.limit <= 0 || check.window_ms <= 0 {
            return Response::error("Invalid rate limit", 400);
        }
        let now = now_ms();
        let mut windows = self.windows.borrow_mut();
        if now.saturating_sub(self.last_pruned_at_ms.get()) >= check.window_ms {
            prune_expired(&mut windows, now, check.window_ms);
            self.last_pruned_at_ms.set(now);
        }

        if !windows.contains_key(&check.key) && windows.len() >= MAX_KEYS_PER_SHARD {
            prune_expired(&mut windows, now, check.window_ms);
            self.last_pruned_at_ms.set(now);
            if windows.len() >= MAX_KEYS_PER_SHARD {
                return Response::from_json(&CheckResponse {
                    allowed: false,
                    retry_after_seconds: ((check.window_ms + 999) / 1000).max(1),
                });
            }
        }

        let window = windows.get(&check.key).copied().unwrap_or(Window {
            started_at_ms: 0,
            count: 0,
        });
        let decision = hit_window(window, now, check.limit, check.window_ms);
        windows.insert(check.key, decision.window);

        Response::from_json(&CheckResponse {
            allowed: decision.allowed,
            retry_after_seconds: decision.retry_after_seconds,
        })
    }
}

pub(crate) async fn check_mailbox_create(req: &Request, env: &Env) -> Result<Option<Response>> {
    let client = client_key(req)?;
    let key = format!("mailbox-create:{client}");
    let decision = check(env, &key, MAILBOX_CREATE_LIMIT, MAILBOX_CREATE_WINDOW_MS).await?;
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

pub(crate) fn hit_window(window: Window, now_ms: i64, limit: i64, window_ms: i64) -> Decision {
    let elapsed = now_ms.saturating_sub(window.started_at_ms);
    let expired = window.started_at_ms <= 0 || elapsed >= window_ms;
    let started_at_ms = if expired {
        now_ms
    } else {
        window.started_at_ms
    };
    let count = if expired { 0 } else { window.count };

    if count >= limit {
        let retry_ms = window_ms.saturating_sub(now_ms.saturating_sub(started_at_ms));
        return Decision {
            allowed: false,
            window: Window {
                started_at_ms,
                count,
            },
            retry_after_seconds: ((retry_ms + 999) / 1000).max(1),
        };
    }

    Decision {
        allowed: true,
        window: Window {
            started_at_ms,
            count: count + 1,
        },
        retry_after_seconds: 0,
    }
}

pub(crate) fn now_ms() -> i64 {
    worker::Date::now().as_millis() as i64
}

async fn check(env: &Env, key: &str, limit: i64, window_ms: i64) -> Result<CheckResponse> {
    let namespace = env.durable_object(RATE_LIMIT_BINDING)?;
    let stub = namespace.get_by_name(&shard_name(key))?;
    let mut init = RequestInit::new();
    init.with_method(Method::Post);
    init.with_body(Some(JsValue::from_str(&serde_json::to_string(
        &CheckRequest {
            key: key.to_owned(),
            limit,
            window_ms,
        },
    )?)));
    let request = Request::new_with_init("https://rate-limit.internal/check", &init)?;
    let mut response = stub.fetch_with_request(request).await?;
    if !(200..300).contains(&response.status_code()) {
        return Err(format!("rate limit check failed: HTTP {}", response.status_code()).into());
    }
    response.json::<CheckResponse>().await
}

fn prune_expired(
    windows: &mut std::collections::HashMap<String, Window>,
    now_ms: i64,
    window_ms: i64,
) {
    windows.retain(|_, window| now_ms.saturating_sub(window.started_at_ms) < window_ms);
}

fn shard_name(key: &str) -> String {
    format!("create-{}", stable_hash(key) % RATE_LIMIT_SHARDS)
}

fn stable_hash(value: &str) -> u64 {
    value.bytes().fold(0xcbf29ce484222325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    })
}

fn client_key(req: &Request) -> Result<String> {
    let value = req
        .headers()
        .get("cf-connecting-ip")?
        .or(req.headers().get("x-real-ip")?)
        .or(req.headers().get("x-forwarded-for")?)
        .unwrap_or_else(|| "unknown".to_owned());
    let first = value.split(',').next().unwrap_or("unknown").trim();
    Ok(safe_key_part(first))
}

fn safe_key_part(value: &str) -> String {
    let key: String = value
        .chars()
        .take(96)
        .map(|c| match c {
            c if c.is_ascii_alphanumeric() => c,
            '.' | ':' | '-' | '_' => c,
            _ => '_',
        })
        .collect();
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
    fn fixed_window_allows_until_limit_then_retries() {
        let mut window = Window {
            started_at_ms: 0,
            count: 0,
        };
        for _ in 0..MAILBOX_CREATE_LIMIT {
            let decision = hit_window(window, 1_000, MAILBOX_CREATE_LIMIT, 60_000);
            assert!(decision.allowed);
            window = decision.window;
        }

        let decision = hit_window(window, 2_000, MAILBOX_CREATE_LIMIT, 60_000);
        assert!(!decision.allowed);
        assert_eq!(decision.retry_after_seconds, 59);
    }

    #[test]
    fn fixed_window_resets_after_window() {
        let window = Window {
            started_at_ms: 1_000,
            count: MAILBOX_CREATE_LIMIT,
        };

        let decision = hit_window(window, 61_000, MAILBOX_CREATE_LIMIT, 60_000);

        assert!(decision.allowed);
        assert_eq!(decision.window.count, 1);
        assert_eq!(decision.window.started_at_ms, 61_000);
    }

    #[test]
    fn client_key_part_is_short_and_safe() {
        assert_eq!(safe_key_part("2001:db8::1"), "2001:db8::1");
        assert_eq!(safe_key_part("bad value/../x"), "bad_value_.._x");
        assert_eq!(safe_key_part(""), "unknown");
    }

    #[test]
    fn prunes_expired_windows() {
        let mut windows = std::collections::HashMap::from([
            (
                "fresh".to_owned(),
                Window {
                    started_at_ms: 59_000,
                    count: 1,
                },
            ),
            (
                "old".to_owned(),
                Window {
                    started_at_ms: 1_000,
                    count: 1,
                },
            ),
        ]);

        prune_expired(&mut windows, 61_000, 60_000);

        assert!(windows.contains_key("fresh"));
        assert!(!windows.contains_key("old"));
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
