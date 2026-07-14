//! Tool call parsing from model output.
//!
//! Extracts `<tool_call>` JSON blocks from assistant responses so the agent
//! can dispatch them. Also extracts `<think>` reasoning blocks.

use serde_json::Value;

/// A parsed tool call extracted from model output.
#[derive(Debug, Clone)]
pub struct ToolCall {
    /// The tool name (from the JSON `name` field).
    pub name: String,
    /// The arguments as a JSON Value.
    pub arguments: Value,
    /// The raw JSON string that was parsed.
    pub raw: String,
}

/// A segment of assistant output.
#[derive(Debug, Clone)]
pub enum AssistantSegment {
    /// Free-text reasoning or speech.
    Text(String),
    /// A `<think>...</think>` reasoning block.
    Think(String),
    /// A `<tool_call>...</tool_call>` block.
    ToolCall(ToolCall),
    /// A `<tool_result>...</tool_result>` block (model-generated, if present).
    ToolResult(String),
}

/// Extract all tool calls from an assistant response string.
///
/// Looks for `<tool_call>{"name": "...", "arguments": {...}}</tool_call>`
/// patterns and returns any found.
pub fn extract_tool_calls(text: &str) -> Vec<ToolCall> {
    let mut calls = Vec::new();
    let mut pos = 0;
    let start_tag = "<tool_call>";
    let end_tag = "</tool_call>";

    while let Some(start) = text[pos..].find(start_tag) {
        let abs_start = pos + start;
        let after_start = abs_start + start_tag.len();
        if let Some(end) = text[after_start..].find(end_tag) {
            let abs_end = after_start + end;
            let json_str = &text[after_start..abs_end];
            if let Ok(json) = serde_json::from_str::<Value>(json_str) {
                let name = json.get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                let arguments = json.get("arguments")
                    .cloned()
                    .unwrap_or(Value::Null);
                calls.push(ToolCall {
                    name,
                    arguments,
                    raw: json_str.to_string(),
                });
            }
            pos = abs_end + end_tag.len();
        } else {
            break;
        }
    }

    calls
}

/// Parse an assistant response into segments (text, think, tool_call, tool_result).
pub fn parse_assistant_response(text: &str) -> Vec<AssistantSegment> {
    let mut segments = Vec::new();
    let mut pos = 0;
    let tags = ["<think>", "</think>", "<tool_call>", "</tool_call>", "<tool_result>", "</tool_result>"];

    while pos < text.len() {
        // Find the next tag occurrence
        let mut next_tag_pos = text.len();
        let mut next_tag = "";
        for &tag in &tags {
            if let Some(tp) = text[pos..].find(tag) {
                let abs_tp = pos + tp;
                if abs_tp < next_tag_pos {
                    next_tag_pos = abs_tp;
                    next_tag = tag;
                }
            }
        }

        // Collect text up to the next tag
        if next_tag_pos > pos {
            segments.push(AssistantSegment::Text(text[pos..next_tag_pos].to_string()));
            pos = next_tag_pos;
        }

        if next_tag.is_empty() {
            break;
        }

        match next_tag {
            "<think>" => {
                let end = "</think>";
                if let Some(close) = text[pos + 7..].find(end) {
                    let abs_close = pos + 7 + close;
                    let content = text[pos + 7..abs_close].to_string();
                    segments.push(AssistantSegment::Think(content));
                    pos = abs_close + end.len();
                } else {
                    segments.push(AssistantSegment::Text(text[pos..].to_string()));
                    break;
                }
            }
            "<tool_call>" => {
                let end = "</tool_call>";
                // <tool_call> is 11 chars
                if let Some(close) = text[pos + 11..].find(end) {
                    let abs_close = pos + 11 + close;
                    let json_str = &text[pos + 11..abs_close];
                    if let Ok(json) = serde_json::from_str::<Value>(json_str) {
                        let name = json.get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown")
                            .to_string();
                        let arguments = json.get("arguments").cloned().unwrap_or(Value::Null);
                        segments.push(AssistantSegment::ToolCall(ToolCall {
                            name,
                            arguments,
                            raw: json_str.to_string(),
                        }));
                    } else {
                        segments.push(AssistantSegment::Text(text[pos..abs_close + end.len()].to_string()));
                    }
                    pos = abs_close + end.len();
                } else {
                    segments.push(AssistantSegment::Text(text[pos..].to_string()));
                    break;
                }
            }
            "<tool_result>" => {
                let end = "</tool_result>";
                if let Some(close) = text[pos + 13..].find(end) {
                    let abs_close = pos + 13 + close;
                    let content = text[pos + 13..abs_close].to_string();
                    segments.push(AssistantSegment::ToolResult(content));
                    pos = abs_close + end.len();
                } else {
                    segments.push(AssistantSegment::Text(text[pos..].to_string()));
                    break;
                }
            }
            _ => {
                // Unknown tag, skip it
                // Actually this covers closing tags too
                let tag_end = text[pos..].find('>').map(|i| pos + i + 1).unwrap_or(text.len());
                segments.push(AssistantSegment::Text(text[pos..tag_end].to_string()));
                pos = tag_end;
            }
        }
    }

    segments
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_tool_call_simple() {
        let text = r#"Some text <tool_call>{"name": "read", "arguments": {"path": "file.txt"}}</tool_call> more text"#;
        let calls = extract_tool_calls(text);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "read");
        assert_eq!(calls[0].arguments["path"], "file.txt");
    }

    #[test]
    fn extract_multiple_tool_calls() {
        let text = r#"<tool_call>{"name": "read", "arguments": {"path": "a.txt"}}</tool_call><tool_call>{"name": "write", "arguments": {"path": "b.txt", "content": "hello"}}</tool_call>"#;
        let calls = extract_tool_calls(text);
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].name, "read");
        assert_eq!(calls[1].name, "write");
    }

    #[test]
    fn extract_no_calls() {
        let text = "Just some regular text without tool calls.";
        let calls = extract_tool_calls(text);
        assert!(calls.is_empty());
    }

    #[test]
    fn parse_assistant_response_mixed() {
        let text = r#"Hello <think>Let me check</think> The answer is <tool_call>{"name": "read", "arguments": {"path": "x"}}</tool_call> Done."#;
        let segments = parse_assistant_response(text);
        assert!(segments.len() >= 1);
        // The first segment should be text or think
        let has_think = segments.iter().any(|s| matches!(s, AssistantSegment::Think(_)));
        let has_tool = segments.iter().any(|s| matches!(s, AssistantSegment::ToolCall(_)));
        assert!(has_think, "should have a think segment");
        assert!(has_tool, "should have a tool_call segment");
    }

    #[test]
    fn empty_text_returns_no_segments() {
        let segments = parse_assistant_response("");
        assert!(segments.is_empty());
    }

    #[test]
    fn malformed_json_in_tool_call() {
        let text = r#"<tool_call>{"name": "read", "arguments": {broken}}</tool_call>"#;
        let calls = extract_tool_calls(text);
        assert!(calls.is_empty(), "malformed JSON yields no calls");
    }
}
