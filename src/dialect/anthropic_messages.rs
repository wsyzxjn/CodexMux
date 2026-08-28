//! Translation between the Responses API codex speaks and the Anthropic-style
//! Messages API a provider may speak instead.
//!
//! codex always sends a Responses request and always reads Responses events, so
//! a `ProviderProtocol::Messages` provider is translated here, inside the
//! forwarder: [`request_from_responses`] rewrites the outgoing body and
//! [`MessagesTranslator`] rewrites the streamed response events. Continuity has
//! already replayed a thread's history into `input` before this runs, so the
//! translated request carries the whole conversation the stateless Messages API
//! needs. Ported from the reviewed TypeScript adapter that proved the mapping.

use std::collections::HashMap;

use anyhow::{Result, bail};
use serde_json::{Map, Value, json};

/// The Messages API refuses a request without `max_tokens`. When the Responses
/// request does not bound the output, this ceiling is sent instead of failing
/// the turn.
const DEFAULT_MAX_TOKENS: i64 = 4096;

/// Translate one Responses request body into a Messages request body.
///
/// Responses-only state (`previous_response_id`, `store`, `include`,
/// `reasoning`, `text`, `metadata`, `parallel_tool_calls`) is dropped:
/// continuity replays history as input items before this runs, so the
/// translated request already carries the whole conversation it needs.
pub fn request_from_responses(request: &Value) -> Result<Value> {
    let source = request
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("Responses request must be a JSON object"))?;

    let mut messages: Vec<Value> = Vec::new();
    match source.get("input") {
        Some(Value::String(text)) => {
            append_message(&mut messages, &Value::String(text.clone()))?;
        }
        Some(Value::Array(items)) => {
            for item in items {
                append_message(&mut messages, item)?;
            }
        }
        None | Some(Value::Null) => {}
        Some(_) => bail!("Responses input must be a string or an array of items"),
    }
    if messages.is_empty() {
        bail!("Responses request has no input to send");
    }

    let mut body = Map::new();
    if let Some(model) = source.get("model") {
        body.insert("model".into(), model.clone());
    }
    // Messages requires `max_tokens` on every request; see DEFAULT_MAX_TOKENS.
    let max_tokens = match source.get("max_output_tokens") {
        Some(value) if !value.is_null() => value.clone(),
        _ => json!(DEFAULT_MAX_TOKENS),
    };
    body.insert("max_tokens".into(), max_tokens);
    if let Some(Value::String(instructions)) = source.get("instructions") {
        if !instructions.is_empty() {
            body.insert("system".into(), Value::String(instructions.clone()));
        }
    }
    // Consecutive same-role messages are left as they arrived: merging them is a
    // policy codex did not ask for, so alternation is the provider's problem.
    body.insert("messages".into(), Value::Array(messages));
    if let Some(stream) = source.get("stream") {
        if !stream.is_null() {
            body.insert("stream".into(), stream.clone());
        }
    }
    for name in ["temperature", "top_p"] {
        if let Some(value) = source.get(name) {
            if !value.is_null() {
                body.insert(name.into(), value.clone());
            }
        }
    }
    if let Some(Value::Array(tools)) = source.get("tools") {
        let translated: Vec<Value> = tools.iter().filter_map(tool_from_responses).collect();
        if !translated.is_empty() {
            body.insert("tools".into(), Value::Array(translated));
        }
    }
    if let Some(choice) = tool_choice_from_responses(source.get("tool_choice")) {
        body.insert("tool_choice".into(), choice);
    }
    Ok(Value::Object(body))
}

/// The Messages endpoint path joined onto a provider's base URL, mirroring the
/// `responses` route convention (base URL ends `/v1`).
pub const MESSAGES_PATH: &str = "messages";

/// Convert one non-streaming Messages response into a Responses response.
pub fn response_from_messages(body: &Value) -> Result<Value> {
    let id = body.get("id").and_then(Value::as_str).unwrap_or("modelmux");
    let model = body.get("model").and_then(Value::as_str).unwrap_or("");
    let mut output = Vec::new();
    for (index, block) in body
        .get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
    {
        match block.get("type").and_then(Value::as_str) {
            Some("text") => {
                let text = block.get("text").and_then(Value::as_str).unwrap_or("");
                if !text.is_empty() {
                    output.push(json!({
                        "id": format!("msg_{id}_{index}"),
                        "type": "message",
                        "status": "completed",
                        "role": "assistant",
                        "content": [{"type": "output_text", "text": text, "annotations": []}],
                    }));
                }
            }
            Some("tool_use") => output.push(json!({
                "id": format!("fc_{id}_{index}"),
                "type": "function_call",
                "status": "completed",
                "call_id": block.get("id").and_then(Value::as_str).unwrap_or(""),
                "name": block.get("name").and_then(Value::as_str).unwrap_or(""),
                "arguments": serde_json::to_string(block.get("input").unwrap_or(&json!({})))?,
            })),
            Some("thinking") => {}
            _ => {}
        }
    }
    let input_tokens = body
        .pointer("/usage/input_tokens")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let output_tokens = body
        .pointer("/usage/output_tokens")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let response_id = if id.starts_with("resp_") {
        id.to_owned()
    } else {
        format!("resp_{id}")
    };
    Ok(json!({
        "id": response_id,
        "object": "response",
        "created_at": 0,
        "status": if body.get("stop_reason").and_then(Value::as_str) == Some("max_tokens") { "incomplete" } else { "completed" },
        "error": Value::Null,
        "incomplete_details": Value::Null,
        "model": model,
        "output": output,
        "parallel_tool_calls": true,
        "previous_response_id": Value::Null,
        "store": false,
        "tool_choice": "auto",
        "tools": [],
        "usage": {
            "input_tokens": input_tokens,
            "input_tokens_details": {"cached_tokens": body.pointer("/usage/cache_read_input_tokens").and_then(Value::as_i64).unwrap_or(0)},
            "output_tokens": output_tokens,
            "output_tokens_details": {"reasoning_tokens": 0},
            "total_tokens": input_tokens + output_tokens,
        },
        "metadata": {},
    }))
}

