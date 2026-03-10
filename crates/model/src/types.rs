//! Core type definitions for model layer

use neoco_core::Role;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::HashMap;

/// Capabilities of a model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCapabilities {
    /// Whether the model supports streaming.
    pub streaming: bool,
    /// Whether the model supports tools.
    pub tools: bool,
    /// Whether the model supports functions.
    pub functions: bool,
    /// Whether the model supports JSON mode.
    pub json_mode: bool,
    /// Whether the model supports vision.
    pub vision: bool,
    /// Context window size.
    pub context_window: usize,
}

impl Default for ModelCapabilities {
    fn default() -> Self {
        Self {
            streaming: true,
            tools: false,
            functions: false,
            json_mode: false,
            vision: false,
            context_window: 4096,
        }
    }
}

/// Chat request to a model.
#[derive(Debug, Clone)]
pub struct ChatRequest<'a> {
    /// The model to use.
    pub model: String,
    /// Messages to send.
    pub messages: Vec<ModelMessage<'a>>,
    /// Whether to stream the response.
    pub stream: bool,
    /// Temperature setting.
    pub temperature: Option<f64>,
    /// Maximum completion tokens.
    pub max_completion_tokens: Option<u32>,
    /// Maximum tokens.
    pub max_tokens: Option<u32>,
    /// Frequency penalty.
    pub frequency_penalty: Option<f64>,
    /// Presence penalty.
    pub presence_penalty: Option<f64>,
    /// Top P sampling.
    pub top_p: Option<f64>,
    /// Random seed.
    pub seed: Option<i64>,
    /// Tools available to the model.
    pub tools: Option<Vec<ToolDefinition>>,
    /// Tool choice.
    pub tool_choice: Option<ToolChoice>,
    /// Response format.
    pub response_format: Option<ResponseFormat>,
    /// Stop sequences.
    pub stop: Option<Vec<String>>,
    /// Thinking setting.
    pub thinking: Option<bool>,
    /// Extra parameters.
    pub extra_params: ExtraParams,
}

impl<'a> ChatRequest<'a> {
    /// Creates a new ChatRequest.
    pub fn new(model: impl Into<String>, messages: Vec<ModelMessage<'a>>) -> Self {
        Self {
            model: model.into(),
            messages,
            stream: false,
            temperature: None,
            max_completion_tokens: None,
            max_tokens: None,
            frequency_penalty: None,
            presence_penalty: None,
            top_p: None,
            seed: None,
            tools: None,
            tool_choice: None,
            response_format: None,
            stop: None,
            thinking: None,
            extra_params: ExtraParams::new(),
        }
    }

    /// Sets streaming mode.
    pub fn with_stream(mut self, stream: bool) -> Self {
        self.stream = stream;
        self
    }

    /// Sets temperature.
    pub fn with_temperature(mut self, temperature: f64) -> Self {
        self.temperature = Some(temperature);
        self
    }

    /// Sets max tokens.
    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = Some(max_tokens);
        self
    }

    /// Sets max completion tokens.
    pub fn with_max_completion_tokens(mut self, max_tokens: u32) -> Self {
        self.max_completion_tokens = Some(max_tokens);
        self
    }

    /// Sets tools.
    pub fn with_tools(mut self, tools: Option<Vec<ToolDefinition>>) -> Self {
        self.tools = tools;
        self
    }

    /// Sets thinking mode.
    pub fn with_thinking(mut self, thinking: bool) -> Self {
        self.thinking = Some(thinking);
        self
    }

    /// Sets tool choice.
    pub fn with_tool_choice(mut self, tool_choice: ToolChoice) -> Self {
        self.tool_choice = Some(tool_choice);
        self
    }

    /// Sets response format.
    pub fn with_response_format(mut self, response_format: ResponseFormat) -> Self {
        self.response_format = Some(response_format);
        self
    }

    /// Gets effective max tokens.
    pub fn effective_max_tokens(&self) -> Option<u32> {
        self.max_completion_tokens.or(self.max_tokens)
    }
}

/// Message to send to a model.
#[derive(Debug, Clone)]
pub struct ModelMessage<'a> {
    /// The role of the message sender.
    pub role: Role,
    /// The content of the message.
    pub content: Cow<'a, str>,
    /// Tool calls included in the message.
    pub tool_calls: Option<Cow<'a, [ToolCall]>>,
    /// ID of the tool call this message is responding to.
    pub tool_call_id: Option<Cow<'a, str>>,
}

