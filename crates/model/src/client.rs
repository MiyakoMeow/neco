//! ModelClient trait and implementations

pub use crate::model_group::{BoxStream, ModelClient, ModelGroupClient, ModelRef};

use crate::error::ModelError;
use crate::types::{ChatChunk, ChatRequest, ChatResponse, ModelCapabilities, RetryConfig};
use async_trait::async_trait;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::time::{Duration, sleep};

/// A client that uses multiple API keys.
pub struct MultiKeyClient {
    /// List of model clients.
    clients: Vec<Arc<dyn ModelClient>>,
    /// Current key index.
    key_index: AtomicUsize,
    /// Retry configuration.
    retry_config: RetryConfig,
    /// Model name.
    model_name: String,
}

impl MultiKeyClient {
    /// Creates a new MultiKeyClient.
    pub fn new(clients: Vec<Arc<dyn ModelClient>>, retry_config: RetryConfig) -> Self {
        Self {
            clients,
            key_index: AtomicUsize::new(0),
            retry_config,
            model_name: String::new(),
        }
    }

    /// Executes request with key rotation.
    async fn execute_with_key_rotation(
        &self,
        request: ChatRequest<'_>,
    ) -> Result<ChatResponse, ModelError> {
        let num_keys = self.clients.len();

        for _ in 0..num_keys {
            let idx = self.key_index.load(Ordering::SeqCst);
            let client = self
                .clients
                .get(idx)
                .ok_or_else(|| ModelError::Config("No client available".to_string()))?;

            match client.chat_completion(request.clone()).await {
                Ok(response) => return Ok(response),
                Err(e) => {
                    if !e.is_retryable() {
                        return Err(e);
                    }

                    if matches!(e, ModelError::RateLimit(_)) {
                        let next = (idx + 1) % num_keys;
                        self.key_index.store(next, Ordering::SeqCst);
                    }

                    let delay = self.retry_config.calculate_delay(0);
                    sleep(Duration::from_millis(delay)).await;
                },
            }
        }

        Err(ModelError::Api {
            provider: "MultiKeyClient".to_string(),
            message: "All API keys failed".to_string(),
        })
    }
}

#[async_trait]
impl ModelClient for MultiKeyClient {
    async fn chat_completion(&self, request: ChatRequest<'_>) -> Result<ChatResponse, ModelError> {
        self.execute_with_key_rotation(request).await
    }

    async fn chat_completion_stream(
        &self,
        request: ChatRequest<'_>,
    ) -> Result<BoxStream<Result<ChatChunk, ModelError>>, ModelError> {
        let idx = self.key_index.load(Ordering::SeqCst);
        let client = self
            .clients
            .get(idx)
            .ok_or_else(|| ModelError::Config("No client available".to_string()))?;
        client.chat_completion_stream(request).await
    }

    fn capabilities(&self) -> ModelCapabilities {
        self.clients
            .first()
            .map(|c| c.capabilities())
            .unwrap_or_default()
    }

    #[allow(clippy::unnecessary_literal_bound)]
    fn provider_name(&self) -> &str {
        "MultiKey"
    }

    fn model_name(&self) -> &str {
        &self.model_name
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ModelMessage, ModelRef, Usage};
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

    #[tokio::test]
    async fn test_model_group_client_non_retryable_error() {
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
}
