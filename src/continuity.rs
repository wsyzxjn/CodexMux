//! In-memory record of Responses turns, keyed by response id, used to switch
//! a thread to another provider in place.
//!
//! codex sends each turn as "new input + `previous_response_id`"; the full
//! conversation lives server-side at the provider that produced those ids.
//! The store keeps the public part of every turn that transits the forwarder
//! — messages, tool calls and results, compaction output — and never
//! reasoning items, which are provider-private and usually encrypted. When a
//! turn references a response another provider produced, the whole chain is
//! materialized into the request input so the new provider receives the
//! conversation without the provider-specific id. The handoff is lossy by
//! design: the reasoning chain does not travel.
//!
//! Memory only: a daemon restart clears it. A chain the store cannot walk to
//! its first turn is rejected before forwarding so an id is never silently
//! sent to the wrong provider.

use std::{
    collections::{HashMap, HashSet},
    sync::Mutex,
    time::Instant,
};

use serde_json::{Map, Value, json};

/// A turn larger than this keeps only its route identity: same-route
/// follow-ups may still pass the id through, but the chain can no longer be
/// replayed to another route.
const MAX_NODE_BYTES: usize = 4 * 1024 * 1024;
/// Stands in for tool output that has no portable text (an image, for
/// example), so the call it answers is never left without an output.
const OMITTED_TOOL_OUTPUT: &str = "[non-text tool output omitted]";
/// Past this budget whole least-recently-used chains are evicted.
const MAX_TOTAL_BYTES: usize = 64 * 1024 * 1024;

struct Node {
    parent: Option<String>,
    route_id: String,
    /// Topmost recorded ancestor; eviction removes a whole chain at once so a
    /// materialization never sees a hole the store itself created.
    root: String,
    /// `None` for a turn over the per-node budget: routable, not replayable.
    items: Option<Vec<Value>>,
}

struct Chain {
    nodes: Vec<String>,
    bytes: usize,
    last_used: Instant,
}

#[derive(Default)]
struct Inner {
    nodes: HashMap<String, Node>,
    ambiguous_ids: HashSet<String>,
    chains: HashMap<String, Chain>,
    total_bytes: usize,
}

#[derive(Default)]
pub struct ContinuityStore {
    inner: Mutex<Inner>,
}

impl ContinuityStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record one completed turn: the portable input the client sent plus the
    /// portable output the provider returned. A turn over the per-node budget
    /// is kept as a stub that still knows its route but cannot be replayed.
    pub fn record(&self, id: &str, parent: Option<&str>, route_id: &str, items: Vec<Value>) {
        let bytes: usize = items.iter().map(approximate_size).sum();
        let (items, bytes) = if bytes > MAX_NODE_BYTES {
            (None, 0)
        } else {
            (Some(items), bytes)
        };
        let mut inner = self.inner.lock().expect("continuity store mutex poisoned");
        if inner.nodes.contains_key(id) {
            inner.ambiguous_ids.insert(id.to_owned());
            return;
        }
        let root = parent
            .and_then(|parent| inner.nodes.get(parent).map(|node| node.root.clone()))
            .unwrap_or_else(|| id.to_owned());
        inner.nodes.insert(
            id.to_owned(),
            Node {
                parent: parent.map(str::to_owned),
                route_id: route_id.to_owned(),
                root: root.clone(),
                items,
            },
        );
        let chain = inner.chains.entry(root).or_insert_with(|| Chain {
            nodes: Vec::new(),
            bytes: 0,
            last_used: Instant::now(),
        });
        chain.nodes.push(id.to_owned());
        chain.bytes += bytes;
        chain.last_used = Instant::now();
        inner.total_bytes += bytes;
        evict_over_budget(&mut inner);
    }

    /// The full portable history ending at `id` when the chain is complete
    /// and replayable. `None` means the id is unknown, an ancestor is
    /// missing, or a turn in the chain was too large to keep.
    pub fn materialize(&self, id: &str) -> Option<Vec<Value>> {
        let mut inner = self.inner.lock().expect("continuity store mutex poisoned");
        if inner.ambiguous_ids.contains(id) {
            return None;
        }
        let node = inner.nodes.get(id)?;
        let root = node.root.clone();
        let mut path = vec![id.to_owned()];
        let mut current = node;
        while let Some(parent) = current.parent.as_deref() {
            if inner.ambiguous_ids.contains(parent) {
                return None;
            }
            current = inner.nodes.get(parent)?;
            path.push(parent.to_owned());
        }
        let mut items = Vec::new();
        for id in path.iter().rev() {
            items.extend(inner.nodes[id].items.as_ref()?.iter().cloned());
        }
        if let Some(chain) = inner.chains.get_mut(&root) {
            chain.last_used = Instant::now();
        }
        Some(items)
    }

    pub fn route_of(&self, id: &str) -> Option<String> {
        let inner = self.inner.lock().expect("continuity store mutex poisoned");
        if inner.ambiguous_ids.contains(id) {
            return None;
        }
        inner.nodes.get(id).map(|node| node.route_id.clone())
    }

    pub fn is_complete(&self, id: &str) -> bool {
        let inner = self.inner.lock().expect("continuity store mutex poisoned");
        if inner.ambiguous_ids.contains(id) {
            return false;
        }
        let Some(mut node) = inner.nodes.get(id) else {
            return false;
        };
        while let Some(parent) = node.parent.as_deref() {
            if inner.ambiguous_ids.contains(parent) {
                return false;
            }
            let Some(parent_node) = inner.nodes.get(parent) else {
                return false;
            };
            node = parent_node;
        }
        true
    }
}

