use std::collections::BTreeMap;

use anyhow::{Result, bail};
use serde_json::{Map, Value, json};

use super::sse;

pub fn request_from_responses(request: &Value) -> Result<Value> {
    let source = request
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("Responses request must be a JSON object"))?;
    let mut messages = Vec::new();
    if let Some(instructions) = source.get("instructions").and_then(Value::as_str)
        && !instructions.is_empty()
    {
        messages.push(json!({"role": "system", "content": instructions}));
    }
    match source.get("input") {
        Some(Value::String(text)) => messages.push(json!({"role": "user", "content": text})),
        Some(Value::Array(items)) => append_input_items(items, &mut messages)?,
        None | Some(Value::Null) => {}
        Some(_) => bail!("Responses input must be a string or array"),
    }
    if messages.is_empty() {
        bail!("Responses request has no input to send");
    }

    let mut body = Map::new();
    copy(source, &mut body, "model", "model");
    body.insert("messages".into(), Value::Array(messages));
    copy(source, &mut body, "temperature", "temperature");
    copy(source, &mut body, "top_p", "top_p");
    copy(source, &mut body, "stream", "stream");
    copy(
        source,
        &mut body,
        "parallel_tool_calls",
        "parallel_tool_calls",
    );
    if let Some(maximum) = source
        .get("max_output_tokens")
        .filter(|value| !value.is_null())
    {
        body.insert("max_tokens".into(), maximum.clone());
    }
    if source.get("stream").and_then(Value::as_bool) == Some(true) {
        body.insert("stream_options".into(), json!({"include_usage": true}));
    }
    if let Some(effort) = source
        .get("reasoning")
        .and_then(|reasoning| reasoning.get("effort"))
        .filter(|value| !value.is_null())
    {
        body.insert("reasoning_effort".into(), effort.clone());
    }
    if let Some(tools) = source.get("tools").and_then(Value::as_array) {
        let tools: Vec<Value> = tools.iter().filter_map(tool_from_responses).collect();
        if !tools.is_empty() {
            body.insert("tools".into(), Value::Array(tools));
        }
    }
    if let Some(choice) = source
        .get("tool_choice")
        .and_then(tool_choice_from_responses)
    {
        body.insert("tool_choice".into(), choice);
    }
    Ok(Value::Object(body))
}

fn copy(source: &Map<String, Value>, target: &mut Map<String, Value>, from: &str, to: &str) {
    if let Some(value) = source.get(from).filter(|value| !value.is_null()) {
        target.insert(to.into(), value.clone());
    }
}

