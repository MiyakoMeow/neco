//! Anthropic provider implementation

#![allow(clippy::unnecessary_lazy_evaluations)]
#![allow(clippy::unnecessary_join)]

use crate::client::{BoxStream, ModelClient};
use crate::error::ModelError;
use crate::registry::CapabilitiesRegistry;
use crate::types::{
    ChatChunk, ChatRequest, ChatResponse, Choice, Delta, Message, ModelCapabilities,
};
use async_anthropic::{
    Client,
    types::{CreateMessagesRequest, MessageContentList, MessageRole, MessagesStreamEvent},
};
use async_trait::async_trait;
use futures::StreamExt;

/// Anthropic model client.
pub struct AnthropicClient {
    /// Internal Anthropic API client.
    client: Client,
    /// Model name.
    model: String,
    /// Provider name.
    provider: String,
}

impl AnthropicClient {
    /// Creates a new AnthropicClient.
    pub fn new(api_key: impl Into<String>, model: impl Into<String>) -> Result<Self, ModelError> {
        let api_key = api_key.into();
        if api_key.is_empty() {
            return Err(ModelError::Config("ANTHROPIC_API_KEY is empty".to_string()));
        }

        let client = Client::from_api_key(api_key);

        Ok(Self {
            client,
            model: model.into(),
            provider: "anthropic".to_string(),
        })
    }

    /// Sets a custom base URL.
    pub fn with_base_url(self, _base_url: impl Into<String>) -> Self {
        self
    }

    /// Sets the provider name.
    pub fn with_provider(mut self, provider: impl Into<String>) -> Self {
        self.provider = provider.into();
        self
    }

    /// Builds an Anthropic API request from a generic chat request.
    fn build_request(&self, request: &ChatRequest<'_>) -> CreateMessagesRequest {
        let messages: Vec<async_anthropic::types::Message> = request
            .messages
            .iter()
            .map(|msg| {
                let content = match msg.role {
                    neoco_core::Role::Tool => {
                        let tool_call_id = msg.tool_call_id.as_deref().unwrap_or_default();
                        let content = msg.content.to_string();
                        async_anthropic::types::MessageContentList::from(
                            async_anthropic::types::ToolResult {
                                tool_use_id: tool_call_id.to_string(),
                                content: Some(content),
                                is_error: false,
                            },
                        )
                    },
                    _ => MessageContentList::from(msg.content.to_string()),
                };
                let role = match msg.role {
                    neoco_core::Role::System | neoco_core::Role::User | neoco_core::Role::Tool => {
                        MessageRole::User
                    },
                    neoco_core::Role::Assistant => MessageRole::Assistant,
                };
                async_anthropic::types::Message { role, content }
            })
            .collect();

        let tools = request.tools.as_ref().map(|tools| {
            tools
                .iter()
                .map(|tool| {
                    let mut map = serde_json::Map::new();
                    map.insert(
                        "type".to_string(),
                        serde_json::Value::String("function".to_string()),
                    );
                    let mut function_map = serde_json::Map::new();
                    function_map.insert(
                        "name".to_string(),
                        serde_json::Value::String(tool.name.clone()),
                    );
                    function_map.insert(
                        "description".to_string(),
                        serde_json::Value::String(tool.description.clone()),
                    );
                    function_map.insert("parameters".to_string(), tool.parameters.clone());
                    map.insert(
                        "function".to_string(),
                        serde_json::Value::Object(function_map),
                    );
                    map
                })
                .collect::<Vec<serde_json::Map<String, serde_json::Value>>>()
        });

        let tool_choice = request.tool_choice.as_ref().map(|choice| match choice {
            crate::types::ToolChoice::Auto => async_anthropic::types::ToolChoice::Auto,
            crate::types::ToolChoice::None => async_anthropic::types::ToolChoice::Any,
            crate::types::ToolChoice::Function { name } => {
                async_anthropic::types::ToolChoice::Tool(name.clone())
            },
        });

        let max_tokens = request.effective_max_tokens().unwrap_or(1024) as i32;

        CreateMessagesRequest {
            model: request.model.clone(),
            messages,
            max_tokens,
            stream: request.stream,
            system: None,
            temperature: request.temperature.map(|t| t as f32),
            top_p: request.top_p.map(|t| t as f32),
            tools,
            tool_choice,
            stop_sequences: request.stop.clone(),
            top_k: None,
            metadata: None,
        }
    }

    /// Sends a chat completion request to the Anthropic API.
    async fn send_request(&self, request: &ChatRequest<'_>) -> Result<ChatResponse, ModelError> {
        let anthropic_request = self.build_request(request);

        let response = self
            .client
            .messages()
            .create(anthropic_request)
            .await
            .map_err(|e| ModelError::api(&self.provider, e.to_string()))?;

        self.convert_response(response)
    }

