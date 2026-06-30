use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Limit {
    pub(crate) max: i64,
    pub(crate) window_ms: i64,
}

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

pub(crate) struct Limiter {
    limit: Limit,
    max_keys: usize,
    windows: HashMap<String, Window>,
    last_pruned_at_ms: i64,
}

impl Limiter {
    pub(crate) fn new(limit: Limit, max_keys: usize) -> Self {
        Self {
            limit,
            max_keys,
            windows: HashMap::new(),
            last_pruned_at_ms: 0,
        }
    }

    pub(crate) fn hit(&mut self, key: &str, now_ms: i64) -> Decision {
        if now_ms.saturating_sub(self.last_pruned_at_ms) >= self.limit.window_ms {
            self.prune(now_ms);
        }

        if !self.windows.contains_key(key) && self.windows.len() >= self.max_keys {
            self.prune(now_ms);
            if self.windows.len() >= self.max_keys {
                return Decision {
                    allowed: false,
                    window: Window::EMPTY,
                    retry_after_seconds: retry_after_seconds(self.limit.window_ms),
                };
            }
        }

        let decision = self
            .windows
            .get(key)
            .copied()
            .unwrap_or(Window::EMPTY)
            .hit(now_ms, self.limit);
        self.windows.insert(key.to_owned(), decision.window);
        decision
    }

    fn prune(&mut self, now_ms: i64) {
        self.windows
            .retain(|_, window| now_ms.saturating_sub(window.started_at_ms) < self.limit.window_ms);
        self.last_pruned_at_ms = now_ms;
    }
}

impl Window {
    pub(crate) const EMPTY: Self = Self {
        started_at_ms: 0,
        count: 0,
    };

    pub(crate) fn hit(self, now_ms: i64, limit: Limit) -> Decision {
        let elapsed = now_ms.saturating_sub(self.started_at_ms);
        let expired = self.started_at_ms <= 0 || elapsed >= limit.window_ms;
        let started_at_ms = if expired { now_ms } else { self.started_at_ms };
        let count = if expired { 0 } else { self.count };

        if count >= limit.max {
            let retry_ms = limit
                .window_ms
                .saturating_sub(now_ms.saturating_sub(started_at_ms));
            return Decision {
                allowed: false,
                window: Window {
                    started_at_ms,
                    count,
                },
                retry_after_seconds: retry_after_seconds(retry_ms),
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
}

fn retry_after_seconds(ms: i64) -> i64 {
    ((ms + 999) / 1000).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_LIMIT: Limit = Limit {
        max: 5,
        window_ms: 60_000,
    };

    #[test]
    fn allows_until_limit_then_retries() {
        let mut window = Window::EMPTY;
        for _ in 0..TEST_LIMIT.max {
            let decision = window.hit(1_000, TEST_LIMIT);
            assert!(decision.allowed);
            window = decision.window;
        }

        let decision = window.hit(2_000, TEST_LIMIT);

        assert!(!decision.allowed);
        assert_eq!(decision.retry_after_seconds, 59);
    }

    #[test]
    fn resets_after_window() {
        let window = Window {
            started_at_ms: 1_000,
            count: TEST_LIMIT.max,
        };

        let decision = window.hit(61_000, TEST_LIMIT);

        assert!(decision.allowed);
        assert_eq!(decision.window.count, 1);
        assert_eq!(decision.window.started_at_ms, 61_000);
    }

    #[test]
    fn limiter_reuses_expired_capacity() {
        let mut limiter = Limiter::new(TEST_LIMIT, 1);

        assert!(limiter.hit("old", 1_000).allowed);
        assert!(limiter.hit("fresh", 61_000).allowed);
    }

    #[test]
    fn limiter_denies_when_capacity_is_full() {
        let mut limiter = Limiter::new(TEST_LIMIT, 1);

        assert!(limiter.hit("one", 1_000).allowed);
        let decision = limiter.hit("two", 2_000);

        assert!(!decision.allowed);
        assert_eq!(decision.retry_after_seconds, 60);
    }
}