fn append_message(messages: &mut Vec<Value>, item: &Value) -> Result<()> {
    if let Value::String(text) = item {
        if !text.is_empty() {
            messages.push(text_message("user", text));
        }
        return Ok(());
    }
    let object = item
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("Responses input item must be a string or an object"))?;
    match object.get("type").and_then(Value::as_str) {
        Some("reasoning") => return Ok(()),
        Some("function_call") | Some("custom_tool_call") => {
            messages.push(json!({
                "role": "assistant",
                "content": [{
                    "type": "tool_use",
                    "id": object.get("call_id").and_then(Value::as_str).unwrap_or(""),
                    "name": object.get("name").and_then(Value::as_str).unwrap_or(""),
                    "input": tool_input(object.get("arguments")),
                }],
            }));
        }
        Some("function_call_output") | Some("custom_tool_call_output") => {
            messages.push(json!({
                "role": "user",
                "content": [{
                    "type": "tool_result",
                    "tool_use_id": object.get("call_id").and_then(Value::as_str).unwrap_or(""),
                    "content": tool_output_text(object.get("output")),
                }],
            }));
        }
        _ => {
            // Messages knows only `user` and `assistant`. A `system`-role item is
            // a conversation item codex replayed, not the instructions, so it
            // stays in the message list as user text instead of merging into
            // `system`.
            let role = if object.get("role").and_then(Value::as_str) == Some("assistant") {
                "assistant"
            } else {
                "user"
            };
            let content = message_content(object.get("content").or_else(|| object.get("summary")))?;
            if content.is_empty() {
                return Ok(());
            }
            messages.push(json!({"role": role, "content": content}));
        }
    }
    Ok(())
}

fn text_message(role: &str, text: &str) -> Value {
    json!({ "role": role, "content": [{ "type": "text", "text": text }] })
}

fn message_content(content: Option<&Value>) -> Result<Vec<Value>> {
    match content {
        Some(Value::String(text)) if !text.is_empty() => {
            Ok(vec![json!({"type": "text", "text": text})])
        }
        Some(Value::Array(parts)) => {
            let mut output = Vec::new();
            for part in parts {
                if let Some(part_text) = part.get("text").and_then(Value::as_str) {
                    if !part_text.is_empty() {
                        output.push(json!({"type": "text", "text": part_text}));
                    }
                } else if matches!(
                    part.get("type").and_then(Value::as_str),
                    Some("input_image" | "image_url")
                ) {
                    let url = part
                        .get("image_url")
                        .or_else(|| part.get("url"))
                        .and_then(Value::as_str)
                        .ok_or_else(|| anyhow::anyhow!("Responses image input has no URL"))?;
                    output.push(json!({"type": "image", "source": image_source(url)?}));
                } else if let Value::String(raw) = part {
                    if !raw.is_empty() {
                        output.push(json!({"type": "text", "text": raw}));
                    }
                }
            }
            Ok(output)
        }
        _ => Ok(Vec::new()),
    }
}

fn image_source(url: &str) -> Result<Value> {
    if let Some(data_url) = url.strip_prefix("data:") {
        let (metadata, data) = data_url
            .split_once(',')
            .ok_or_else(|| anyhow::anyhow!("invalid image data URL"))?;
        let media_type = metadata
            .strip_suffix(";base64")
            .filter(|media_type| media_type.starts_with("image/"))
            .ok_or_else(|| anyhow::anyhow!("image data URL must contain base64 image data"))?;
        return Ok(json!({"type": "base64", "media_type": media_type, "data": data}));
    }
    Ok(json!({"type": "url", "url": url}))
}

/// `tool_use.input` is the parsed arguments object; broken JSON becomes `{}`.
fn tool_input(value: Option<&Value>) -> Value {
    let Some(Value::String(text)) = value else {
        return json!({});
    };
    if text.is_empty() {
        return json!({});
    }
    match serde_json::from_str::<Value>(text) {
        Ok(Value::Object(object)) => Value::Object(object),
        _ => json!({}),
    }
}

fn tool_output_text(output: Option<&Value>) -> String {
    match output {
        None => String::new(),
        Some(Value::String(text)) => text.clone(),
        Some(value) => value.to_string(),
    }
}