fn evict_over_budget(inner: &mut Inner) {
    while inner.total_bytes > MAX_TOTAL_BYTES {
        let Some(oldest) = inner
            .chains
            .iter()
            .min_by_key(|(_, chain)| chain.last_used)
            .map(|(root, _)| root.clone())
        else {
            return;
        };
        let Some(chain) = inner.chains.remove(&oldest) else {
            return;
        };
        for id in chain.nodes {
            inner.nodes.remove(&id);
            inner.ambiguous_ids.remove(&id);
        }
        inner.total_bytes = inner.total_bytes.saturating_sub(chain.bytes);
    }
}

/// Normalize a request `input` into portable items: a string becomes a user
/// message, an array is filtered to portable items, anything else is empty.
pub fn portable_input_items(input: Option<&Value>) -> Vec<Value> {
    match input {
        Some(Value::String(text)) => vec![json!({
            "role": "user",
            "content": [{"type": "input_text", "text": text}]
        })],
        Some(Value::Array(items)) => items.iter().filter_map(portable_item).collect(),
        _ => Vec::new(),
    }
}

/// Filter a response `output` array to its portable items.
pub fn portable_output_items(output: Option<&Value>) -> Vec<Value> {
    match output {
        Some(Value::Array(items)) => items.iter().filter_map(portable_item).collect(),
        _ => Vec::new(),
    }
}

/// Keep an item only when its type is portable; strip the provider-assigned
/// `id` and `status` fields so a replay never carries another provider's
/// identifiers.
fn portable_item(item: &Value) -> Option<Value> {
    let object = item.as_object()?;
    let kind = object.get("type").and_then(Value::as_str);
    match kind {
        Some("message") => {
            let mut kept = Map::new();
            kept.insert("type".into(), json!("message"));
            copy_string(&mut kept, object, "role");
            kept.insert("content".into(), public_content(object.get("content"))?);
            Some(Value::Object(kept))
        }
        Some("function_call") => {
            let mut kept = Map::new();
            kept.insert("type".into(), json!("function_call"));
            kept.insert("call_id".into(), required_string(object, "call_id")?);
            kept.insert("name".into(), required_string(object, "name")?);
            kept.insert("arguments".into(), required_string(object, "arguments")?);
            Some(Value::Object(kept))
        }
        Some("custom_tool_call") => {
            let mut kept = Map::new();
            kept.insert("type".into(), json!("custom_tool_call"));
            kept.insert("call_id".into(), required_string(object, "call_id")?);
            kept.insert("name".into(), required_string(object, "name")?);
            kept.insert("input".into(), required_string(object, "input")?);
            Some(Value::Object(kept))
        }
        Some("function_call_output") | Some("custom_tool_call_output") => {
            let mut kept = Map::new();
            kept.insert("type".into(), Value::String(kind?.into()));
            kept.insert("call_id".into(), required_string(object, "call_id")?);
            kept.insert("output".into(), public_tool_output(object.get("output"))?);
            Some(Value::Object(kept))
        }
        Some("compaction") => {
            let mut kept = Map::new();
            kept.insert("type".into(), json!("compaction"));
            if let Some(summary) = public_text_value(object.get("summary")) {
                kept.insert("summary".into(), summary);
            }
            if let Some(content) = public_content(object.get("content")) {
                kept.insert("content".into(), content);
            }
            (kept.len() > 1).then_some(Value::Object(kept))
        }
        None if object.contains_key("role") => {
            let mut kept = Map::new();
            copy_string(&mut kept, object, "role");
            kept.insert("content".into(), public_content(object.get("content"))?);
            Some(Value::Object(kept))
        }
        _ => None,
    }
}