    /// Converts an Anthropic API response to a generic chat response.
    fn convert_response(
        &self,
        anthropic: async_anthropic::types::CreateMessagesResponse,
    ) -> Result<ChatResponse, ModelError> {
        let mut content = String::new();
        let mut tool_calls: Option<Vec<crate::types::ToolCall>> = None;

        for block in anthropic.content.unwrap_or_default() {
            match block {
                async_anthropic::types::MessageContent::Text(text) => {
                    content.push_str(&text.text);
                },
                async_anthropic::types::MessageContent::ToolUse(tool_use) => {
                    let tc = crate::types::ToolCall {
                        id: tool_use.id,
                        name: tool_use.name,
                        arguments: tool_use.input,
                    };
                    if tool_calls.is_none() {
                        tool_calls = Some(Vec::new());
                    }
                    tool_calls.as_mut().unwrap().push(tc);
                },
                async_anthropic::types::MessageContent::ToolResult(_) => {},
            }
        }

        let finish_reason = anthropic.stop_reason.map(|r| format!("{r:?}")).or_else(|| {
            if tool_calls.is_some() {
                Some("tool_calls".to_string())
            } else {
                None
            }
        });

        let choices = vec![Choice {
            index: 0,
            message: Message {
                role: "assistant".to_string(),
                content,
                tool_calls,
                tool_call_id: None,
            },
            finish_reason,
        }];

        let usage = anthropic
            .usage
            .unwrap_or_else(|| async_anthropic::types::Usage {
                input_tokens: Some(0),
                output_tokens: Some(0),
            });

        Ok(ChatResponse {
            id: anthropic.id.unwrap_or_default(),
            model: anthropic.model.unwrap_or_default(),
            choices,
            usage: crate::types::Usage {
                prompt_tokens: usage.input_tokens.unwrap_or(0),
                completion_tokens: usage.output_tokens.unwrap_or(0),
                total_tokens: usage.input_tokens.unwrap_or(0) + usage.output_tokens.unwrap_or(0),
            },
        })
    }
}

#[async_trait]
impl ModelClient for AnthropicClient {
    async fn chat_completion(&self, request: ChatRequest<'_>) -> Result<ChatResponse, ModelError> {
        self.send_request(&request).await
    }

    async fn chat_completion_stream(
        &self,
        request: ChatRequest<'_>,
    ) -> Result<BoxStream<Result<ChatChunk, ModelError>>, ModelError> {
        let mut req = request;
        req.stream = true;
        let model = req.model.clone();

        let anthropic_request = self.build_request(&req);

        let stream = self
            .client
            .messages()
            .create_stream(anthropic_request)
            .await;

        let provider = self.provider.clone();
        let mapped_stream = stream.map(
            move |result: Result<MessagesStreamEvent, async_anthropic::errors::AnthropicError>| {
                let event = result.map_err(|e| ModelError::api(&provider, e.to_string()))?;
                Ok(convert_stream_event(event, &model))
            },
        );

        Ok(Box::pin(mapped_stream))
    }

    fn capabilities(&self) -> ModelCapabilities {
        CapabilitiesRegistry::get(&self.provider, &self.model)
    }

    fn provider_name(&self) -> &str {
        &self.provider
    }

    fn model_name(&self) -> &str {
        &self.model
    }
}

/// Converts an Anthropic streaming event to a generic chat chunk.
fn convert_stream_event(event: MessagesStreamEvent, _model: &str) -> ChatChunk {
    match event {
        MessagesStreamEvent::MessageStart { message, usage: _ } => {
            let content = message
                .content
                .into_iter()
                .filter_map(|block| {
                    if let async_anthropic::types::MessageContent::Text(text) = block {
                        Some(text.text)
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .join("");

            ChatChunk {
                id: message.id,
                choices: vec![crate::types::ChunkChoice {
                    index: 0,
                    delta: Delta {
                        role: Some("assistant".to_string()),
                        content: if content.is_empty() {
                            None
                        } else {
                            Some(content)
                        },
                        tool_calls: None,
                    },
                    finish_reason: None,
                }],
            }
        },
        MessagesStreamEvent::ContentBlockDelta { index, delta } => {
            let content =
                if let async_anthropic::types::ContentBlockDelta::TextDelta { text } = delta {
                    Some(text)
                } else {
                    None
                };

            ChatChunk {
                id: String::new(),
                choices: vec![crate::types::ChunkChoice {
                    index,
                    delta: Delta {
                        role: None,
                        content,
                        tool_calls: None,
                    },
                    finish_reason: None,
                }],
            }
        },
        MessagesStreamEvent::MessageDelta { delta, usage: _ } => {
            let finish_reason = delta.stop_reason.map(|r| format!("{r:?}"));

            ChatChunk {
                id: String::new(),
                choices: vec![crate::types::ChunkChoice {
                    index: 0,
                    delta: Delta::default(),
                    finish_reason,
                }],
            }
        },
        _ => ChatChunk {
            id: String::new(),
            choices: vec![crate::types::ChunkChoice {
                index: 0,
                delta: Delta::default(),
                finish_reason: None,
            }],
        },
    }
}
