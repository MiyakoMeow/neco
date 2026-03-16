//! ModelGroupClient with failover support

pub use crate::types::ModelRef;

use crate::error::ModelError;
use crate::types::{ChatChunk, ChatRequest, ChatResponse, ModelCapabilities, RetryConfig};
use async_trait::async_trait;
use dashmap::DashMap;
use futures::Stream;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::time::{Duration, sleep};

/// Boxed stream type.
pub type BoxStream<T> = Pin<Box<dyn Stream<Item = T> + Send>>;

/// Model client trait for interacting with LLM providers.
#[async_trait]
pub trait ModelClient: Send + Sync {
    /// Creates a chat completion.
    async fn chat_completion(&self, request: ChatRequest<'_>) -> Result<ChatResponse, ModelError>;

    /// Creates a streaming chat completion.
    async fn chat_completion_stream(
        &self,
        request: ChatRequest<'_>,
    ) -> Result<BoxStream<Result<ChatChunk, ModelError>>, ModelError>;

    /// Returns the model capabilities.
    fn capabilities(&self) -> ModelCapabilities;

    /// Returns the provider name.
    fn provider_name(&self) -> &str;

    /// Returns the model name.
    fn model_name(&self) -> &str;
}

/// A client that manages multiple models with failover support.
pub struct ModelGroupClient {
    /// Group name.
    name: String,
    /// List of model references.
    models: Vec<ModelRef>,
    /// Map of model name to client.
    clients: DashMap<String, Arc<dyn ModelClient>>,
    /// Retry configuration.
    retry_config: RetryConfig,
    /// List of API keys for rotation.
    api_keys: Vec<String>,
    /// Current API key index.
    key_index: AtomicUsize,
}

impl ModelGroupClient {
    /// Creates a new ModelGroupClient.
    pub fn new(name: String, models: Vec<ModelRef>, retry_config: RetryConfig) -> Self {
        Self {
            name,
            models,
            clients: DashMap::new(),
            retry_config,
            api_keys: Vec::new(),
            key_index: AtomicUsize::new(0),
        }
    }

    /// Sets API keys for rotation.
    pub fn with_api_keys(mut self, api_keys: Vec<String>) -> Self {
        self.api_keys = api_keys;
        self
    }

    /// Registers a model client.
    pub fn register_client(&self, model_name: String, client: Arc<dyn ModelClient>) {
        self.clients.insert(model_name, client);
    }

    /// Creates a chat completion with failover support.
    pub async fn chat_completion(
        &self,
        mut request: ChatRequest<'_>,
    ) -> Result<ChatResponse, ModelError> {
        for model_ref in &self.models {
            let client = if let Some(c) = self.clients.get(&model_ref.name) {
                Arc::clone(&c)
            } else {
                continue;
            };

            request.model = model_ref.name.clone();

            let result = self
                .execute_with_retry(client.as_ref(), request.clone())
                .await;

            if result.is_ok() {
                return result;
            }
        }

        Err(ModelError::AllModelsFailed {
            group: self.name.clone(),
        })
    }

    /// Creates a streaming chat completion with failover support.
    pub async fn chat_completion_stream(
        &self,
        mut request: ChatRequest<'_>,
    ) -> Result<BoxStream<Result<ChatChunk, ModelError>>, ModelError> {
        for model_ref in &self.models {
            let client = if let Some(c) = self.clients.get(&model_ref.name) {
                Arc::clone(&c)
            } else {
                continue;
            };

            request.model = model_ref.name.clone();

            let result = client.chat_completion_stream(request.clone()).await;

            if result.is_ok() {
                return result;
            }
        }

        Err(ModelError::AllModelsFailed {
            group: self.name.clone(),
        })
    }

    /// Executes request with retry logic.
    async fn execute_with_retry(
        &self,
        client: &dyn ModelClient,
        request: ChatRequest<'_>,
    ) -> Result<ChatResponse, ModelError> {
        let mut attempt = 0u32;
        let mut current_key_idx = self.key_index.load(Ordering::SeqCst);

        loop {
            match client.chat_completion(request.clone()).await {
                Ok(response) => return Ok(response),
                Err(e) => {
                    if !e.is_retryable() {
                        return Err(e);
                    }
                    if attempt >= self.retry_config.max_retries {
                        return Err(e);
                    }

                    if matches!(e, ModelError::RateLimit(_)) {
                        if let Some(next_idx) = self.rotate_key_index(current_key_idx) {
                            current_key_idx = next_idx;
                        }
                    }

                    let delay = self.retry_config.calculate_delay(attempt);
                    sleep(Duration::from_millis(delay)).await;
                    attempt += 1;
                },
            }
        }
    }