fn copy_string(target: &mut Map<String, Value>, source: &Map<String, Value>, key: &str) {
    if let Some(Value::String(value)) = source.get(key) {
        target.insert(key.into(), Value::String(value.clone()));
    }
}

fn required_string(source: &Map<String, Value>, key: &str) -> Option<Value> {
    source
        .get(key)
        .and_then(Value::as_str)
        .map(|value| Value::String(value.to_owned()))
}

/// Tool output keeps only public text. Output with no public text at all
/// becomes a placeholder rather than disappearing, because dropping it would
/// leave the matching call unanswered and the upstream would reject the
/// replay.
fn public_tool_output(value: Option<&Value>) -> Option<Value> {
    let placeholder = || Value::String(OMITTED_TOOL_OUTPUT.into());
    Some(match value? {
        Value::String(text) => Value::String(text.clone()),
        Value::Array(parts) => {
            let sanitized: Vec<Value> = parts.iter().filter_map(public_content_part).collect();
            if sanitized.is_empty() {
                placeholder()
            } else {
                Value::Array(sanitized)
            }
        }
        _ => placeholder(),
    })
}

/// Public compaction summary text. Only parts whose type is a known public
/// text type are kept, so a provider cannot smuggle reasoning text into a
/// replay by attaching a `text` field to some other part type.
fn public_text_value(value: Option<&Value>) -> Option<Value> {
    match value? {
        Value::String(text) => Some(Value::String(text.clone())),
        Value::Array(parts) => {
            let texts: Vec<Value> = parts
                .iter()
                .filter(|part| {
                    matches!(
                        part.get("type").and_then(Value::as_str),
                        Some("summary_text" | "output_text" | "text" | "input_text")
                    )
                })
                .filter_map(|part| {
                    part.get("text")
                        .and_then(Value::as_str)
                        .map(|text| json!({"type": "summary_text", "text": text}))
                })
                .collect();
            (!texts.is_empty()).then_some(Value::Array(texts))
        }
        _ => None,
    }
}

fn public_content(value: Option<&Value>) -> Option<Value> {
    match value? {
        Value::String(text) => Some(Value::String(text.clone())),
        Value::Array(parts) => {
            let sanitized: Vec<Value> = parts.iter().filter_map(public_content_part).collect();
            (!sanitized.is_empty()).then_some(Value::Array(sanitized))
        }
        _ => None,
    }
}

/// Only text parts are portable. Images and every other part type are
/// dropped on handoff.
fn public_content_part(part: &Value) -> Option<Value> {
    if let Value::String(text) = part {
        return Some(Value::String(text.clone()));
    }
    let object = part.as_object()?;
    let kind = object.get("type").and_then(Value::as_str)?;
    if !matches!(kind, "input_text" | "output_text" | "text") {
        return None;
    }
    let mut kept = Map::new();
    kept.insert("type".into(), Value::String(kind.into()));
    copy_string(&mut kept, object, "text");
    (kept.len() > 1).then_some(Value::Object(kept))
}

/// Drop tool calls without an output and outputs without a call. Replay
/// input must pair every call with its output by `call_id`; a provider
/// rejects either half on its own. Run this on the complete replay list
/// (history plus the current turn), where both halves of a pair are present.
pub fn balance_tool_calls(items: &mut Vec<Value>) {
    fn pair(item: &Value) -> Option<(bool, &str)> {
        let kind = item.get("type").and_then(Value::as_str)?;
        let is_call = match kind {
            "function_call" | "custom_tool_call" => true,
            "function_call_output" | "custom_tool_call_output" => false,
            _ => return None,
        };
        Some((is_call, item.get("call_id").and_then(Value::as_str)?))
    }
    let mut calls = HashSet::new();
    let mut outputs = HashSet::new();
    for item in items.iter() {
        match pair(item) {
            Some((true, call_id)) => {
                calls.insert(call_id.to_owned());
            }
            Some((false, call_id)) => {
                outputs.insert(call_id.to_owned());
            }
            None => {}
        }
    }
    items.retain(|item| match pair(item) {
        Some((true, call_id)) => outputs.contains(call_id),
        Some((false, call_id)) => calls.contains(call_id),
        None => true,
    });
}

