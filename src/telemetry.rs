use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use serde::Serialize;
use serde_json::Value;

#[derive(Clone, Debug, Serialize)]
pub struct TelemetrySnapshot {
    pub active_requests: usize,
    pub current: Option<TurnSnapshot>,
    pub last_completed: Option<TurnSnapshot>,
}

#[derive(Clone, Debug, Serialize)]
pub struct TurnSnapshot {
    pub route: String,
    pub model: String,
    pub elapsed_ms: u64,
    pub output_tokens: u64,
    pub tokens_per_second: Option<f64>,
    pub exact: bool,
}

struct ActiveTurn {
    route: String,
    model: String,
    started_at: Instant,
    estimated_output_tokens: f64,
}

#[derive(Default)]
struct Inner {
    next_id: u64,
    active: BTreeMap<u64, ActiveTurn>,
    last_completed: Option<TurnSnapshot>,
}

#[derive(Default)]
pub struct TelemetryStore {
    inner: Mutex<Inner>,
}

impl TelemetryStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn begin(self: &Arc<Self>, route_id: &str) -> TelemetrySession {
        let (route, model) = route_parts(route_id);
        let mut inner = self.inner.lock().expect("telemetry store mutex poisoned");
        inner.next_id = inner.next_id.wrapping_add(1);
        let id = inner.next_id;
        inner.active.insert(
            id,
            ActiveTurn {
                route,
                model,
                started_at: Instant::now(),
                estimated_output_tokens: 0.0,
            },
        );
        TelemetrySession {
            store: self.clone(),
            id: Some(id),
        }
    }

    pub fn snapshot(&self) -> TelemetrySnapshot {
        let inner = self.inner.lock().expect("telemetry store mutex poisoned");
        let now = Instant::now();
        TelemetrySnapshot {
            active_requests: inner.active.len(),
            current: inner
                .active
                .iter()
                .next_back()
                .map(|(_, turn)| active_snapshot(turn, now)),
            last_completed: inner.last_completed.clone(),
        }
    }

    fn observe_delta(&self, id: u64, delta: &str) {
        let mut inner = self.inner.lock().expect("telemetry store mutex poisoned");
        if let Some(turn) = inner.active.get_mut(&id) {
            turn.estimated_output_tokens += estimate_delta_tokens(delta);
        }
    }

    fn complete(&self, id: u64, response: &Value) {
        let mut inner = self.inner.lock().expect("telemetry store mutex poisoned");
        let Some(turn) = inner.active.remove(&id) else {
            return;
        };
        let elapsed = turn.started_at.elapsed();
        let exact_tokens = response
            .get("usage")
            .and_then(|usage| usage.get("output_tokens"))
            .and_then(Value::as_u64);
        let output_tokens =
            exact_tokens.unwrap_or_else(|| turn.estimated_output_tokens.round().max(0.0) as u64);
        inner.last_completed = Some(TurnSnapshot {
            route: turn.route,
            model: turn.model,
            elapsed_ms: duration_ms(elapsed),
            output_tokens,
            tokens_per_second: rate(output_tokens as f64, elapsed),
            exact: exact_tokens.is_some(),
        });
    }

    fn cancel(&self, id: u64) {
        self.inner
            .lock()
            .expect("telemetry store mutex poisoned")
            .active
            .remove(&id);
    }
}

pub struct TelemetrySession {
    store: Arc<TelemetryStore>,
    id: Option<u64>,
}

impl TelemetrySession {
    pub fn observe_delta(&self, delta: &str) {
        if let Some(id) = self.id {
            self.store.observe_delta(id, delta);
        }
    }

    pub fn complete(&mut self, response: &Value) {
        if let Some(id) = self.id.take() {
            self.store.complete(id, response);
        }
    }
}

impl Drop for TelemetrySession {
    fn drop(&mut self) {
        if let Some(id) = self.id.take() {
            self.store.cancel(id);
        }
    }
}

fn active_snapshot(turn: &ActiveTurn, now: Instant) -> TurnSnapshot {
    let elapsed = now.saturating_duration_since(turn.started_at);
    TurnSnapshot {
        route: turn.route.clone(),
        model: turn.model.clone(),
        elapsed_ms: duration_ms(elapsed),
        output_tokens: turn.estimated_output_tokens.round().max(0.0) as u64,
        tokens_per_second: rate(turn.estimated_output_tokens, elapsed),
        exact: false,
    }
}

fn route_parts(route_id: &str) -> (String, String) {
    route_id
        .split_once(':')
        .map(|(route, model)| (route.to_owned(), model.to_owned()))
        .unwrap_or_else(|| ("unknown".into(), route_id.to_owned()))
}

fn duration_ms(duration: Duration) -> u64 {
    duration.as_millis().min(u64::MAX as u128) as u64
}

fn rate(tokens: f64, elapsed: Duration) -> Option<f64> {
    let seconds = elapsed.as_secs_f64();
    (tokens >= 1.0 && seconds >= 0.25).then_some(tokens / seconds)
}

/// A deliberately lightweight estimate for live display. Final provider usage
/// replaces it when available. ASCII prose is roughly four bytes per token;
/// punctuation and multibyte characters receive a little more weight so code
/// and CJK text are not severely under-counted.
fn estimate_delta_tokens(delta: &str) -> f64 {
    delta.chars().fold(0.0, |tokens, character| {
        tokens
            + if character.is_ascii_alphanumeric() || character.is_ascii_whitespace() {
                0.25
            } else if character.is_ascii() {
                0.5
            } else {
                character.len_utf8() as f64 / 4.0
            }
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn live_snapshot_is_approximate_and_completion_uses_provider_usage() {
        let store = Arc::new(TelemetryStore::new());
        let mut session = store.begin("cpa:cpa/gpt-test");
        session.observe_delta("hello, 世界");
        {
            let mut inner = store.inner.lock().unwrap();
            let turn = inner.active.get_mut(&session.id.unwrap()).unwrap();
            turn.started_at = Instant::now() - Duration::from_secs(2);
        }

        let live = store.snapshot();
        assert_eq!(live.active_requests, 1);
        let current = live.current.unwrap();
        assert_eq!(current.route, "cpa");
        assert_eq!(current.model, "cpa/gpt-test");
        assert!(!current.exact);
        assert!(current.output_tokens > 0);
        assert!(current.tokens_per_second.is_some());

        session.complete(&json!({"usage":{"output_tokens":20}}));
        let completed = store.snapshot();
        assert_eq!(completed.active_requests, 0);
        let last = completed.last_completed.unwrap();
        assert_eq!(last.output_tokens, 20);
        assert!(last.exact);
        assert!((9.0..=10.1).contains(&last.tokens_per_second.unwrap()));
    }

    #[test]
    fn dropping_a_session_removes_it_from_active_requests() {
        let store = Arc::new(TelemetryStore::new());
        let session = store.begin("official:gpt-test");
        assert_eq!(store.snapshot().active_requests, 1);
        drop(session);
        assert_eq!(store.snapshot().active_requests, 0);
        assert!(store.snapshot().last_completed.is_none());
    }
}
