//! OpenAI provider implementation

use crate::client::{BoxStream, ModelClient};
use crate::error::ModelError;
use crate::registry::CapabilitiesRegistry;
use crate::types::{
    ChatChunk, ChatRequest, ChatResponse, Choice, Delta, Message, ModelCapabilities,
};
use async_openai::{
    Client,
    config::OpenAIConfig,
    types::chat::{ChatCompletionRequestMessage, CreateChatCompletionRequest},
};
use async_trait::async_trait;
use futures::StreamExt;
use hmac::{Hmac, Mac};
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue};
use sha2::Sha256;
use std::time::{SystemTime, UNIX_EPOCH};

/// HMAC-SHA256 type for signature generation.
type HmacSha256 = Hmac<Sha256>;

/// Default OpenAI API base URL.
const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";

/// OpenAI model client.
pub struct OpenAiClient {
    /// HTTP client.
    client: Client<OpenAIConfig>,
    /// Model name.
    model: String,
    /// Provider name.
    provider: String,
    /// Secret key for HMAC-SHA256 signature.
    secret_key: Option<String>,
    /// API key (stored separately for signature auth).
    api_key: String,
    /// Base URL.
    base_url: String,
}

impl OpenAiClient {
    /// Creates a new OpenAiClient.
    pub fn new(api_key: impl Into<String>, model: impl Into<String>) -> Result<Self, ModelError> {
        let api_key = api_key.into();
        if api_key.is_empty() {
            return Err(ModelError::Config("OPENAI_API_KEY is empty".to_string()));
        }

        let config = OpenAIConfig::new()
            .with_api_key(&api_key)
            .with_api_base(DEFAULT_BASE_URL);

        let client = Client::with_config(config);

        Ok(Self {
            client,
            model: model.into(),
            provider: "openai".to_string(),
            secret_key: None,
            api_key,
            base_url: DEFAULT_BASE_URL.to_string(),
        })
    }