impl<'a> ModelMessage<'a> {
    /// Creates a user message.
    pub fn user(content: impl Into<Cow<'a, str>>) -> Self {
        Self {
            role: Role::User,
            content: content.into(),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    /// Creates an assistant message.
    pub fn assistant(content: impl Into<Cow<'a, str>>, tool_calls: Option<&'a [ToolCall]>) -> Self {
        Self {
            role: Role::Assistant,
            content: content.into(),
            tool_calls: tool_calls.map(Cow::Borrowed),
            tool_call_id: None,
        }
    }

    /// Creates a tool message.
    pub fn tool(content: impl Into<Cow<'a, str>>, tool_call_id: &'a str) -> Self {
        Self {
            role: Role::Tool,
            content: content.into(),
            tool_calls: None,
            tool_call_id: Some(Cow::Borrowed(tool_call_id)),
        }
    }

    /// Creates a system message.
    pub fn system(content: impl Into<Cow<'a, str>>) -> Self {
        Self {
            role: Role::System,
            content: content.into(),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    /// Validates the message.
    pub fn validate(&self) -> Result<(), MessageValidationError> {
        match self.role {
            Role::System | Role::User => {
                if self.tool_calls.is_some() {
                    return Err(MessageValidationError::InvalidFieldForRole {
                        role: self.role.as_str().to_string(),
                        field: "tool_calls".to_string(),
                    });
                }
                if self.tool_call_id.is_some() {
                    return Err(MessageValidationError::InvalidFieldForRole {
                        role: self.role.as_str().to_string(),
                        field: "tool_call_id".to_string(),
                    });
                }
            },
            Role::Assistant => {
                let has_content = !self.content.is_empty();
                let has_tool_calls = self.tool_calls.is_some();
                if !has_content && !has_tool_calls {
                    return Err(MessageValidationError::AssistantMissingContentOrToolCalls);
                }
                if self.tool_call_id.is_some() {
                    return Err(MessageValidationError::InvalidFieldForRole {
                        role: "assistant".to_string(),
                        field: "tool_call_id".to_string(),
                    });
                }
            },
            Role::Tool => {
                if self.tool_calls.is_some() {
                    return Err(MessageValidationError::InvalidFieldForRole {
                        role: "tool".to_string(),
                        field: "tool_calls".to_string(),
                    });
                }
                if self.tool_call_id.is_none() {
                    return Err(MessageValidationError::ToolMissingToolCallId);
                }
            },
        }
        Ok(())
    }
}

/// Error when validating a message.
#[derive(Debug, Clone)]
pub enum MessageValidationError {
    /// Invalid field for the given role.
    InvalidFieldForRole {
        /// The role.
        role: String,
        /// The field name.
        field: String,
    },
    /// Assistant message missing content or tool calls.
    AssistantMissingContentOrToolCalls,
    /// Tool message missing tool call ID.
    ToolMissingToolCallId,
}

impl std::fmt::Display for MessageValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidFieldForRole { role, field } => {
                write!(f, "field '{field}' is not valid for role '{role}'")
            },
            Self::AssistantMissingContentOrToolCalls => {
                write!(f, "assistant message must have content or tool_calls")
            },
            Self::ToolMissingToolCallId => {
                write!(f, "tool message must have tool_call_id")
            },
        }
    }
}

impl std::error::Error for MessageValidationError {}

/// A tool call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    /// The ID of the tool call.
    pub id: String,
    /// The name of the tool.
    pub name: String,
    /// The arguments to the tool.
    pub arguments: serde_json::Value,
}

/// Definition of a tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// The name of the tool.
    pub name: String,
    /// The description of the tool.
    pub description: String,
    /// The parameters of the tool.
    pub parameters: serde_json::Value,
}

/// Choice of tools to use.
#[derive(Debug, Clone)]
pub enum ToolChoice {
    /// Let the model decide.
    Auto,
    /// No tools.
    None,
    /// Use a specific function.
    Function {
        /// The function name.
        name: String,
    },
}

impl ToolChoice {
    /// Converts to a JSON value.
    pub fn to_value(&self) -> serde_json::Value {
        match self {
            Self::Auto => serde_json::Value::String("auto".to_string()),
            Self::None => serde_json::Value::String("none".to_string()),
            Self::Function { name } => {
                serde_json::json!({ "type": "function", "function": { "name": name } })
            },
        }
    }
}