    /// Rotates the API key index.
    fn rotate_key_index(&self, current: usize) -> Option<usize> {
        if self.api_keys.is_empty() {
            return None;
        }
        let next = (current + 1) % self.api_keys.len();
        self.key_index.store(next, Ordering::SeqCst);
        Some(next)
    }

    /// Gets the current API key.
    pub fn get_current_key(&self) -> Option<&str> {
        if self.api_keys.is_empty() {
            return None;
        }
        let idx = self.key_index.load(Ordering::SeqCst);
        self.api_keys.get(idx).map(|s| s.as_str())
    }

    /// Returns the group name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the list of models.
    pub fn models(&self) -> &[ModelRef] {
        &self.models
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ModelMessage, Usage};
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct MockClient {
        fail_count: AtomicUsize,
        call_count: AtomicUsize,
        provider: &'static str,
        model: &'static str,
    }

    #[async_trait]
    impl ModelClient for MockClient {
        async fn chat_completion(
            &self,
            request: ChatRequest<'_>,
        ) -> Result<ChatResponse, ModelError> {
            let count = self.call_count.fetch_add(1, Ordering::SeqCst);
            if count < self.fail_count.load(Ordering::SeqCst) {
                return Err(ModelError::RateLimit("rate limited".to_string()));
            }
            Ok(ChatResponse {
                id: "test".to_string(),
                model: request.model.clone(),
                choices: vec![],
                usage: Usage {
                    prompt_tokens: 0,
                    completion_tokens: 0,
                    total_tokens: 0,
                },
            })
        }

        async fn chat_completion_stream(
            &self,
            _request: ChatRequest<'_>,
        ) -> Result<BoxStream<Result<ChatChunk, ModelError>>, ModelError> {
            unreachable!()
        }

        fn capabilities(&self) -> ModelCapabilities {
            ModelCapabilities::default()
        }

        fn provider_name(&self) -> &str {
            self.provider
        }

        fn model_name(&self) -> &str {
            self.model
        }
    }

    #[tokio::test]
    async fn test_model_group_client_success_first_model() {
        let client = Arc::new(MockClient {
            fail_count: AtomicUsize::new(0),
            call_count: AtomicUsize::new(0),
            provider: "test",
            model: "model1",
        });

        let group = ModelGroupClient::new(
            "test-group".to_string(),
            vec![ModelRef {
                name: "model1".to_string(),
                provider: "test".to_string(),
            }],
            RetryConfig::default(),
        );
        group.register_client("model1".to_string(), client);

        let request = ChatRequest::new("model1", vec![ModelMessage::user("hello")]);
        let result = group.chat_completion(request).await;

        result.unwrap();
    }

    #[tokio::test]
    async fn test_model_group_client_failover() {
        let client1 = Arc::new(MockClient {
            fail_count: AtomicUsize::new(10),
            call_count: AtomicUsize::new(0),
            provider: "test",
            model: "model1",
        });

        let client2 = Arc::new(MockClient {
            fail_count: AtomicUsize::new(0),
            call_count: AtomicUsize::new(0),
            provider: "test",
            model: "model2",
        });

        let group = ModelGroupClient::new(
            "test-group".to_string(),
            vec![
                ModelRef {
                    name: "model1".to_string(),
                    provider: "test".to_string(),
                },
                ModelRef {
                    name: "model2".to_string(),
                    provider: "test".to_string(),
                },
            ],
            RetryConfig::default(),
        );
        group.register_client("model1".to_string(), client1);
        group.register_client("model2".to_string(), client2);

        let request = ChatRequest::new("model1", vec![ModelMessage::user("hello")]);
        let result = group.chat_completion(request).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap().model, "model2");
    }

    #[tokio::test]
    async fn test_model_group_client_all_fail() {
        let client = Arc::new(MockClient {
            fail_count: AtomicUsize::new(10),
            call_count: AtomicUsize::new(0),
            provider: "test",
            model: "model1",
        });

        let group = ModelGroupClient::new(
            "test-group".to_string(),
            vec![ModelRef {
                name: "model1".to_string(),
                provider: "test".to_string(),
            }],
            RetryConfig {
                max_retries: 1,
                initial_delay_ms: 10,
                max_delay_ms: 100,
                backoff_multiplier: 2.0,
            },
        );
        group.register_client("model1".to_string(), client);

        let request = ChatRequest::new("model1", vec![ModelMessage::user("hello")]);
        let result = group.chat_completion(request).await;

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ModelError::AllModelsFailed { .. }
        ));
    }
}
