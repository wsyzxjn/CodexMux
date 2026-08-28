use serde_json::Value;

pub fn encode(event: &Value) -> Vec<u8> {
    let name = event
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("message");
    format!("event: {name}\ndata: {event}\n\n").into_bytes()
}

pub fn frames(buffer: &mut Vec<u8>, incoming: &[u8]) -> Vec<Vec<u8>> {
    buffer.extend_from_slice(incoming);
    let mut output = Vec::new();
    while let Some(end) = frame_end(buffer) {
        output.push(buffer.drain(..end).collect());
    }
    output
}

pub fn data(frame: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(frame).ok()?;
    let data = text
        .lines()
        .filter_map(|line| line.strip_prefix("data:").map(str::trim_start))
        .collect::<Vec<_>>()
        .join("\n");
    (!data.is_empty()).then_some(data)
}

fn frame_end(buffer: &[u8]) -> Option<usize> {
    let lf = find(buffer, b"\n\n").map(|index| index + 2);
    let crlf = find(buffer, b"\r\n\r\n").map(|index| index + 4);
    match (lf, crlf) {
        (Some(left), Some(right)) => Some(left.min(right)),
        (left, right) => left.or(right),
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waits_for_complete_frames() {
        let mut buffer = Vec::new();
        assert!(frames(&mut buffer, b"data: {\"a\":").is_empty());
        let frames = frames(&mut buffer, b"1}\n\n");
        assert_eq!(frames.len(), 1);
        assert_eq!(data(&frames[0]).as_deref(), Some("{\"a\":1}"));
    }

    #[test]
    fn joins_multiple_data_lines() {
        assert_eq!(
            data(b"data: {\"a\":\ndata: 1}\n\n").as_deref(),
            Some("{\"a\":\n1}")
        );
    }
}