/// Response format to use.
#[derive(Debug, Clone)]
pub enum ResponseFormat {
    /// Plain text.
    Text,
    /// JSON object.
    JsonObject,
    /// JSON schema.
    JsonSchema {
        /// The JSON schema.
        schema: serde_json::Value,
    },
}

impl ResponseFormat {
    /// Converts to a JSON value.
    pub fn to_value(&self) -> serde_json::Value {
        match self {
            Self::Text => serde_json::Value::String("text".to_string()),
            Self::JsonObject => serde_json::Value::String("json_object".to_string()),
            Self::JsonSchema { schema } => serde_json::json!({ "json_schema": schema }),
        }
    }
}

/// Extra parameters for requests.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExtraParams(HashMap<String, serde_json::Value>);

impl ExtraParams {
    /// Creates new ExtraParams.
    pub fn new() -> Self {
        Self(HashMap::new())
    }

    /// Inserts a parameter.
    pub fn insert(&mut self, key: impl Into<String>, value: serde_json::Value) {
        self.0.insert(key.into(), value);
    }

    /// Gets a parameter.
    pub fn get(&self, key: &str) -> Option<&serde_json::Value> {
        self.0.get(key)
    }
}

impl std::ops::Deref for ExtraParams {
    type Target = HashMap<String, serde_json::Value>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Chat response from a model.
#[derive(Debug, Clone)]
pub struct ChatResponse {
    /// The response ID.
    pub id: String,
    /// The model used.
    pub model: String,
    /// The choices returned.
    pub choices: Vec<Choice>,
    /// Token usage information.
    pub usage: Usage,
}

/// A choice in the response.
#[derive(Debug, Clone)]
pub struct Choice {
    /// The index of the choice.
    pub index: usize,
    /// The message content.
    pub message: Message,
    /// Why the response finished.
    pub finish_reason: Option<String>,
}

/// Token usage information.
#[derive(Debug, Clone)]
pub struct Usage {
    /// Tokens in the prompt.
    pub prompt_tokens: u32,
    /// Tokens in the completion.
    pub completion_tokens: u32,
    /// Total tokens used.
    pub total_tokens: u32,
}

/// A message from the model.
#[derive(Debug, Clone)]
pub struct Message {
    /// The role of the message sender.
    pub role: String,
    /// The content of the message.
    pub content: String,
    /// Tool calls in the message.
    pub tool_calls: Option<Vec<ToolCall>>,
    /// ID of the tool call this is responding to.
    pub tool_call_id: Option<String>,
}

/// A chunk in a streaming response.
#[derive(Debug, Clone)]
pub struct ChatChunk {
    /// The chunk ID.
    pub id: String,
    /// The choices in this chunk.
    pub choices: Vec<ChunkChoice>,
}

/// A choice in a streaming response.
#[derive(Debug, Clone)]
pub struct ChunkChoice {
    /// The index of the choice.
    pub index: usize,
    /// The delta content.
    pub delta: Delta,
    /// Why this chunk finished.
    pub finish_reason: Option<String>,
}

/// Delta content in a streaming response.
#[derive(Debug, Clone, Default)]
pub struct Delta {
    /// The role, if present.
    pub role: Option<String>,
    /// The content, if present.
    pub content: Option<String>,
    /// Tool calls, if present.
    pub tool_calls: Option<Vec<ToolCall>>,
}

/// Reference to a model.
#[derive(Debug, Clone)]
pub struct ModelRef {
    /// The model name.
    pub name: String,
    /// The provider name.
    pub provider: String,
}

/// Retry configuration.
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retries.
    pub max_retries: u32,
    /// Initial delay in milliseconds.
    pub initial_delay_ms: u64,
    /// Maximum delay in milliseconds.
    pub max_delay_ms: u64,
    /// Backoff multiplier.
    pub backoff_multiplier: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_delay_ms: 1000,
            max_delay_ms: 4000,
            backoff_multiplier: 2.0,
        }
    }
}