fn approximate_size(value: &Value) -> usize {
    serde_json::to_string(value)
        .map(|text| text.len())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(text: &str) -> Value {
        json!({"type": "message", "role": "user", "content": [{"type": "input_text", "text": text}]})
    }

    #[test]
    fn a_complete_chain_materializes() {
        let store = ContinuityStore::new();
        store.record("resp-1", None, "openai", vec![message("first")]);
        store.record("resp-2", Some("resp-1"), "openai", vec![message("second")]);

        let items = store.materialize("resp-2").unwrap();
        assert_eq!(items.len(), 2);
        assert!(items[0].to_string().contains("first"));
        assert!(items[1].to_string().contains("second"));
    }

    #[test]
    fn an_incomplete_or_unknown_chain_is_never_rewritten() {
        let store = ContinuityStore::new();
        // Parent was never seen (daemon restarted mid-thread).
        store.record("resp-2", Some("resp-1"), "openai", vec![message("second")]);
        assert!(store.materialize("resp-2").is_none());
        assert!(!store.is_complete("resp-2"));
        assert!(store.materialize("resp-unknown").is_none());
        assert!(!store.is_complete("resp-unknown"));
    }

    #[test]
    fn a_switch_turn_extends_the_chain_with_its_delta_only() {
        let store = ContinuityStore::new();
        store.record("resp-1", None, "openai", vec![message("first")]);
        // The rewritten turn records its original delta, not the replay.
        store.record("resp-2", Some("resp-1"), "ark", vec![message("second")]);
        let items = store.materialize("resp-2").unwrap();
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn reasoning_and_provider_specific_items_are_not_portable() {
        let input = json!([
            {"type": "reasoning", "encrypted_content": "never-forward"},
            {"type": "local_shell_call", "command": ["ls"]},
            {"type": "message", "role": "assistant", "id": "msg_1", "status": "completed",
             "provider_state": "never-forward", "content": [
                 {"type": "output_text", "text": "answer", "encrypted_content": "never-forward"}
             ]},
            {"type": "function_call", "call_id": "call-1", "name": "inspect", "arguments": "{}",
             "provider_state": "never-forward"},
            {"role": "user", "content": [{"type": "input_text", "text": "shorthand"}],
             "provider_state": "never-forward"}
        ]);
        let items = portable_input_items(Some(&input));
        let text = serde_json::to_string(&items).unwrap();
        assert_eq!(items.len(), 3);
        assert!(!text.contains("never-forward"));
        assert!(!text.contains("provider_state"));
        assert!(!text.contains("local_shell_call"));
        assert!(!text.contains("msg_1"));
        assert!(!text.contains("completed"));
        assert!(text.contains("call-1"));
        assert!(text.contains("shorthand"));
    }

    #[test]
    fn encrypted_compaction_state_is_not_portable() {
        let input = json!([
            {"type": "compaction", "encrypted_content": "never-forward"},
            {"type": "compaction", "encrypted_content": "never-forward", "summary": "public"}
        ]);
        let items = portable_input_items(Some(&input));
        assert_eq!(
            items,
            vec![json!({"type": "compaction", "summary": "public"})]
        );
        assert!(
            !serde_json::to_string(&items)
                .unwrap()
                .contains("never-forward")
        );
    }

    #[test]
    fn duplicate_response_ids_fail_closed() {
        let store = ContinuityStore::new();
        store.record("resp-same", None, "provider-a", vec![message("first")]);
        store.record(
            "resp-child",
            Some("resp-same"),
            "provider-a",
            vec![message("child")],
        );
        store.record("resp-same", None, "provider-b", vec![message("second")]);
        assert!(store.route_of("resp-same").is_none());
        assert!(store.materialize("resp-same").is_none());
        assert!(!store.is_complete("resp-child"));
        assert!(store.materialize("resp-child").is_none());
    }

    #[test]
    fn image_replay_is_dropped_even_for_public_urls() {
        let input = json!([{
            "type": "message",
            "role": "user",
            "content": [{
                "type": "input_image",
                "image_url": "https://example.com/image.png"
            }]
        }]);
        assert!(portable_input_items(Some(&input)).is_empty());
    }

    #[test]
    fn tool_outputs_strip_nested_private_state() {
        let input = json!([{
            "type":"function_call_output", "call_id":"call-1",
            "output":[
                {"type":"input_text","text":"public","encrypted_content":"secret"},
                {"type":"private","signature":"secret"}
            ],
            "provider_state":"secret"
        }]);
        let items = portable_input_items(Some(&input));
        assert_eq!(
            items,
            vec![json!({
                "type":"function_call_output", "call_id":"call-1",
                "output":[{"type":"input_text","text":"public"}]
            })]
        );
        let text = serde_json::to_string(&items).unwrap();
        assert!(!text.contains("secret"));
        assert!(!text.contains("signature"));
    }

    #[test]
    fn image_only_tool_output_keeps_its_call_answered() {
        let input = json!([{
            "type":"function_call_output", "call_id":"call-1",
            "output":[{"type":"input_image","image_url":"https://example.com/a.png"}]
        }, {
            "type":"custom_tool_call_output", "call_id":"call-2", "output":{"unexpected":true}
        }]);
        assert_eq!(
            portable_input_items(Some(&input)),
            vec![
                json!({"type":"function_call_output", "call_id":"call-1", "output":OMITTED_TOOL_OUTPUT}),
                json!({"type":"custom_tool_call_output", "call_id":"call-2", "output":OMITTED_TOOL_OUTPUT}),
            ]
        );
    }

    #[test]
    fn unpaired_tool_items_are_removed_from_a_replay() {
        let mut items = vec![
            json!({"type":"function_call", "call_id":"paired", "name":"a", "arguments":"{}"}),
            json!({"type":"function_call", "call_id":"lonely-call", "name":"b", "arguments":"{}"}),
            message("between"),
            json!({"type":"function_call_output", "call_id":"paired", "output":"ok"}),
            json!({"type":"custom_tool_call_output", "call_id":"lonely-output", "output":"x"}),
        ];
        balance_tool_calls(&mut items);
        let call_ids: Vec<_> = items
            .iter()
            .filter_map(|item| item.get("call_id").and_then(Value::as_str))
            .collect();
        assert_eq!(call_ids, ["paired", "paired"]);
        assert_eq!(items.len(), 3);
    }

    #[test]
    fn oversized_turns_stay_routable_but_are_never_replayed() {
        let store = ContinuityStore::new();
        store.record("resp-1", None, "official:gpt", vec![message("first")]);
        let big = "x".repeat(MAX_NODE_BYTES + 1);
        store.record(
            "resp-2",
            Some("resp-1"),
            "official:gpt",
            vec![message(&big)],
        );
        store.record(
            "resp-3",
            Some("resp-2"),
            "official:gpt",
            vec![message("third")],
        );

        assert_eq!(store.route_of("resp-2").as_deref(), Some("official:gpt"));
        assert!(store.is_complete("resp-3"));
        assert!(store.materialize("resp-2").is_none());
        assert!(store.materialize("resp-3").is_none());
        assert!(store.materialize("resp-1").is_some());
        assert_eq!(
            store.inner.lock().unwrap().total_bytes,
            approximate_size(&message("first")) + approximate_size(&message("third"))
        );
    }

    #[test]
    fn compaction_summary_keeps_only_public_text_types() {
        let input = json!([{
            "type":"compaction",
            "summary":[
                {"type":"summary_text","text":"public"},
                {"type":"reasoning_text","text":"private"}
            ]
        }]);
        assert_eq!(
            portable_input_items(Some(&input)),
            vec![json!({"type":"compaction", "summary":[{"type":"summary_text","text":"public"}]})]
        );
    }

    #[test]
    fn least_recently_used_chains_are_evicted_whole() {
        let store = ContinuityStore::new();
        let big = "x".repeat(MAX_NODE_BYTES - 1024);
        // Each chain is one node just under the per-node cap; 17 of them
        // exceed the 64 MiB budget.
        for index in 0..17 {
            store.record(
                &format!("resp-{index}"),
                None,
                "openai",
                vec![message(&big)],
            );
        }
        let inner = store.inner.lock().unwrap();
        assert!(inner.total_bytes <= MAX_TOTAL_BYTES);
        assert!(!inner.nodes.contains_key("resp-0"));
        assert!(inner.nodes.contains_key("resp-16"));
    }
}
