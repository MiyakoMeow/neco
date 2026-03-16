//! Message domain model module.

use chrono::Utc;
use neoco_core::ids::MessageId;

pub use neoco_core::messages::{Message, MessageMetadata, ModelMessage, Role, ToolCall};

/// Builder for constructing messages.
pub struct MessageBuilder {
    /// Optional message ID.
    id: Option<MessageId>,
    /// Message role.
    role: Role,
    /// Message content.
    content: String,
    /// Optional tool calls.
    tool_calls: Option<Vec<ToolCall>>,
    /// Optional tool call ID.
    tool_call_id: Option<String>,
}

impl MessageBuilder {
    /// Creates a new message builder with the given role.
    #[must_use]
    pub fn new(role: Role) -> Self {
        Self {
            id: None,
            role,
            content: String::new(),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    /// Sets the message ID.
    #[must_use]
    pub fn id(mut self, id: MessageId) -> Self {
        self.id = Some(id);
        self
    }

    /// Sets the message content.
    #[must_use]
    pub fn content(mut self, content: impl Into<String>) -> Self {
        self.content = content.into();
        self
    }

    /// Sets tool calls for the message.
    #[must_use]
    pub fn tool_calls(mut self, calls: Vec<ToolCall>) -> Self {
        self.tool_calls = Some(calls);
        self
    }

    /// Sets the tool call ID.
    #[must_use]
    pub fn tool_call_id(mut self, id: impl Into<String>) -> Self {
        self.tool_call_id = Some(id.into());
        self
    }

    /// Builds the message.
    ///
    /// # Panics
    ///
    /// Panics if `id` was not set before calling build.
    #[must_use]
    pub fn build(self) -> Message {
        Message {
            id: self.id.unwrap_or_default(),
            role: self.role,
            content: self.content,
            tool_calls: self.tool_calls,
            tool_call_id: self.tool_call_id,
            timestamp: Utc::now(),
            metadata: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_builder_basic() {
        let msg = MessageBuilder::new(Role::User)
            .id(MessageId::new())
            .content("Hello world")
            .build();

        assert_eq!(msg.role, Role::User);
        assert_eq!(msg.content, "Hello world");
    }

    #[test]
    fn test_message_builder_with_id() {
        let id = neoco_core::ids::MessageId::new();
        let msg = MessageBuilder::new(Role::Assistant)
            .id(id)
            .content("Response")
            .build();

        assert_eq!(msg.id, id);
    }

    #[test]
    fn test_message_builder_without_id_uses_default() {
        let msg = MessageBuilder::new(Role::User).content("Test").build();
        assert_eq!(msg.id, MessageId::default());
        assert_eq!(msg.role, Role::User);
        assert_eq!(msg.content, "Test");
    }

    #[test]
    fn test_message_builder_with_tool_calls() {
        let tool_calls = vec![ToolCall {
            id: "call_1".to_string(),
            tool_name: neoco_core::ids::ToolId::from_string("fs::read").unwrap(),
            arguments: r#"{"path": "/test"}"#.to_string(),
        }];

        let msg = MessageBuilder::new(Role::Assistant)
            .id(neoco_core::ids::MessageId::new())
            .content("Using tool")
            .tool_calls(tool_calls)
            .build();

        assert!(msg.tool_calls.is_some());
    }

    #[test]
    fn test_message_builder_with_tool_call_id() {
        let msg = MessageBuilder::new(Role::Tool)
            .id(neoco_core::ids::MessageId::new())
            .content("Tool result")
            .tool_call_id("call_1")
            .build();

        assert_eq!(msg.tool_call_id, Some("call_1".to_string()));
    }

    #[test]
    fn test_core_message_usage() {
        let msg = Message::user("Hello from core");
        assert_eq!(msg.role, Role::User);
    }

    #[test]
    fn test_model_message_from_message() {
        let msg = Message::user("Test content");
        let model_msg = ModelMessage::from_message(&msg);
        assert_eq!(model_msg.role, Role::User);
    }
}