    /// Sets a custom base URL.
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        let base_url = base_url.into();
        let config = OpenAIConfig::new()
            .with_api_key(&self.api_key)
            .with_api_base(&base_url);
        self.client = Client::with_config(config);
        self.base_url = base_url;
        self
    }

    /// Sets the provider name.
    pub fn with_provider(mut self, provider: impl Into<String>) -> Self {
        self.provider = provider.into();
        self
    }

    /// Sets the secret key for HMAC-SHA256 signature authentication.
    pub fn with_secret_key(mut self, secret_key: impl Into<String>) -> Self {
        self.secret_key = Some(secret_key.into());
        self
    }

    /// Generates HMAC-SHA256 signature for MiniMax API.
    fn generate_signature(
        &self,
        method: &str,
        request_path: &str,
        timestamp: &str,
    ) -> Result<String, ModelError> {
        let secret_key = self
            .secret_key
            .as_ref()
            .ok_or_else(|| ModelError::Config("Secret key not set".to_string()))?;

        let accept = "application/json";
        let content_type = "application/json";

        let string_to_sign =
            format!("{method}\n{accept}\n{content_type}\n{timestamp}\n{request_path}");

        let mut mac = HmacSha256::new_from_slice(secret_key.as_bytes())
            .map_err(|e| ModelError::Config(format!("Failed to create HMAC: {e}")))?;

        mac.update(string_to_sign.as_bytes());
        let result = mac.finalize();

        Ok(hex::encode(result.into_bytes()))
    }

    /// Sends request to OpenAI API with optional signature authentication.
    async fn send_request(&self, request: &ChatRequest<'_>) -> Result<ChatResponse, ModelError> {
        if let Some(ref secret_key) = self.secret_key {
            return self.send_request_with_signature(request, secret_key).await;
        }

        let openai_request = self.build_request(request);

        let response = self
            .client
            .chat()
            .create(openai_request)
            .await
            .map_err(|e| ModelError::api(&self.provider, e.to_string()))?;

        self.convert_response(response)
    }

    /// Sends request with HMAC-SHA256 signature authentication.
    async fn send_request_with_signature(
        &self,
        request: &ChatRequest<'_>,
        _secret_key: &str,
    ) -> Result<ChatResponse, ModelError> {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| ModelError::Config(format!("Failed to get timestamp: {e}")))?
            .as_millis()
            .to_string();

        let request_path = "/v1/chat/completions";
        let signature = self.generate_signature("POST", request_path, &timestamp)?;

        let openai_request = self.build_request(request);
        let request_json = serde_json::to_string(&openai_request)
            .map_err(|e| ModelError::Config(format!("Failed to serialize request: {e}")))?;

        let client = reqwest::Client::new();
        let url = format!("{}{}", self.base_url.trim_end_matches('/'), request_path);

        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", self.api_key)).map_err(|e| {
                ModelError::Config(format!("Failed to create authorization header: {e}"))
            })?,
        );
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(
            "X-Ca-Key",
            HeaderValue::from_str(&self.api_key).map_err(|e| {
                ModelError::Config(format!("Failed to create X-Ca-Key header: {e}"))
            })?,
        );
        headers.insert(
            "X-Ca-Signature",
            HeaderValue::from_str(&signature).map_err(|e| {
                ModelError::Config(format!("Failed to create X-Ca-Signature header: {e}"))
            })?,
        );
        headers.insert(
            "X-Ca-Timestamp",
            HeaderValue::from_str(&timestamp).map_err(|e| {
                ModelError::Config(format!("Failed to create X-Ca-Timestamp header: {e}"))
            })?,
        );

        let response = client
            .post(&url)
            .headers(headers)
            .body(request_json)
            .send()
            .await
            .map_err(|e| ModelError::api(&self.provider, e.to_string()))?;

        let openai_response: async_openai::types::chat::CreateChatCompletionResponse = response
            .json()
            .await
            .map_err(|e| ModelError::api(&self.provider, e.to_string()))?;

        self.convert_response(openai_response)
    }

    /// Builds request for OpenAI API.
    fn build_request(&self, request: &ChatRequest<'_>) -> CreateChatCompletionRequest {
        let mut system_contents: Vec<String> = Vec::new();
        let mut user_contents: Vec<String> = Vec::new();
        let mut assistant_contents: Vec<String> = Vec::new();

        for msg in &request.messages {
            let content = msg.content.to_string();
            match msg.role {
                neoco_core::Role::System => {
                    system_contents.push(content);
                },
                neoco_core::Role::User | neoco_core::Role::Tool => {
                    user_contents.push(content);
                },
                neoco_core::Role::Assistant => {
                    assistant_contents.push(content);
                },
            }
        }

        let mut messages: Vec<ChatCompletionRequestMessage> = Vec::new();

        if !system_contents.is_empty() {
            let combined = system_contents.join("\n\n---\n\n");
            messages.push(ChatCompletionRequestMessage::System(combined.into()));
        }

        for content in user_contents {
            messages.push(ChatCompletionRequestMessage::User(content.into()));
        }

        for content in assistant_contents {
            messages.push(ChatCompletionRequestMessage::Assistant(content.into()));
        }

        CreateChatCompletionRequest {
            model: request.model.clone(),
            messages,
            stream: Some(request.stream),
            ..Default::default()
        }
    }

    /// Converts OpenAI response to chat response.
    #[allow(clippy::uninlined_format_args)]
    fn convert_response(
        &self,
        openai: async_openai::types::chat::CreateChatCompletionResponse,
    ) -> Result<ChatResponse, ModelError> {
        let choices: Vec<Choice> = openai
            .choices
            .into_iter()
            .enumerate()
            .map(|(i, c)| {
                let message = c.message;
                Choice {
                    index: i,
                    message: Message {
                        role: message.role.to_string(),
                        content: message.content.unwrap_or_default(),
                        tool_calls: None,
                        tool_call_id: None,
                    },
                    finish_reason: c.finish_reason.map(|f| format!("{f:?}")),
                }
            })
            .collect();

        let usage = openai.usage.unwrap_or_default();

        Ok(ChatResponse {
            id: openai.id,
            model: openai.model,
            choices,
            usage: crate::types::Usage {
                prompt_tokens: usage.prompt_tokens,
                completion_tokens: usage.completion_tokens,
                total_tokens: usage.total_tokens,
            },
        })
    }
}

#[async_trait]
impl ModelClient for OpenAiClient {
    async fn chat_completion(&self, request: ChatRequest<'_>) -> Result<ChatResponse, ModelError> {
        self.send_request(&request).await
    }

    async fn chat_completion_stream(
        &self,
        request: ChatRequest<'_>,
    ) -> Result<BoxStream<Result<ChatChunk, ModelError>>, ModelError> {
        let mut req = request;
        req.stream = true;

        let openai_request = self.build_request(&req);

        let stream = self
            .client
            .chat()
            .create_stream(openai_request)
            .await
            .map_err(|e| ModelError::api(&self.provider, e.to_string()))?;

        let provider = self.provider.clone();
        let mapped_stream = stream.map(move |result| match result {
            Ok(chunk) => Ok(convert_stream_chunk(chunk)),
            Err(e) => Err(ModelError::api(&provider, e.to_string())),
        });

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

/// Converts OpenAI chunk response to chat chunk.
#[allow(clippy::uninlined_format_args)]
fn convert_stream_chunk(
    openai: async_openai::types::chat::CreateChatCompletionStreamResponse,
) -> ChatChunk {
    ChatChunk {
        id: openai.id,
        choices: openai
            .choices
            .into_iter()
            .enumerate()
            .map(|(i, c)| {
                let delta = c.delta;
                crate::types::ChunkChoice {
                    index: i,
                    delta: Delta {
                        role: delta.role.map(|r| r.to_string()),
                        content: delta.content,
                        tool_calls: None,
                    },
                    finish_reason: c.finish_reason.map(|f| format!("{f:?}")),
                }
            })
            .collect(),
    }
}
