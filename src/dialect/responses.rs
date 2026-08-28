use serde_json::Value;

/// Native Responses providers receive the parsed body only when continuity had
/// to rewrite it. Otherwise the server forwards the original bytes.
pub fn validate_request(value: &Value) -> anyhow::Result<()> {
    anyhow::ensure!(value.is_object(), "Responses request must be a JSON object");
    anyhow::ensure!(
        value.get("model").and_then(Value::as_str).is_some(),
        "Responses request is missing model"
    );
    Ok(())
}