impl RetryConfig {
    /// Calculates the delay for a given attempt.
    pub fn calculate_delay(&self, attempt: u32) -> u64 {
        let delay = self.initial_delay_ms as f64 * self.backoff_multiplier.powi(attempt as i32);
        delay.min(self.max_delay_ms as f64) as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chat_request_effective_max_tokens_prefers_completion() {
        let req = ChatRequest::new("test-model", vec![])
            .with_max_tokens(100)
            .with_max_completion_tokens(200);

        assert_eq!(req.effective_max_tokens(), Some(200));
    }

    #[test]
    fn test_chat_request_effective_max_tokens_fallback() {
        let req = ChatRequest::new("test-model", vec![]).with_max_tokens(100);

        assert_eq!(req.effective_max_tokens(), Some(100));
    }

    #[test]
    fn test_retry_config_calculate_delay() {
        let config = RetryConfig::default();

        assert_eq!(config.calculate_delay(0), 1000);
        assert_eq!(config.calculate_delay(1), 2000);
        assert_eq!(config.calculate_delay(2), 4000);
    }

    #[test]
    fn test_retry_config_max_delay() {
        let config = RetryConfig {
            max_retries: 3,
            initial_delay_ms: 1000,
            max_delay_ms: 3000,
            backoff_multiplier: 2.0,
        };

        assert_eq!(config.calculate_delay(0), 1000);
        assert_eq!(config.calculate_delay(1), 2000);
        assert_eq!(config.calculate_delay(2), 3000);
        assert_eq!(config.calculate_delay(3), 3000);
    }

    #[test]
    fn test_tool_choice_to_value() {
        assert_eq!(
            ToolChoice::Auto.to_value(),
            serde_json::Value::String("auto".to_string())
        );
        assert_eq!(
            ToolChoice::None.to_value(),
            serde_json::Value::String("none".to_string())
        );
        assert_eq!(
            ToolChoice::Function {
                name: "test".to_string()
            }
            .to_value(),
            serde_json::json!({ "type": "function", "function": { "name": "test" } })
        );
    }

    #[test]
    fn test_response_format_to_value() {
        assert_eq!(
            ResponseFormat::Text.to_value(),
            serde_json::Value::String("text".to_string())
        );
        assert_eq!(
            ResponseFormat::JsonObject.to_value(),
            serde_json::Value::String("json_object".to_string())
        );
        assert_eq!(
            ResponseFormat::JsonSchema {
                schema: serde_json::json!({"type": "object"})
            }
            .to_value(),
            serde_json::json!({ "json_schema": {"type": "object"} })
        );
    }

    #[test]
    fn test_model_capabilities_default() {
        let caps = ModelCapabilities::default();
        assert!(caps.streaming);
        assert!(!caps.tools);
        assert!(!caps.functions);
    }

    #[test]
    fn test_extra_params() {
        let mut params = ExtraParams::new();
        params.insert("key1", serde_json::json!("value1"));
        params.insert("key2", serde_json::json!(42));

        assert_eq!(params.get("key1"), Some(&serde_json::json!("value1")));
        assert_eq!(params.get("key2"), Some(&serde_json::json!(42)));
        assert!(params.get("nonexistent").is_none());
    }

    #[test]
    fn test_model_message_validate_user() {
        let msg = ModelMessage::user("Hello");
        msg.validate().unwrap();
    }

    #[test]
    fn test_model_message_validate_system() {
        let msg = ModelMessage::system("You are helpful");
        msg.validate().unwrap();
    }

    #[test]
    fn test_model_message_validate_assistant_with_content() {
        let msg = ModelMessage::assistant("Hello", None);
        msg.validate().unwrap();
    }

    #[test]
    fn test_model_message_validate_assistant_with_tool_calls() {
        let tool_calls = vec![ToolCall {
            id: "call_1".to_string(),
            name: "test".to_string(),
            arguments: serde_json::json!({}),
        }];
        let msg = ModelMessage::assistant("", Some(&tool_calls));
        msg.validate().unwrap();
    }

    #[test]
    fn test_model_message_validate_assistant_empty_error() {
        let msg: ModelMessage<'_> = ModelMessage {
            role: Role::Assistant,
            content: Cow::Borrowed(""),
            tool_calls: None,
            tool_call_id: None,
        };
        assert!(msg.validate().is_err());
    }

    #[test]
    fn test_model_message_validate_tool_with_id() {
        let msg = ModelMessage::tool("result", "call_123");
        msg.validate().unwrap();
    }

    #[test]
    fn test_model_message_validate_tool_missing_id() {
        let msg: ModelMessage<'_> = ModelMessage {
            role: Role::Tool,
            content: Cow::Borrowed("result"),
            tool_calls: None,
            tool_call_id: None,
        };
        assert!(msg.validate().is_err());
    }
}