fn tool_from_responses(tool: &Value) -> Option<Value> {
    if tool.get("type").and_then(Value::as_str) != Some("function") {
        return None;
    }
    // Responses declares a function inline; the nested chat-completions form is
    // accepted too.
    let source = tool.get("function").unwrap_or(tool);
    let name = source.get("name")?;
    let mut declaration = Map::new();
    declaration.insert("name".into(), name.clone());
    if let Some(description) = source.get("description") {
        if !description.is_null() {
            declaration.insert("description".into(), description.clone());
        }
    }
    let input_schema = match source.get("parameters") {
        Some(parameters) if !parameters.is_null() => parameters.clone(),
        _ => json!({ "type": "object", "properties": {} }),
    };
    declaration.insert("input_schema".into(), input_schema);
    Some(Value::Object(declaration))
}

fn tool_choice_from_responses(choice: Option<&Value>) -> Option<Value> {
    let choice = choice.filter(|value| !value.is_null())?;
    if let Some(name) = choice.get("name").and_then(Value::as_str) {
        return Some(json!({ "type": "tool", "name": name }));
    }
    match choice.as_str() {
        Some("auto") => Some(json!({ "type": "auto" })),
        Some("required") => Some(json!({ "type": "any" })),
        // `"none"` (and anything unrecognized) sends no `tool_choice`: the tools
        // stay declared — dropping them would erase declarations codex replays.
        _ => None,
    }
}

enum BlockKind {
    Text {
        text: String,
    },
    ToolUse {
        call_id: String,
        name: String,
        arguments: String,
    },
}

struct Block {
    output_index: usize,
    item_id: String,
    closed: bool,
    kind: BlockKind,
}

/// Reassembles a Messages SSE stream into Responses events.
///
/// codex reads one Responses event stream, so the translator holds the item
/// ids and accumulated text between events. [`push`](Self::push) returns the
/// events one upstream chunk produced; [`finish`](Self::finish) closes the
/// response. Event field names, id prefixes, and the response shape match what
/// the forwarder's native path emits so codex sees a single dialect.
pub struct MessagesTranslator {
    fallback_model: String,
    buffer: Vec<u8>,
    response_id: Option<String>,
    model: Option<String>,
    input_tokens: Option<i64>,
    output_tokens: Option<i64>,
    stop_reason: Option<String>,
    sequence: u64,
    started: bool,
    completed: bool,
    /// Anthropic `index` → position in `order`.
    index_of: HashMap<i64, usize>,
    /// Blocks in arrival order; the position is the Responses `output_index`.
    order: Vec<Block>,
}

impl MessagesTranslator {
    pub fn new(fallback_model: impl Into<String>) -> Self {
        Self {
            fallback_model: fallback_model.into(),
            buffer: Vec::new(),
            response_id: None,
            model: None,
            input_tokens: None,
            output_tokens: None,
            stop_reason: None,
            sequence: 0,
            started: false,
            completed: false,
            index_of: HashMap::new(),
            order: Vec::new(),
        }
    }

    /// Translate a chunk of the upstream Messages SSE body into Responses
    /// events.
    pub fn push(&mut self, chunk: &[u8]) -> Vec<Value> {
        self.buffer.extend_from_slice(chunk);
        let mut events = Vec::new();
        while let Some(end) = frame_end(&self.buffer) {
            let frame: Vec<u8> = self.buffer.drain(..end).collect();
            if let Some(payload) = parse_frame(&frame) {
                events.extend(self.push_event(&payload));
            }
        }
        events
    }

    /// Flush a trailing frame that never ended in a blank line, then close the
    /// response.
    pub fn finish(&mut self) -> Vec<Value> {
        if self.buffer.is_empty() {
            return Vec::new();
        }
        let frame: Vec<u8> = std::mem::take(&mut self.buffer);
        parse_frame(&frame)
            .map(|payload| self.push_event(&payload))
            .unwrap_or_default()
    }

    fn push_event(&mut self, payload: &Value) -> Vec<Value> {
        // An `error` event or `message_stop` already completed the response;
        // codex must not see events after the terminal one.
        if self.completed {
            return Vec::new();
        }
        match payload.get("type").and_then(Value::as_str) {
            Some("ping") => Vec::new(),
            Some("error") => {
                self.completed = true;
                let error = payload.get("error");
                vec![self.event(
                    "error",
                    json!({
                        "code": error.and_then(|e| e.get("type")).and_then(Value::as_str)
                            .unwrap_or("upstream_stream"),
                        "message": error.and_then(|e| e.get("message")).and_then(Value::as_str)
                            .unwrap_or("provider reported an error"),
                        "param": Value::Null,
                    }),
                )]
            }
            Some("message_start") => self.push_message_start(payload.get("message")),
            Some("content_block_start") => self.push_block_start(
                integer_field(payload, "index"),
                payload.get("content_block"),
            ),
            Some("content_block_delta") => {
                self.push_block_delta(integer_field(payload, "index"), payload.get("delta"))
            }
            Some("content_block_stop") => match integer_field(payload, "index") {
                Some(index) => self.close_block_at(index),
                None => Vec::new(),
            },
            Some("message_delta") => {
                self.stop_reason = payload
                    .pointer("/delta/stop_reason")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                if let Some(output) = integer_field(
                    payload.get("usage").unwrap_or(&Value::Null),
                    "output_tokens",
                ) {
                    self.output_tokens = Some(output);
                }
                Vec::new()
            }
            Some("message_stop") => self.finish_response(),
            // An event name the translator does not know is skipped, not fatal.
            _ => Vec::new(),
        }
    }

