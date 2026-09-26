//! Request diagnostics for forwarded Responses turns.

use std::{collections::BTreeMap, fmt};

use axum::http::StatusCode;
use serde_json::Value;
use tokio::time::Instant;

use super::{
    ProxyError,
    responses::{Endpoint, ResponseRequest, Target},
};

/// Shape of a Responses request for diagnostics. It records sizes, counts,
/// and key names only: never message text, tool payloads, or credentials.
pub(super) struct RequestLogSummary {
    model: String,
    stream: bool,
    has_parent: bool,
    request_bytes: usize,
    input_items: usize,
    input_types: TypeCounts,
    top_level_keys: Vec<String>,
    tools_count: usize,
    instructions_bytes: Option<usize>,
    has_metadata: bool,
    store: Option<bool>,
    tool_choice: Option<String>,
    include: Option<Vec<String>>,
    text_keys: Vec<String>,
    prompt_cache_key_bytes: Option<usize>,
    client_metadata_keys: Vec<String>,
    reasoning_keys: Vec<String>,
    reasoning_effort: Option<String>,
}

/// Input items counted by type, rendered as `function_call:2,message:3`.
/// A long conversation repeats a handful of types, so counting keeps the log
/// line short no matter how much history the client sends.
struct TypeCounts(BTreeMap<String, usize>);

impl TypeCounts {
    fn of(input: Option<&Value>) -> (usize, Self) {
        let mut counts = BTreeMap::new();
        let items: &[Value] = match input {
            Some(Value::Array(items)) => items,
            Some(Value::String(_)) => {
                counts.insert("message".to_owned(), 1);
                return (1, Self(counts));
            }
            _ => &[],
        };
        for item in items {
            let kind = match item.get("type").and_then(Value::as_str) {
                Some(kind) => kind,
                None if item.get("role").is_some() => "message",
                None => "unknown",
            };
            *counts.entry(kind.to_owned()).or_insert(0) += 1;
        }
        (items.len(), Self(counts))
    }
}

impl fmt::Display for TypeCounts {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, (kind, count)) in self.0.iter().enumerate() {
            if index > 0 {
                formatter.write_str(",")?;
            }
            write!(formatter, "{kind}:{count}")?;
        }
        Ok(())
    }
}

fn object_keys(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_object)
        .map(|object| object.keys().cloned().collect())
        .unwrap_or_default()
}

impl RequestLogSummary {
    pub(super) fn of(request: &ResponseRequest) -> Self {
        let object = request.object();
        let (input_items, input_types) = TypeCounts::of(object.get("input"));
        let reasoning = object.get("reasoning");
        Self {
            model: request.model().to_owned(),
            stream: object
                .get("stream")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            has_parent: request.has_parent(),
            request_bytes: request.body_len(),
            input_items,
            input_types,
            top_level_keys: object.keys().cloned().collect(),
            tools_count: object
                .get("tools")
                .and_then(Value::as_array)
                .map_or(0, Vec::len),
            instructions_bytes: object
                .get("instructions")
                .and_then(Value::as_str)
                .map(str::len),
            has_metadata: object.get("metadata").is_some_and(|value| !value.is_null()),
            store: object.get("store").and_then(Value::as_bool),
            tool_choice: object
                .get("tool_choice")
                .and_then(Value::as_str)
                .map(str::to_owned),
            include: object
                .get("include")
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect()
                }),
            text_keys: object_keys(object.get("text")),
            prompt_cache_key_bytes: object
                .get("prompt_cache_key")
                .and_then(Value::as_str)
                .map(str::len),
            client_metadata_keys: object_keys(object.get("client_metadata")),
            reasoning_keys: object_keys(reasoning),
            reasoning_effort: reasoning
                .and_then(|reasoning| reasoning.get("effort"))
                .and_then(Value::as_str)
                .map(str::to_owned),
        }
    }

    pub(super) fn log_success(
        &self,
        endpoint: Endpoint,
        target: &Target,
        started: Instant,
        status: StatusCode,
    ) {
        tracing::info!(
            model = %self.model,
            upstream_model = ?target.upstream_model(),
            route = target.route().name(),
            endpoint = endpoint.path(),
            status = status.as_u16(),
            stream = self.stream,
            has_parent = self.has_parent,
            input_items = self.input_items,
            input_types = %self.input_types,
            top_level_keys = ?self.top_level_keys,
            request_bytes = self.request_bytes,
            tools_count = self.tools_count,
            instructions_bytes = ?self.instructions_bytes,
            has_metadata = self.has_metadata,
            store = ?self.store,
            tool_choice = ?self.tool_choice,
            include = ?self.include,
            text_keys = ?self.text_keys,
            prompt_cache_key_bytes = ?self.prompt_cache_key_bytes,
            client_metadata_keys = ?self.client_metadata_keys,
            reasoning_keys = ?self.reasoning_keys,
            reasoning_effort = ?self.reasoning_effort,
            latency_ms = started.elapsed().as_millis() as u64,
            "response forwarded"
        );
    }

    pub(super) fn log_failure(
        &self,
        endpoint: Endpoint,
        target: Option<&Target>,
        started: Instant,
        error: &ProxyError,
    ) {
        tracing::warn!(
            model = %self.model,
            upstream_model = ?target.and_then(Target::upstream_model),
            route = target.map(|target| target.route().name()),
            endpoint = endpoint.path(),
            status = error.status.as_u16(),
            error_code = error.code,
            error = %error.message,
            stream = self.stream,
            has_parent = self.has_parent,
            input_items = self.input_items,
            input_types = %self.input_types,
            top_level_keys = ?self.top_level_keys,
            request_bytes = self.request_bytes,
            tools_count = self.tools_count,
            instructions_bytes = ?self.instructions_bytes,
            has_metadata = self.has_metadata,
            store = ?self.store,
            tool_choice = ?self.tool_choice,
            include = ?self.include,
            text_keys = ?self.text_keys,
            prompt_cache_key_bytes = ?self.prompt_cache_key_bytes,
            client_metadata_keys = ?self.client_metadata_keys,
            reasoning_keys = ?self.reasoning_keys,
            reasoning_effort = ?self.reasoning_effort,
            latency_ms = started.elapsed().as_millis() as u64,
            "response forwarding failed"
        );
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// A long history logs one count per item type, not one entry per item.
    #[test]
    fn input_types_are_counted_per_type() {
        let turn = json!([
            {"type": "message", "role": "user", "content": "hi"},
            {"type": "function_call", "call_id": "c", "name": "n", "arguments": "{}"},
            {"type": "function_call_output", "call_id": "c", "output": "ok"}
        ]);
        let history: Vec<Value> = std::iter::repeat_n(turn.as_array().unwrap().clone(), 500)
            .flatten()
            .chain([json!({"role": "user", "content": "shorthand"}), json!(42)])
            .collect();
        let (items, types) = TypeCounts::of(Some(&Value::Array(history)));
        assert_eq!(items, 1502);
        assert_eq!(
            types.to_string(),
            "function_call:500,function_call_output:500,message:501,unknown:1"
        );
        let (items, types) = TypeCounts::of(Some(&json!("plain")));
        assert_eq!((items, types.to_string().as_str()), (1, "message:1"));
    }
}
