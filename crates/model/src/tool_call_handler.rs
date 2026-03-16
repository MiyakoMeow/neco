//! Tool call handler for parsing and building tool-related messages.

use crate::types::{ChatResponse, Message, ToolCall};

/// Handler for tool calls.
pub struct ToolCallHandler;

impl ToolCallHandler {
    /// Parses tool calls from a chat response.
    pub fn parse_tool_calls(response: &ChatResponse) -> Vec<ToolCall> {
        response
            .choices
            .first()
            .and_then(|choice| choice.message.tool_calls.clone())
            .unwrap_or_default()
    }

    /// Builds a tool message from the result.
    pub fn build_tool_message(tool_call_id: &str, result: &str) -> Message {
        Message {
            role: "tool".to_string(),
            content: result.to_string(),
            tool_calls: None,
            tool_call_id: Some(tool_call_id.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_tool_calls() {
        let response = ChatResponse {
            id: "test".to_string(),
            model: "test-model".to_string(),
            choices: vec![crate::types::Choice {
                index: 0,
                message: Message {
                    role: "assistant".to_string(),
                    content: String::new(),
                    tool_calls: Some(vec![ToolCall {
                        id: "call_123".to_string(),
                        name: "get_weather".to_string(),
                        arguments: serde_json::json!({"city": "Beijing"}),
                    }]),
                    tool_call_id: None,
                },
                finish_reason: Some("tool_calls".to_string()),
            }],
            usage: crate::types::Usage {
                prompt_tokens: 10,
                completion_tokens: 20,
                total_tokens: 30,
            },
        };

        let tool_calls = ToolCallHandler::parse_tool_calls(&response);
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0].id, "call_123");
        assert_eq!(tool_calls[0].name, "get_weather");
    }

    #[test]
    fn test_parse_tool_calls_empty() {
        let response = ChatResponse {
            id: "test".to_string(),
            model: "test-model".to_string(),
            choices: vec![crate::types::Choice {
                index: 0,
                message: Message {
                    role: "assistant".to_string(),
                    content: "Hello".to_string(),
                    tool_calls: None,
                    tool_call_id: None,
                },
                finish_reason: Some("stop".to_string()),
            }],
            usage: crate::types::Usage {
                prompt_tokens: 10,
                completion_tokens: 5,
                total_tokens: 15,
            },
        };

        let tool_calls = ToolCallHandler::parse_tool_calls(&response);
        assert!(tool_calls.is_empty());
    }

    #[test]
    fn test_build_tool_message() {
        let msg = ToolCallHandler::build_tool_message("call_123", "Weather is sunny");
        assert_eq!(msg.role, "tool");
        assert_eq!(msg.content, "Weather is sunny");
        assert_eq!(msg.tool_call_id, Some("call_123".to_string()));
    }
}