fn append_input_items(items: &[Value], messages: &mut Vec<Value>) -> Result<()> {
    for item in items {
        let Some(object) = item.as_object() else {
            bail!("Responses input item must be an object");
        };
        match object.get("type").and_then(Value::as_str) {
            Some("reasoning") => {}
            Some("function_call") | Some("custom_tool_call") => {
                let message = json!({
                    "role": "assistant",
                    "content": Value::Null,
                    "tool_calls": [{
                        "id": object.get("call_id").and_then(Value::as_str).unwrap_or(""),
                        "type": "function",
                        "function": {
                            "name": object.get("name").and_then(Value::as_str).unwrap_or(""),
                            "arguments": arguments_text(object.get("arguments").or_else(|| object.get("input"))),
                        }
                    }]
                });
                messages.push(message);
            }
            Some("function_call_output") | Some("custom_tool_call_output") => {
                messages.push(json!({
                    "role": "tool",
                    "tool_call_id": object.get("call_id").and_then(Value::as_str).unwrap_or(""),
                    "content": content_text(object.get("output")),
                }));
            }
            Some("message") | None if object.contains_key("role") => {
                let role = object.get("role").and_then(Value::as_str).unwrap_or("user");
                let message = json!({
                    "role": role,
                    "content": chat_content(object.get("content")),
                });
                messages.push(message);
            }
            Some("compaction") => {
                if let Some(text) = object
                    .get("summary")
                    .or_else(|| object.get("content"))
                    .and_then(Value::as_str)
                {
                    messages.push(json!({"role": "system", "content": text}));
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn chat_content(value: Option<&Value>) -> Value {
    match value {
        Some(Value::String(text)) => Value::String(text.clone()),
        Some(Value::Array(parts)) => Value::Array(
            parts
                .iter()
                .filter_map(|part| match part.get("type").and_then(Value::as_str) {
                    Some("input_text" | "output_text" | "text") => Some(json!({
                        "type": "text",
                        "text": part.get("text").and_then(Value::as_str).unwrap_or("")
                    })),
                    Some("input_image" | "image_url") => Some(json!({
                        "type": "image_url",
                        "image_url": {
                            "url": part.get("image_url")
                                .or_else(|| part.get("url"))
                                .and_then(Value::as_str)
                                .unwrap_or("")
                        }
                    })),
                    _ => None,
                })
                .collect(),
        ),
        _ => Value::String(String::new()),
    }
}

fn content_text(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(text)) => text.clone(),
        Some(value) => serde_json::to_string(value).unwrap_or_default(),
        None => String::new(),
    }
}

fn arguments_text(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(text)) => text.clone(),
        Some(value) => serde_json::to_string(value).unwrap_or_else(|_| "{}".into()),
        None => "{}".into(),
    }
}

fn tool_from_responses(tool: &Value) -> Option<Value> {
    match tool.get("type").and_then(Value::as_str) {
        Some("function") => {
            let source = tool.get("function").unwrap_or(tool);
            let mut function = Map::new();
            function.insert("name".into(), source.get("name")?.clone());
            if let Some(description) = source.get("description").filter(|value| !value.is_null()) {
                function.insert("description".into(), description.clone());
            }
            function.insert(
                "parameters".into(),
                source
                    .get("parameters")
                    .cloned()
                    .unwrap_or_else(|| json!({"type": "object", "properties": {}})),
            );
            if let Some(strict) = source.get("strict").filter(|value| !value.is_null()) {
                function.insert("strict".into(), strict.clone());
            }
            Some(json!({"type": "function", "function": function}))
        }
        Some("custom") => Some(json!({
            "type": "function",
            "function": {
                "name": tool.get("name")?,
                "description": tool.get("description").cloned().unwrap_or(Value::Null),
                "parameters": tool.get("format")
                    .or_else(|| tool.get("parameters"))
                    .cloned()
                    .unwrap_or_else(|| json!({"type": "object", "properties": {}})),
            }
        })),
        _ => None,
    }
}

fn tool_choice_from_responses(choice: &Value) -> Option<Value> {
    match choice.as_str() {
        Some("auto" | "none" | "required") => Some(choice.clone()),
        _ => choice
            .get("name")
            .map(|name| json!({"type": "function", "function": {"name": name}})),
    }
}

pub fn response_from_chat(body: &Value) -> Result<Value> {
    let choice = body
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .ok_or_else(|| anyhow::anyhow!("Chat response has no choices"))?;
    let message = choice
        .get("message")
        .ok_or_else(|| anyhow::anyhow!("Chat response has no message"))?;
    let response_id = response_id(body.get("id").and_then(Value::as_str));
    let mut output = Vec::new();
    if let Some(reasoning) = chat_reasoning(message).filter(|text| !text.is_empty()) {
        output.push(json!({
            "id": format!("rs_{}", short_id(&response_id)),
            "type": "reasoning",
            "summary": [{"type": "summary_text", "text": reasoning}],
        }));
    }
    if let Some(content) = message.get("content").and_then(Value::as_str)
        && !content.is_empty()
    {
        output.push(message_item(&response_id, content));
    }
    if let Some(calls) = message.get("tool_calls").and_then(Value::as_array) {
        for (index, call) in calls.iter().enumerate() {
            output.push(tool_item(call, &response_id, index, "completed"));
        }
    }
    let status = if choice.get("finish_reason").and_then(Value::as_str) == Some("length") {
        "incomplete"
    } else {
        "completed"
    };
    Ok(response_object(
        &response_id,
        body.get("model").and_then(Value::as_str).unwrap_or(""),
        body.get("created").and_then(Value::as_i64).unwrap_or(0),
        status,
        output,
        usage_from_chat(body.get("usage")),
    ))
}

fn chat_reasoning(value: &Value) -> Option<String> {
    value
        .get("reasoning_content")
        .or_else(|| value.get("reasoning"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

#[derive(Clone, Default)]
struct ToolState {
    call_id: String,
    name: String,
    arguments: String,
    item_id: String,
    output_index: usize,
    added: bool,
}

pub struct ChatTranslator {
    fallback_model: String,
    buffer: Vec<u8>,
    response_id: String,
    model: String,
    created: i64,
    sequence: u64,
    started: bool,
    completed: bool,
    text: String,
    text_added: bool,
    reasoning: String,
    reasoning_added: bool,
    tools: BTreeMap<usize, ToolState>,
    usage: Value,
    finish_reason: Option<String>,
}

impl ChatTranslator {
    pub fn new(fallback_model: impl Into<String>) -> Self {
        Self {
            fallback_model: fallback_model.into(),
            buffer: Vec::new(),
            response_id: "resp_modelmux".into(),
            model: String::new(),
            created: 0,
            sequence: 0,
            started: false,
            completed: false,
            text: String::new(),
            text_added: false,
            reasoning: String::new(),
            reasoning_added: false,
            tools: BTreeMap::new(),
            usage: usage_from_chat(None),
            finish_reason: None,
        }
    }

    pub fn push(&mut self, chunk: &[u8]) -> Vec<Value> {
        let frames = sse::frames(&mut self.buffer, chunk);
        let mut events = Vec::new();
        for frame in frames {
            let Some(data) = sse::data(&frame) else {
                continue;
            };
            if data == "[DONE]" {
                events.extend(self.finish_response());
                continue;
            }
            if let Ok(value) = serde_json::from_str::<Value>(&data) {
                events.extend(self.push_chunk(&value));
            }
        }
        events
    }

    pub fn finish(&mut self) -> Vec<Value> {
        if self.buffer.is_empty() {
            return Vec::new();
        }
        self.buffer.extend_from_slice(b"\n\n");
        self.push(&[])
    }

    fn push_chunk(&mut self, chunk: &Value) -> Vec<Value> {
        if self.completed {
            return Vec::new();
        }
        if let Some(id) = chunk.get("id").and_then(Value::as_str) {
            self.response_id = response_id(Some(id));
        }
        if let Some(model) = chunk.get("model").and_then(Value::as_str) {
            self.model = model.to_string();
        }
        if let Some(created) = chunk.get("created").and_then(Value::as_i64) {
            self.created = created;
        }
        if let Some(usage) = chunk.get("usage").filter(|value| !value.is_null()) {
            self.usage = usage_from_chat(Some(usage));
        }
        let mut events = self.start();
        let Some(choice) = chunk
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|choices| choices.first())
        else {
            return events;
        };
        if let Some(delta) = choice.get("delta") {
            if let Some(reasoning) = chat_reasoning(delta).filter(|text| !text.is_empty()) {
                events.extend(self.push_reasoning(&reasoning));
            }
            if let Some(text) = delta
                .get("content")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
            {
                events.extend(self.push_text(text));
            }
            if let Some(calls) = delta.get("tool_calls").and_then(Value::as_array) {
                for call in calls {
                    events.extend(self.push_tool(call));
                }
            }
        }
        if let Some(reason) = choice.get("finish_reason").and_then(Value::as_str) {
            self.finish_reason = Some(reason.to_string());
        }
        events
    }

    fn start(&mut self) -> Vec<Value> {
        if self.started {
            return Vec::new();
        }
        self.started = true;
        vec![
            self.event(
                "response.created",
                json!({"response": self.base_response("in_progress")}),
            ),
            self.event(
                "response.in_progress",
                json!({"response": self.base_response("in_progress")}),
            ),
        ]
    }

    fn push_text(&mut self, text: &str) -> Vec<Value> {
        let mut events = Vec::new();
        let item_id = format!("msg_{}", short_id(&self.response_id));
        if !self.text_added {
            self.text_added = true;
            let index = self.output_index();
            events.push(self.event("response.output_item.added", json!({
                "output_index": index,
                "item": {"id": item_id, "type": "message", "status": "in_progress", "role": "assistant", "content": []}
            })));
            events.push(self.event(
                "response.content_part.added",
                json!({
                    "item_id": item_id, "output_index": index, "content_index": 0,
                    "part": {"type": "output_text", "text": "", "annotations": []}
                }),
            ));
        }
        let index = self.text_index();
        self.text.push_str(text);
        events.push(self.event(
            "response.output_text.delta",
            json!({
                "item_id": item_id, "output_index": index, "content_index": 0, "delta": text
            }),
        ));
        events
    }

    fn push_reasoning(&mut self, text: &str) -> Vec<Value> {
        let mut events = Vec::new();
        let item_id = format!("rs_{}", short_id(&self.response_id));
        if !self.reasoning_added {
            self.reasoning_added = true;
            events.push(self.event(
                "response.output_item.added",
                json!({
                    "output_index": 0,
                    "item": {"id": item_id, "type": "reasoning", "summary": []}
                }),
            ));
            events.push(self.event(
                "response.reasoning_summary_part.added",
                json!({
                    "item_id": item_id, "output_index": 0, "summary_index": 0,
                    "part": {"type": "summary_text", "text": ""}
                }),
            ));
        }
        self.reasoning.push_str(text);
        events.push(self.event(
            "response.reasoning_summary_text.delta",
            json!({
                "item_id": item_id, "output_index": 0, "summary_index": 0, "delta": text
            }),
        ));
        events
    }

    fn push_tool(&mut self, call: &Value) -> Vec<Value> {
        let index = call.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
        let function = call.get("function").unwrap_or(&Value::Null);
        let output_index = self.output_count_before_tools() + index;
        let state = self.tools.entry(index).or_insert_with(|| ToolState {
            item_id: format!("fc_{}_{}", short_id(&self.response_id), index),
            output_index: usize::MAX,
            ..ToolState::default()
        });
        if let Some(id) = call.get("id").and_then(Value::as_str) {
            append_metadata(&mut state.call_id, id);
        }
        if let Some(name) = function.get("name").and_then(Value::as_str) {
            append_metadata(&mut state.name, name);
        }
        let delta = function
            .get("arguments")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        state.arguments.push_str(&delta);

        let mut add_item = None;
        if !state.added && !state.name.is_empty() {
            state.added = true;
            state.output_index = output_index;
            add_item = Some((
                state.item_id.clone(),
                state.output_index,
                state.call_id.clone(),
                state.name.clone(),
            ));
        }
        let item_id = state.item_id.clone();
        let output_index = state.output_index;
        let mut events = Vec::new();
        if let Some((item_id, output_index, call_id, name)) = add_item {
            events.push(self.event("response.output_item.added", json!({
                "output_index": output_index,
                "item": {"id": item_id, "type": "function_call", "status": "in_progress", "call_id": call_id, "name": name, "arguments": ""}
            })));
        }
        if output_index != usize::MAX && !delta.is_empty() {
            events.push(self.event(
                "response.function_call_arguments.delta",
                json!({
                    "item_id": item_id, "output_index": output_index, "delta": delta
                }),
            ));
        }
        events
    }

    fn finish_response(&mut self) -> Vec<Value> {
        if self.completed {
            return Vec::new();
        }
        self.completed = true;
        let mut events = self.start();
        let mut output = Vec::new();
        if self.reasoning_added {
            let item_id = format!("rs_{}", short_id(&self.response_id));
            let item = json!({"id": item_id, "type": "reasoning", "summary": [{"type": "summary_text", "text": self.reasoning}]});
            events.push(self.event("response.reasoning_summary_text.done", json!({"item_id": item_id, "output_index": 0, "summary_index": 0, "text": self.reasoning})));
            events.push(self.event(
                "response.output_item.done",
                json!({"output_index": 0, "item": item}),
            ));
            output.push(item);
        }
        if self.text_added {
            let item_id = format!("msg_{}", short_id(&self.response_id));
            let index = self.text_index();
            let item = message_item(&self.response_id, &self.text);
            events.push(self.event("response.output_text.done", json!({"item_id": item_id, "output_index": index, "content_index": 0, "text": self.text})));
            events.push(self.event("response.content_part.done", json!({"item_id": item_id, "output_index": index, "content_index": 0, "part": {"type": "output_text", "text": self.text, "annotations": []}})));
            events.push(self.event(
                "response.output_item.done",
                json!({"output_index": index, "item": item}),
            ));
            output.push(item);
        }
        let tool_indices: Vec<usize> = self.tools.keys().copied().collect();
        for index in tool_indices {
            let state = self.tools[&index].clone();
            if !state.added {
                continue;
            }
            let item = json!({
                "id": state.item_id, "type": "function_call", "status": "completed",
                "call_id": state.call_id, "name": state.name, "arguments": state.arguments
            });
            events.push(self.event("response.function_call_arguments.done", json!({"item_id": state.item_id, "output_index": state.output_index, "arguments": state.arguments})));
            events.push(self.event(
                "response.output_item.done",
                json!({"output_index": state.output_index, "item": item}),
            ));
            output.push(item);
        }
        let status = if self.finish_reason.as_deref() == Some("length") {
            "incomplete"
        } else {
            "completed"
        };
        let response = response_object(
            &self.response_id,
            self.model(),
            self.created,
            status,
            output,
            self.usage.clone(),
        );
        let terminal = if status == "incomplete" {
            "response.incomplete"
        } else {
            "response.completed"
        };
        events.push(self.event(terminal, json!({"response": response})));
        events
    }

    fn model(&self) -> &str {
        if self.model.is_empty() {
            &self.fallback_model
        } else {
            &self.model
        }
    }

    fn base_response(&self, status: &str) -> Value {
        response_object(
            &self.response_id,
            self.model(),
            self.created,
            status,
            Vec::new(),
            self.usage.clone(),
        )
    }

    fn output_count_before_tools(&self) -> usize {
        usize::from(self.reasoning_added) + usize::from(self.text_added)
    }

    fn output_index(&self) -> usize {
        self.output_count_before_tools()
    }

    fn text_index(&self) -> usize {
        usize::from(self.reasoning_added)
    }

    fn event(&mut self, kind: &str, extra: Value) -> Value {
        let sequence = self.sequence;
        self.sequence += 1;
        let mut object = extra.as_object().cloned().unwrap_or_default();
        object.insert("type".into(), Value::String(kind.into()));
        object.insert("sequence_number".into(), json!(sequence));
        Value::Object(object)
    }
}

fn append_metadata(target: &mut String, incoming: &str) {
    if incoming.is_empty() || incoming == target {
        return;
    }
    if incoming.starts_with(target.as_str()) {
        target.clear();
        target.push_str(incoming);
    } else if !target.ends_with(incoming) {
        target.push_str(incoming);
    }
}

fn response_id(id: Option<&str>) -> String {
    match id {
        Some(id) if id.starts_with("resp_") => id.to_string(),
        Some(id) => format!("resp_{id}"),
        None => format!("resp_{}", uuid::Uuid::new_v4().simple()),
    }
}

fn short_id(id: &str) -> &str {
    id.strip_prefix("resp_").unwrap_or(id)
}

fn message_item(response_id: &str, text: &str) -> Value {
    json!({
        "id": format!("msg_{}", short_id(response_id)),
        "type": "message", "status": "completed", "role": "assistant",
        "content": [{"type": "output_text", "text": text, "annotations": []}]
    })
}

fn tool_item(call: &Value, response_id: &str, index: usize, status: &str) -> Value {
    let function = call.get("function").unwrap_or(&Value::Null);
    json!({
        "id": format!("fc_{}_{}", short_id(response_id), index),
        "type": "function_call", "status": status,
        "call_id": call.get("id").and_then(Value::as_str).unwrap_or(""),
        "name": function.get("name").and_then(Value::as_str).unwrap_or(""),
        "arguments": function.get("arguments").and_then(Value::as_str).unwrap_or("{}")
    })
}

fn usage_from_chat(value: Option<&Value>) -> Value {
    let input = value
        .and_then(|v| v.get("prompt_tokens"))
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let output = value
        .and_then(|v| v.get("completion_tokens"))
        .and_then(Value::as_i64)
        .unwrap_or(0);
    json!({
        "input_tokens": input,
        "input_tokens_details": {"cached_tokens": value.and_then(|v| v.pointer("/prompt_tokens_details/cached_tokens")).and_then(Value::as_i64).unwrap_or(0)},
        "output_tokens": output,
        "output_tokens_details": {"reasoning_tokens": value.and_then(|v| v.pointer("/completion_tokens_details/reasoning_tokens")).and_then(Value::as_i64).unwrap_or(0)},
        "total_tokens": input + output
    })
}

fn response_object(
    id: &str,
    model: &str,
    created: i64,
    status: &str,
    output: Vec<Value>,
    usage: Value,
) -> Value {
    json!({
        "id": id, "object": "response", "created_at": created, "status": status,
        "error": Value::Null, "incomplete_details": Value::Null, "model": model,
        "output": output, "parallel_tool_calls": true, "previous_response_id": Value::Null,
        "store": false, "tool_choice": "auto", "tools": [], "usage": usage, "metadata": {}
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_tools_and_outputs() {
        let converted = request_from_responses(&json!({
            "model": "glm", "stream": true,
            "input": [
                {"type": "function_call", "call_id": "call_1", "name": "read", "arguments": "{\"path\":\"a\"}"},
                {"type": "function_call_output", "call_id": "call_1", "output": "ok"}
            ]
        })).unwrap();
        assert_eq!(converted["messages"][0]["tool_calls"][0]["id"], "call_1");
        assert_eq!(converted["messages"][1]["tool_call_id"], "call_1");
        assert_eq!(converted["stream_options"]["include_usage"], true);
    }

    #[test]
    fn request_reasoning_items_are_never_forwarded() {
        let converted = request_from_responses(&json!({
            "model": "chat-model",
            "input": [
                {"type": "reasoning", "summary": [{"type": "summary_text", "text": "private reasoning"}]},
                {"type": "message", "role": "user", "content": [{"type": "input_text", "text": "go"}]}
            ]
        }))
        .unwrap();
        assert!(!converted.to_string().contains("private reasoning"));
        assert!(converted.to_string().contains("go"));
    }

    #[test]
    fn responses_function_tools_use_chat_completions_shape() {
        let request: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/openai_chat/request_with_tools.json"
        ))
        .unwrap();
        let converted = request_from_responses(&request).unwrap();
        assert_eq!(converted["tools"][0]["type"], "function");
        assert_eq!(converted["tools"][0]["function"]["name"], "inspect");
        assert_eq!(
            converted["tools"][0]["function"]["parameters"]["required"][0],
            "path"
        );
        assert_eq!(converted["tools"][0]["function"]["strict"], true);
        assert_eq!(converted["tool_choice"]["function"]["name"], "inspect");
    }

    #[test]
    fn premature_stream_eof_does_not_synthesize_completion() {
        let mut translator = ChatTranslator::new("chat-model");
        let events = translator.push(
            b"data: {\"id\":\"chat-1\",\"choices\":[{\"delta\":{\"content\":\"partial\"}}]}\n\n",
        );
        assert!(
            events
                .iter()
                .all(|event| event["type"] != "response.completed")
        );
        assert!(translator.finish().is_empty());
    }

    #[test]
    fn streaming_utf8_and_tool_deltas_survive_fragmented_chunks() {
        let mut translator = ChatTranslator::new("chat-model");
        let stream =
            include_bytes!("../../tests/fixtures/openai_chat/fragmented_utf8_tool_calls.sse");
        let character = "你".as_bytes();
        let offset = stream
            .windows(character.len())
            .position(|window| window == character)
            .unwrap();
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
}