    fn finish_response(&mut self) -> Vec<Value> {
        if self.completed {
            return Vec::new();
        }
        self.completed = true;
        let mut events = self.close_open_blocks();
        let output = self.output_items();
        let response = self.response_value(output);
        let terminal = if self.stop_reason.as_deref() == Some("max_tokens") {
            "response.incomplete"
        } else {
            "response.completed"
        };
        events.push(self.event(terminal, json!({ "response": response })));
        events
    }

    fn push_message_start(&mut self, message: Option<&Value>) -> Vec<Value> {
        if self.response_id.is_none() {
            self.response_id = message
                .and_then(|m| m.get("id"))
                .and_then(Value::as_str)
                .map(str::to_owned);
            self.model = message
                .and_then(|m| m.get("model"))
                .and_then(Value::as_str)
                .map(str::to_owned);
        }
        if let Some(input) = integer_field(
            message.and_then(|m| m.get("usage")).unwrap_or(&Value::Null),
            "input_tokens",
        ) {
            self.input_tokens = Some(input);
        }
        self.start()
    }

    fn push_block_start(&mut self, index: Option<i64>, block_value: Option<&Value>) -> Vec<Value> {
        let mut events = self.start();
        let key = index.unwrap_or(self.order.len() as i64);
        // A repeated index is a broken stream; the first block keeps its state.
        if self.index_of.contains_key(&key) {
            return events;
        }
        // Anthropic stops a block before starting the next, but a missing
        // content_block_stop must not leave two output items open at once.
        events.extend(self.close_open_blocks());

        let output_index = self.order.len();
        let block_type = block_value
            .and_then(|block| block.get("type"))
            .and_then(Value::as_str);
        if !matches!(block_type, Some("text" | "tool_use")) {
            return events;
        }
        let block = if block_type == Some("tool_use") {
            Block {
                output_index,
                item_id: format!("fc_{}_{}", self.response_key(), key),
                closed: false,
                kind: BlockKind::ToolUse {
                    call_id: block_value
                        .and_then(|b| b.get("id"))
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_owned(),
                    name: block_value
                        .and_then(|b| b.get("name"))
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_owned(),
                    arguments: String::new(),
                },
            }
        } else {
            Block {
                output_index,
                item_id: self.text_item_id(key),
                closed: false,
                kind: BlockKind::Text {
                    text: block_value
                        .and_then(|b| b.get("text"))
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_owned(),
                },
            }
        };

        match &block.kind {
            BlockKind::Text { .. } => {
                events.push(self.event(
                    "response.output_item.added",
                    json!({
                        "output_index": output_index,
                        "item": {
                            "id": block.item_id,
                            "type": "message",
                            "status": "in_progress",
                            "role": "assistant",
                            "content": [],
                        },
                    }),
                ));
                events.push(self.event(
                    "response.content_part.added",
                    json!({
                        "item_id": block.item_id,
                        "output_index": output_index,
                        "content_index": 0,
                        "part": { "type": "output_text", "text": "", "annotations": [] },
                    }),
                ));
            }
            BlockKind::ToolUse { .. } => {
                let item = call_item(&block, "in_progress");
                events.push(self.event(
                    "response.output_item.added",
                    json!({ "output_index": output_index, "item": item }),
                ));
            }
        }
        self.index_of.insert(key, output_index);
        self.order.push(block);
        events
    }

    fn push_block_delta(&mut self, index: Option<i64>, delta: Option<&Value>) -> Vec<Value> {
        let Some(position) = index.and_then(|index| self.index_of.get(&index).copied()) else {
            return Vec::new();
        };
        // A delta for a block that already closed is a broken stream; skip it.
        if self.order[position].closed {
            return Vec::new();
        }
        let item_id = self.order[position].item_id.clone();
        let output_index = self.order[position].output_index;
        match &mut self.order[position].kind {
            BlockKind::Text { text } => {
                let chunk = delta.and_then(|d| d.get("text")).and_then(Value::as_str);
                let is_text =
                    delta.and_then(|d| d.get("type")).and_then(Value::as_str) == Some("text_delta");
                let Some(chunk) = chunk.filter(|chunk| is_text && !chunk.is_empty()) else {
                    return Vec::new();
                };
                text.push_str(chunk);
                let chunk = chunk.to_owned();
                vec![self.event(
                    "response.output_text.delta",
                    json!({
                        "item_id": item_id,
                        "output_index": output_index,
                        "content_index": 0,
                        "delta": chunk,
                    }),
                )]
            }
            BlockKind::ToolUse { arguments, .. } => {
                let partial = delta
                    .and_then(|d| d.get("partial_json"))
                    .and_then(Value::as_str);
                let is_json = delta.and_then(|d| d.get("type")).and_then(Value::as_str)
                    == Some("input_json_delta");
                let Some(partial) = partial.filter(|_| is_json) else {
                    return Vec::new();
                };
                arguments.push_str(partial);
                let partial = partial.to_owned();
                vec![self.event(
                    "response.function_call_arguments.delta",
                    json!({
                        "item_id": item_id,
                        "output_index": output_index,
                        "delta": partial,
                    }),
                )]
            }
        }
    }

