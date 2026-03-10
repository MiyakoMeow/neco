//! Unified message system for Session and Model layers.

use crate::ids::{MessageId, ToolId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::fmt;

/// Message role in conversation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// System message.
    System,
    /// User message.
    User,
    /// Assistant message.
    Assistant,
    /// Tool result message.
    Tool,
}

impl Role {
    /// Get string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            Role::System => "system",
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::Tool => "tool",
        }
    }
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for Role {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "system" => Ok(Role::System),
            "user" => Ok(Role::User),
            "assistant" => Ok(Role::Assistant),
            "tool" => Ok(Role::Tool),
            _ => Err(format!("unknown role: {}", s)),
        }
    }
}

/// Tool call from model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    /// Tool call ID.
    pub id: String,
    /// Tool name.
    pub tool_name: ToolId,
    /// Tool arguments as JSON string.
    pub arguments: String,
}

/// Message metadata for tracking and observability.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageMetadata {
    /// Model used for this message.
    pub model: Option<String>,
    /// Temperature setting used.
    pub temperature: Option<f32>,
    /// Token usage information.
    pub tokens: Option<MessageTokens>,
}

/// Token usage breakdown.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageTokens {
    /// Prompt tokens used.
    pub prompt_tokens: Option<u32>,
    /// Completion tokens generated.
    pub completion_tokens: Option<u32>,
    /// Total tokens used.
    pub total_tokens: Option<u32>,
}

/// Domain message with ID (used in Session layer).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Unique message ID.
    pub id: MessageId,
    /// Message role.
    pub role: Role,
    /// Message content.
    pub content: String,
    /// Tool calls from assistant.
    pub tool_calls: Option<Vec<ToolCall>>,
    /// Tool call ID for tool results.
    pub tool_call_id: Option<String>,
    /// Timestamp when message was created.
    pub timestamp: DateTime<Utc>,
    /// Additional metadata.
    pub metadata: Option<MessageMetadata>,
}

impl Message {
    /// Create a new message with given role and content.
    pub fn new(role: Role, content: String) -> Self {
        Self {
            id: MessageId::new(),
            role,
            content,
            tool_calls: None,
            tool_call_id: None,
            timestamp: Utc::now(),
            metadata: None,
        }
    }

    /// Create a system message.
    pub fn system(content: impl Into<String>) -> Self {
        Self::new(Role::System, content.into())
    }

    /// Create a user message.
    pub fn user(content: impl Into<String>) -> Self {
        Self::new(Role::User, content.into())
    }

    /// Create an assistant message.
    pub fn assistant(content: impl Into<String>) -> Self {
        Self::new(Role::Assistant, content.into())
    }

    /// Create a tool result message.
    pub fn tool(content: impl Into<String>, tool_call_id: impl Into<String>) -> Self {
        Self {
            id: MessageId::new(),
            role: Role::Tool,
            content: content.into(),
            tool_calls: None,
            tool_call_id: Some(tool_call_id.into()),
            timestamp: Utc::now(),
            metadata: None,
        }
    }

    /// Add tool calls to message.
    pub fn with_tool_calls(mut self, tool_calls: Vec<ToolCall>) -> Self {
        self.tool_calls = Some(tool_calls);
        self
    }

    /// Add metadata to message.
    pub fn with_metadata(mut self, metadata: MessageMetadata) -> Self {
        self.metadata = Some(metadata);
        self
    }
}

/// Model message without ID (used in Model layer for LLM calls).
#[derive(Debug, Clone)]
pub struct ModelMessage<'a> {
    /// Message role.
    pub role: Role,
    /// Message content.
    pub content: Cow<'a, str>,
    /// Tool calls.
    pub tool_calls: Option<Cow<'a, [ToolCall]>>,
    /// Tool call ID.
    pub tool_call_id: Option<Cow<'a, str>>,
}

impl<'a> ModelMessage<'a> {
    /// Create from a domain message.
    pub fn from_message(msg: &'a Message) -> Self {
        Self {
            role: msg.role,
            content: Cow::Borrowed(&msg.content),
            tool_calls: msg
                .tool_calls
                .as_ref()
                .map(|tc| Cow::Borrowed(tc.as_slice())),
            tool_call_id: msg
                .tool_call_id
                .as_ref()
                .map(|id| Cow::Borrowed(id.as_str())),
        }
    }

    /// Convert to owned version.
    pub fn into_owned(self) -> OwnedModelMessage {
        OwnedModelMessage {
            role: self.role,
            content: self.content.into_owned(),
            tool_calls: self.tool_calls.map(|tc| tc.into_owned()),
            tool_call_id: self.tool_call_id.map(|id| id.into_owned()),
        }
    }
}

/// Owned version of ModelMessage.
pub struct OwnedModelMessage {
    /// Message role.
    pub role: Role,
    /// Message content.
    pub content: String,
    /// Tool calls.
    pub tool_calls: Option<Vec<ToolCall>>,
    /// Tool call ID.
    pub tool_call_id: Option<String>,
}

impl OwnedModelMessage {
    /// Convert to domain message with given ID.
    pub fn to_message(&self, id: MessageId) -> Message {
        Message {
            id,
            role: self.role,
            content: self.content.clone(),
            tool_calls: self.tool_calls.clone(),
            tool_call_id: self.tool_call_id.clone(),
            timestamp: Utc::now(),
            metadata: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_creation() {
        let msg = Message::user("Hello");
        assert_eq!(msg.role, Role::User);
        assert_eq!(msg.content, "Hello");
    }

    #[test]
    fn test_message_system() {
        let msg = Message::system("You are helpful");
        assert_eq!(msg.role, Role::System);
    }

    #[test]
    fn test_message_assistant() {
        let msg = Message::assistant("Response");
        assert_eq!(msg.role, Role::Assistant);
    }

    #[test]
    fn test_message_tool() {
        let msg = Message::tool("tool result", "call_123");
        assert_eq!(msg.role, Role::Tool);
        assert_eq!(msg.tool_call_id, Some("call_123".to_string()));
    }

    #[test]
    fn test_message_with_tool_calls() {
        let tool_calls = vec![ToolCall {
            id: "call_1".to_string(),
            tool_name: ToolId::from_string("fs::read").unwrap(),
            arguments: r#"{"path": "/test"}"#.to_string(),
        }];
        let msg = Message::assistant("Using tool").with_tool_calls(tool_calls);
        assert!(msg.tool_calls.is_some());
    }

    #[test]
    fn test_model_message_from_message() {
        let msg = Message::user("Hello");
        let model_msg = ModelMessage::from_message(&msg);
        assert_eq!(model_msg.role, Role::User);
        assert_eq!(model_msg.content.as_ref(), "Hello");
    }

    #[test]
    fn test_owned_model_message_roundtrip() {
        let msg = Message::user("Test content");
        let model_msg = ModelMessage::from_message(&msg);
        let owned = model_msg.into_owned();
        let back = owned.to_message(MessageId::new());
        assert_eq!(back.content, "Test content");
    }

    #[test]
    fn test_role_display() {
        assert_eq!(Role::System.as_str(), "system");
        assert_eq!(Role::User.as_str(), "user");
        assert_eq!(Role::Assistant.as_str(), "assistant");
        assert_eq!(Role::Tool.as_str(), "tool");
    }

    #[test]
    fn test_role_from_str() {
        assert_eq!("system".parse::<Role>().unwrap(), Role::System);
        assert_eq!("USER".parse::<Role>().unwrap(), Role::User);
        "invalid".parse::<Role>().unwrap_err();
    }
}