    fn close_open_blocks(&mut self) -> Vec<Value> {
        let mut events = Vec::new();
        for position in 0..self.order.len() {
            if !self.order[position].closed {
                events.extend(self.close_block(position));
            }
        }
        events
    }

    fn close_block_at(&mut self, index: i64) -> Vec<Value> {
        match self.index_of.get(&index).copied() {
            Some(position) => self.close_block(position),
            None => Vec::new(),
        }
    }

    fn close_block(&mut self, position: usize) -> Vec<Value> {
        if self.order[position].closed {
            return Vec::new();
        }
        self.order[position].closed = true;
        let item_id = self.order[position].item_id.clone();
        let output_index = self.order[position].output_index;
        match &self.order[position].kind {
            BlockKind::Text { text } => {
                let text = text.clone();
                vec![
                    self.event(
                        "response.output_text.done",
                        json!({
                            "item_id": item_id,
                            "output_index": output_index,
                            "content_index": 0,
                            "text": text,
                        }),
                    ),
                    self.event(
                        "response.content_part.done",
                        json!({
                            "item_id": item_id,
                            "output_index": output_index,
                            "content_index": 0,
                            "part": { "type": "output_text", "text": text, "annotations": [] },
                        }),
                    ),
                    self.event(
                        "response.output_item.done",
                        json!({ "output_index": output_index, "item": message_item(&item_id, &text) }),
                    ),
                ]
            }
            BlockKind::ToolUse { arguments, .. } => {
                let arguments = arguments.clone();
                let item = call_item(&self.order[position], "completed");
                vec![
                    self.event(
                        "response.function_call_arguments.done",
                        json!({
                            "item_id": item_id,
                            "output_index": output_index,
                            "arguments": arguments,
                        }),
                    ),
                    self.event(
                        "response.output_item.done",
                        json!({ "output_index": output_index, "item": item }),
                    ),
                ]
            }
        }
    }

    /// `response.created` + `response.in_progress`, once per turn.
    fn start(&mut self) -> Vec<Value> {
        if self.started {
            return Vec::new();
        }
        self.started = true;
        vec![
            self.event("response.created", json!({})),
            self.event("response.in_progress", json!({})),
        ]
    }

    fn output_items(&self) -> Vec<Value> {
        self.order
            .iter()
            .map(|block| match &block.kind {
                BlockKind::Text { text } => message_item(&block.item_id, text),
                BlockKind::ToolUse { .. } => call_item(block, "completed"),
            })
            .collect()
    }

    fn response_value(&self, output: Vec<Value>) -> Value {
        response_object(
            &self.response_key(),
            self.model.as_deref().unwrap_or(&self.fallback_model),
            output,
            self.usage_value(),
            self.stop_reason.as_deref(),
        )
    }

    fn usage_value(&self) -> Value {
        if self.input_tokens.is_none() && self.output_tokens.is_none() {
            return Value::Null;
        }
        let input = self.input_tokens.unwrap_or(0);
        let output = self.output_tokens.unwrap_or(0);
        json!({ "input_tokens": input, "output_tokens": output, "total_tokens": input + output })
    }

    /// Each event carries the running `sequence_number` the Responses stream has.
    fn event(&mut self, kind: &str, payload: Value) -> Value {
        let mut event = payload.as_object().cloned().unwrap_or_default();
        event.insert("type".into(), Value::String(kind.into()));
        event.insert("sequence_number".into(), json!(self.sequence));
        self.sequence += 1;
        if kind == "response.created" || kind == "response.in_progress" {
            let mut in_progress = self
                .response_value(Vec::new())
                .as_object()
                .cloned()
                .unwrap_or_default();
            in_progress.insert("status".into(), Value::String("in_progress".into()));
            in_progress.insert("usage".into(), Value::Null);
            event.insert("response".into(), Value::Object(in_progress));
        }
        Value::Object(event)
    }

    fn response_key(&self) -> String {
        self.response_id.clone().unwrap_or_else(|| "message".into())
    }

    /// The single text block a turn normally has keeps the plain `msg_` item id;
    /// a further text block stays unique through its Anthropic index.
    fn text_item_id(&self, key: i64) -> String {
        let taken = self
            .order
            .iter()
            .any(|block| matches!(block.kind, BlockKind::Text { .. }));
        if taken {
            format!("msg_{}_{}", self.response_key(), key)
        } else {
            format!("msg_{}", self.response_key())
        }
    }
}

fn message_item(id: &str, text: &str) -> Value {
    json!({
        "id": id,
        "type": "message",
        "status": "completed",
        "role": "assistant",
        "content": [{ "type": "output_text", "text": text, "annotations": [] }],
    })
}

fn call_item(block: &Block, status: &str) -> Value {
    let (call_id, name, arguments) = match &block.kind {
        BlockKind::ToolUse {
            call_id,
            name,
            arguments,
        } => (call_id.as_str(), name.as_str(), arguments.as_str()),
        BlockKind::Text { .. } => ("", "", ""),
    };
    json!({
        "id": block.item_id,
        "type": "function_call",
        "status": status,
        "call_id": call_id,
        "name": name,
        "arguments": arguments,
    })
}

fn response_object(
    id: &str,
    model: &str,
    output: Vec<Value>,
    usage: Value,
    stop_reason: Option<&str>,
) -> Value {
    let response_id = if id.starts_with("resp_") {
        id.to_owned()
    } else {
        format!("resp_{id}")
    };
    let incomplete = stop_reason == Some("max_tokens");
    json!({
        "id": response_id,
        "object": "response",
        // Messages events carry no creation timestamp, so the response reports
        // the epoch on a missing field.
        "created_at": 0,
        "status": if incomplete { "incomplete" } else { "completed" },
        "error": Value::Null,
        "incomplete_details": if incomplete { json!({"reason": "max_output_tokens"}) } else { Value::Null },
        "model": model,
        "output": output,
        "parallel_tool_calls": true,
        "previous_response_id": Value::Null,
        "store": false,
        "tool_choice": "auto",
        "tools": [],
        "usage": usage,
        "metadata": {},
    })
}

fn integer_field(value: &Value, key: &str) -> Option<i64> {
    value.get(key).and_then(Value::as_i64)
}

/// A frame ends at a blank line; both `\n\n` and `\r\n\r\n` appear in the wild.
/// `\n` is ASCII, so splitting on it never severs a UTF-8 codepoint.
fn frame_end(buffer: &[u8]) -> Option<usize> {
    let lf = find(buffer, b"\n\n").map(|index| index + 2);
    let crlf = find(buffer, b"\r\n\r\n").map(|index| index + 4);
    match (lf, crlf) {
        (Some(lf), Some(crlf)) => Some(lf.min(crlf)),
        (lf, crlf) => lf.or(crlf),
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// Parse the `data:` line of one SSE frame. Anthropic frames every event as
/// `event: <name>` plus `data: {json}`; only the `data:` line is parsed. There
/// is no `[DONE]` sentinel — the stream ends with `message_stop`.
fn parse_frame(frame: &[u8]) -> Option<Value> {
    let text = std::str::from_utf8(frame).ok()?;
    let data = text
        .lines()
        .filter_map(|line| line.strip_prefix("data:").map(str::trim_start))
        .collect::<Vec<_>>()
        .join("\n");
    (!data.is_empty())
        .then(|| serde_json::from_str::<Value>(&data).ok())
        .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn responses_request() -> Value {
        json!({
            "model": "claude-large",
            "instructions": "You are Codex.",
            "stream": true,
            "max_output_tokens": 256,
            "temperature": 0.2,
            "top_p": 0.9,
            "previous_response_id": "resp_should_be_dropped",
            "store": true,
            "reasoning": { "effort": "medium" },
            "metadata": { "origin": "test" },
            "parallel_tool_calls": false,
            "input": [
                { "type": "message", "role": "user", "content": [{ "type": "input_text", "text": "hi" }] },
                { "type": "message", "role": "assistant", "content": [{ "type": "output_text", "text": "hello" }] },
                { "type": "message", "role": "system", "content": [{ "type": "input_text", "text": "be terse" }] },
                { "type": "function_call", "name": "shell", "call_id": "toolu_1", "arguments": "{\"cmd\":\"ls\"}" },
                { "type": "function_call_output", "call_id": "toolu_1", "output": "README.md" },
                "and now?"
            ],
            "tools": [{
                "type": "function",
                "name": "shell",
                "description": "Run a command",
                "parameters": { "type": "object", "properties": { "cmd": { "type": "string" } } }
            }],
            "tool_choice": "auto"
        })
    }

    #[test]
    fn a_responses_request_becomes_messages_without_responses_only_state() {
        let body = request_from_responses(&responses_request()).unwrap();
        assert_eq!(body["model"], "claude-large");
        assert_eq!(body["stream"], true);
        assert_eq!(body["max_tokens"], 256);
        assert_eq!(body["system"], "You are Codex.");
        assert_eq!(body["temperature"], 0.2);
        assert_eq!(body["top_p"], 0.9);
        for dropped in [
            "previous_response_id",
            "store",
            "reasoning",
            "metadata",
            "instructions",
            "input",
            "max_output_tokens",
            "parallel_tool_calls",
        ] {
            assert!(body.get(dropped).is_none(), "{dropped} was forwarded");
        }

        let messages = body["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 6);
        assert_eq!(
            messages[0],
            json!({ "role": "user", "content": [{ "type": "text", "text": "hi" }] })
        );
        assert_eq!(
            messages[1],
            json!({ "role": "assistant", "content": [{ "type": "text", "text": "hello" }] })
        );
        // A replayed system-role item stays as user text, not merged into `system`.
        assert_eq!(
            messages[2],
            json!({ "role": "user", "content": [{ "type": "text", "text": "be terse" }] })
        );
        assert_eq!(
            messages[3],
            json!({
                "role": "assistant",
                "content": [{ "type": "tool_use", "id": "toolu_1", "name": "shell", "input": { "cmd": "ls" } }]
            })
        );
        assert_eq!(
            messages[4],
            json!({
                "role": "user",
                "content": [{ "type": "tool_result", "tool_use_id": "toolu_1", "content": "README.md" }]
            })
        );
        assert_eq!(
            messages[5],
            json!({ "role": "user", "content": [{ "type": "text", "text": "and now?" }] })
        );

        assert_eq!(
            body["tools"],
            json!([{
                "name": "shell",
                "description": "Run a command",
                "input_schema": { "type": "object", "properties": { "cmd": { "type": "string" } } }
            }])
        );
        assert_eq!(body["tool_choice"], json!({ "type": "auto" }));
    }

    #[test]
    fn max_tokens_defaults_and_reasoning_items_are_dropped() {
        let body = request_from_responses(&json!({
            "model": "claude-large",
            "input": [
                { "type": "reasoning", "encrypted_content": "never-forward" },
                { "type": "message", "role": "user", "content": [{ "type": "input_text", "text": "go" }] }
            ]
        }))
        .unwrap();
        assert_eq!(body["max_tokens"], DEFAULT_MAX_TOKENS);
        let messages = body["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 1);
        assert!(!body.to_string().contains("never-forward"));

        let body = request_from_responses(&json!({
            "model": "claude-large",
            "input": [
                { "type": "reasoning", "summary": [{"type": "summary_text", "text": "private reasoning"}] },
                { "type": "message", "role": "user", "content": [{ "type": "input_text", "text": "go" }] }
            ]
        }))
        .unwrap();
        assert!(!body.to_string().contains("private reasoning"));
    }

    #[test]
    fn image_inputs_preserve_url_and_base64_sources() {
        let request: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/anthropic_messages/request_with_images.json"
        ))
        .unwrap();
        let body = request_from_responses(&request).unwrap();
        let content = body["messages"][0]["content"].as_array().unwrap();
        assert_eq!(content[0]["text"], "compare these");
        assert_eq!(content[1]["source"]["type"], "url");
        assert_eq!(content[1]["source"]["url"], "https://example.com/image.png");
        assert_eq!(content[2]["source"]["type"], "base64");
        assert_eq!(content[2]["source"]["media_type"], "image/jpeg");
        assert_eq!(content[2]["source"]["data"], "aGVsbG8=");
    }

    #[test]
    fn non_streaming_private_thinking_is_dropped_and_ids_are_normalized() {
        let response = response_from_messages(&json!({
            "id": "resp_message",
            "model": "claude-large",
            "stop_reason": "end_turn",
            "content": [
                {"type": "thinking", "thinking": "private reasoning"},
                {"type": "text", "text": "public answer"}
            ]
        }))
        .unwrap();
        assert_eq!(response["id"], "resp_message");
        assert_eq!(response["output"].as_array().unwrap().len(), 1);
        assert!(!response.to_string().contains("private reasoning"));
    }

    #[test]
    fn an_empty_input_is_rejected() {
        assert!(request_from_responses(&json!({ "model": "x", "input": [] })).is_err());
        assert!(request_from_responses(&json!({ "model": "x" })).is_err());
    }

    fn kinds(events: &[Value]) -> Vec<String> {
        events
            .iter()
            .map(|event| event["type"].as_str().unwrap().to_owned())
            .collect()
    }

    #[test]
    fn a_text_turn_produces_the_documented_responses_event_order() {
        let body = concat!(
            "event: message_start\n",
            "data: {\"type\":\"message_start\",\"message\":{\"id\":\"m1\",\"model\":\"claude-large\",\"usage\":{\"input_tokens\":4}}}\n\n",
            "event: content_block_start\n",
            "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
            "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hel\"}}\n\n",
            "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"lo\"}}\n\n",
            "data: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
            "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":2}}\n\n",
            "data: {\"type\":\"message_stop\"}\n\n",
        );
        let mut translator = MessagesTranslator::new("fallback");
        let mut events = translator.push(body.as_bytes());
        events.extend(translator.finish());
        assert_eq!(
            kinds(&events),
            [
                "response.created",
                "response.in_progress",
                "response.output_item.added",
                "response.content_part.added",
                "response.output_text.delta",
                "response.output_text.delta",
                "response.output_text.done",
                "response.content_part.done",
                "response.output_item.done",
                "response.completed",
            ]
        );
        // A running sequence number on every event.
        for (index, event) in events.iter().enumerate() {
            assert_eq!(event["sequence_number"], json!(index));
        }
        let completed = events.last().unwrap();
        assert_eq!(completed["response"]["id"], "resp_m1");
        assert_eq!(completed["response"]["model"], "claude-large");
        assert_eq!(completed["response"]["usage"]["input_tokens"], 4);
        assert_eq!(completed["response"]["usage"]["output_tokens"], 2);
        assert_eq!(
            completed["response"]["output"][0]["content"][0]["text"],
            "Hello"
        );
    }

    #[test]
    fn a_tool_use_block_becomes_a_function_call() {
        let body = concat!(
            "data: {\"type\":\"message_start\",\"message\":{\"id\":\"m2\"}}\n\n",
            "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"tool_use\",\"id\":\"toolu_9\",\"name\":\"shell\"}}\n\n",
            "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"cmd\\\":\"}}\n\n",
            "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"\\\"ls\\\"}\"}}\n\n",
            "data: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
            "data: {\"type\":\"message_stop\"}\n\n",
        );
        let mut translator = MessagesTranslator::new("fallback");
        let mut events = translator.push(body.as_bytes());
        events.extend(translator.finish());
        assert!(kinds(&events).contains(&"response.function_call_arguments.delta".to_owned()));
        let done = events
            .iter()
            .find(|event| event["type"] == "response.output_item.done")
            .unwrap();
        assert_eq!(done["item"]["type"], "function_call");
        assert_eq!(done["item"]["call_id"], "toolu_9");
        assert_eq!(done["item"]["name"], "shell");
        assert_eq!(done["item"]["arguments"], "{\"cmd\":\"ls\"}");
    }

    #[test]
    fn fragmented_utf8_and_tool_deltas_survive_chunking() {
        let stream = include_bytes!(
            "../../tests/fixtures/anthropic_messages/fragmented_utf8_tool_calls.sse"
        );
        let character = "你".as_bytes();
        let offset = stream
            .windows(character.len())
            .position(|window| window == character)
            .unwrap();
        let mut translator = MessagesTranslator::new("fallback");
        let mut events = translator.push(&stream[..offset + 1]);
        events.extend(translator.push(&stream[offset + 1..]));
        events.extend(translator.finish());
        let completed = events
            .iter()
            .find(|event| event["type"] == "response.completed")
            .unwrap();
        assert_eq!(
            completed["response"]["output"][0]["content"][0]["text"],
            "你好"
        );
        assert_eq!(completed["response"]["output"][1]["name"], "read");
        assert_eq!(completed["response"]["output"][1]["arguments"], "{\"p\":1}");
    }

    #[test]
    fn frames_split_across_chunks_reassemble() {
        let body = concat!(
            "data: {\"type\":\"message_start\",\"message\":{\"id\":\"m1\"}}\n\n",
            "data: {\"type\":\"message_stop\"}\n\n",
        );
        let mut translator = MessagesTranslator::new("fallback");
        let mut events = Vec::new();
        // One byte at a time: a frame split anywhere still parses.
        for byte in body.as_bytes() {
            events.extend(translator.push(&[*byte]));
        }
        events.extend(translator.finish());
        let all = kinds(&events);
        assert_eq!(all.first().unwrap(), "response.created");
        assert_eq!(all.last().unwrap(), "response.completed");
    }

    #[test]
    fn crlf_frames_and_a_trailing_frame_without_a_blank_line() {
        let mut translator = MessagesTranslator::new("fallback");
        let mut events =
            translator.push(b": keep-alive\r\n\r\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"m3\"}}\r\n\r\n");
        // The last frame has no trailing blank line; finish() flushes it.
        events.extend(translator.push(b"data: {\"type\":\"message_stop\"}"));
        events.extend(translator.finish());
        assert_eq!(kinds(&events).last().unwrap(), "response.completed");
    }

    #[test]
    fn max_tokens_stream_is_incomplete_and_private_blocks_are_skipped() {
        let body = concat!(
            "data: {\"type\":\"message_start\",\"message\":{\"id\":\"resp_m4\"}}\n\n",
            "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"thinking\",\"thinking\":\"private\"}}\n\n",
            "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"thinking_delta\",\"thinking\":\"private\"}}\n\n",
            "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"max_tokens\"}}\n\n",
            "data: {\"type\":\"message_stop\"}\n\n",
        );
        let mut translator = MessagesTranslator::new("fallback");
        let mut events = translator.push(body.as_bytes());
        events.extend(translator.finish());
        let completed = events.last().unwrap();
        assert_eq!(completed["type"], "response.incomplete");
        assert_eq!(completed["response"]["id"], "resp_m4");
        assert_eq!(completed["response"]["status"], "incomplete");
        assert_eq!(completed["response"]["output"], json!([]));
        assert!(!completed.to_string().contains("private"));
    }

    #[test]
    fn premature_stream_eof_does_not_synthesize_completion() {
        let mut translator = MessagesTranslator::new("fallback");
        let events =
            translator.push(b"data: {\"type\":\"message_start\",\"message\":{\"id\":\"m5\"}}\n\n");
        assert!(
            events
                .iter()
                .all(|event| event["type"] != "response.completed")
        );
        assert!(translator.finish().is_empty());
    }

    #[test]
    fn an_error_event_is_terminal() {
        let mut translator = MessagesTranslator::new("fallback");
        let mut events = translator.push(
            b"data: {\"type\":\"error\",\"error\":{\"type\":\"overloaded_error\",\"message\":\"busy\"}}\n\n",
        );
        // Events after the terminal error are ignored.
        events.extend(translator.push(b"data: {\"type\":\"message_stop\"}\n\n"));
        events.extend(translator.finish());
        assert_eq!(kinds(&events), ["error"]);
        assert_eq!(events[0]["code"], "overloaded_error");
        assert_eq!(events[0]["message"], "busy");
    }
}
