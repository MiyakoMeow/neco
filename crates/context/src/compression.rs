//! Compression service module.
//!
//! This module provides the `CompressionService` for context compression,
//! using LLM to generate summaries of conversation history.

use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use neoco_core::ids::MessageId;
use neoco_core::messages::{Message, Role};
use neoco_core::prompt::PromptLoader;
use neoco_core::traits::MessageRepository;
use neoco_model::types::{ChatRequest, ModelMessage};

use crate::config::{ContextConfig, TokenSavings};
use crate::tokenizer::TokenCounter;

/// Error type for compression operations.
#[derive(Debug, thiserror::Error)]
pub enum CompressionError {
    /// Model error.
    #[error("模型调用失败: {0}")]
    Model(String),

    /// Nothing to compress.
    #[error("没有可压缩的消息")]
    NothingToCompact,

    /// Storage error.
    #[error("存储错误: {0}")]
    Storage(String),
}

/// Compression service trait.
#[async_trait]
pub trait CompressionService: Send + Sync {
    /// Compresses the context.
    async fn compress(
        &self,
        agent_ulid: &neoco_core::AgentUlid,
        tag: Option<&str>,
    ) -> Result<String, String>;
}

/// Compression service implementation.
pub struct CompressionServiceImpl {
    /// Message repository.
    message_repo: Arc<dyn MessageRepository>,
    /// Token counter.
    token_counter: Arc<dyn TokenCounter>,
    /// Model client.
    model_client: Arc<dyn neoco_model::client::ModelClient>,
    /// Configuration.
    config: ContextConfig,
    /// Prompt loader for loading prompt templates.
    prompt_loader: Arc<dyn PromptLoader>,
}

impl CompressionServiceImpl {
    /// Creates a new `CompressionServiceImpl`.
    pub fn new(
        message_repo: Arc<dyn MessageRepository>,
        token_counter: Arc<dyn TokenCounter>,
        model_client: Arc<dyn neoco_model::client::ModelClient>,
        config: ContextConfig,
        prompt_loader: Arc<dyn PromptLoader>,
    ) -> Self {
        Self {
            message_repo,
            token_counter,
            model_client,
            config,
            prompt_loader,
        }
    }

    /// Compresses messages for an agent and returns detailed result.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Storage operations fail
    /// - There are no messages to compress
    /// - Model client fails to process compression
    pub async fn compress_with_result(
        &self,
        agent_ulid: &neoco_core::AgentUlid,
        tag: Option<&str>,
    ) -> Result<CompressionResult, CompressionError> {
        let start = Instant::now();

        let messages = self
            .message_repo
            .list(agent_ulid)
            .await
            .map_err(|e| CompressionError::Storage(e.to_string()))?;

        if messages.is_empty() {
            return Err(CompressionError::NothingToCompact);
        }

        let original_count = messages.len();
        let before_tokens = self.token_counter.estimate_tokens(&messages);

        let (preserve_ids, to_compact) = self.select_messages_to_compact(&messages, tag);

        if to_compact.is_empty() {
            return Err(CompressionError::NothingToCompact);
        }

        let summary = self.generate_summary(&to_compact).await?;

        let summary_message = Message::system(summary.clone());

        if let Some(before_id) = preserve_ids.first() {
            let _ = self
                .message_repo
                .delete_prefix(agent_ulid, *before_id)
                .await;
        }

        let _ = self.message_repo.append(agent_ulid, &summary_message).await;

        let compacted_messages = self
            .message_repo
            .list(agent_ulid)
            .await
            .map_err(|e| CompressionError::Storage(e.to_string()))?;
        let after_tokens = self.token_counter.estimate_tokens(&compacted_messages);

        let token_savings = TokenSavings::new(
            before_tokens.try_into().unwrap_or(u32::MAX),
            after_tokens.try_into().unwrap_or(u32::MAX),
        );

        let duration = start.elapsed();

        Ok(CompressionResult {
            original_count,
            compacted_count: compacted_messages.len(),
            summary,
            preserved_ids: preserve_ids,
            token_savings,
            duration,
        })
    }

    /// Selects messages to be compressed.
    fn select_messages_to_compact<'a>(
        &self,
        messages: &'a [Message],
        tag: Option<&str>,
    ) -> (Vec<MessageId>, Vec<&'a Message>) {
        let start_idx = if let Some(tag) = tag {
            messages
                .iter()
                .position(|m| m.content.starts_with(&format!("[TAG:{tag}]")))
                .map_or(0, |i| i + 1)
        } else {
            0
        };

        if messages.len() <= self.config.keep_recent_messages {
            return (Vec::new(), Vec::new());
        }

        let preserve_count = self.config.keep_recent_messages;
        let compact_start = start_idx;
        let compact_end = messages.len() - preserve_count;

        if compact_start >= compact_end {
            return (Vec::new(), Vec::new());
        }

        let preserve_ids: Vec<MessageId> = messages[compact_end..].iter().map(|m| m.id).collect();

        let compact_refs: Vec<&Message> = messages[compact_start..compact_end].iter().collect();

        (preserve_ids, compact_refs)
    }

    /// Generates a summary for the given messages using LLM.
    async fn generate_summary(&self, messages: &[&Message]) -> Result<String, CompressionError> {
        let prompt = self.build_summary_prompt(messages);

        let request = ChatRequest::new(
            self.config.compact_model_group.as_str(),
            vec![ModelMessage::user(prompt)],
        )
        .with_max_tokens(2048)
        .with_temperature(0.3)
        .with_thinking(false)
        .with_tools(None);

        let response = self
            .model_client
            .chat_completion(request)
            .await
            .map_err(|e| CompressionError::Model(e.to_string()))?;

        let summary = response
            .choices
            .first()
            .and_then(|c| {
                if c.message.content.is_empty() {
                    None
                } else {
                    Some(c.message.content.clone())
                }
            })
            .unwrap_or_else(|| "[Summary generation failed - empty response]".to_string());

        Ok(summary)
    }

    /// Builds the prompt for summary generation.
    fn build_summary_prompt(&self, messages: &[&Message]) -> String {
        // Try to load prompt from component system
        let template = self
            .prompt_loader
            .load("context::compact")
            .unwrap_or_else(|_| {
                // Fallback to default prompt
                String::from(
                    "Analyze the following conversation history and answer these 5 questions:\n\n\
1. What is the main topic or goal of this conversation?\n\
2. What are the key decisions or conclusions reached?\n\
3. What important information should be preserved?\n\
4. What actions or tasks are pending?\n\
5. What context is no longer relevant?\n\n\
Conversation content:\n\n",
                )
            });

        // Build messages content
        let mut messages_content = String::new();
        for msg in messages.iter().take(30) {
            let role_str = match msg.role {
                Role::System => "System",
                Role::User => "User",
                Role::Assistant => "Assistant",
                Role::Tool => "Tool",
            };
            let content = if msg.content.len() > 800 {
                format!("{}...", &msg.content[..800])
            } else {
                msg.content.clone()
            };
            messages_content.push_str(role_str);
            messages_content.push_str(": ");
            messages_content.push_str(&content);
            messages_content.push_str("\n\n");
        }

        // Replace placeholder with actual messages
        template.replace("{{MESSAGES}}", &messages_content)
    }
}

#[async_trait]
impl CompressionService for CompressionServiceImpl {
    async fn compress(
        &self,
        agent_ulid: &neoco_core::AgentUlid,
        tag: Option<&str>,
    ) -> Result<String, String> {
        self.compress_with_result(agent_ulid, tag)
            .await
            .map(|r| r.summary)
            .map_err(|e| e.to_string())
    }
}

/// Result of compression.
#[derive(Debug, Clone)]
pub struct CompressionResult {
    /// Original message count.
    pub original_count: usize,
    /// Compacted message count.
    pub compacted_count: usize,
    /// The summary.
    pub summary: String,
    /// Preserved message IDs.
    pub preserved_ids: Vec<MessageId>,
    /// Token savings.
    pub token_savings: TokenSavings,
    /// Duration of compression.
    pub duration: Duration,
}

/// Alias for `CompressionResult`.
pub type CompactResult = CompressionResult;

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::RwLock;

    struct MockPromptLoader {
        prompts: RwLock<HashMap<String, String>>,
    }

    impl MockPromptLoader {
        fn with_default_prompt() -> Self {
            let mut prompts = HashMap::new();
            prompts.insert(
                "context::compact".to_string(),
                "Analyze the following conversation history and answer these 5 questions:\n\n\
1. What is the main topic or goal of this conversation?\n\
2. What are the key decisions or conclusions reached?\n\
3. What important information should be preserved?\n\
4. What actions or tasks are pending?\n\
5. What context is no longer relevant?\n\n\
Conversation content:\n\n{{MESSAGES}}"
                    .to_string(),
            );
            Self {
                prompts: RwLock::new(prompts),
            }
        }
    }

    impl PromptLoader for MockPromptLoader {
        fn load(&self, id: &str) -> Result<String, neoco_core::prompt::PromptError> {
            self.prompts
                .read()
                .unwrap()
                .get(id)
                .cloned()
                .ok_or_else(|| neoco_core::prompt::PromptError::NotFound(id.to_string()))
        }

        fn list_components(
            &self,
        ) -> Result<Vec<neoco_core::prompt::PromptComponent>, neoco_core::prompt::PromptError>
        {
            Ok(vec![])
        }
    }

    fn create_test_service() -> CompressionServiceImpl {
        CompressionServiceImpl::new(
            Arc::new(MockMessageRepo),
            Arc::new(MockTokenCounter),
            Arc::new(MockModelClient),
            ContextConfig::default(),
            Arc::new(MockPromptLoader::with_default_prompt()),
        )
    }

    struct MockMessageRepo;
    struct MockTokenCounter;

    #[async_trait]
    impl MessageRepository for MockMessageRepo {
        async fn append(
            &self,
            _: &neoco_core::AgentUlid,
            _: &Message,
        ) -> Result<(), neoco_core::errors::StorageError> {
            Ok(())
        }
        async fn list(
            &self,
            _: &neoco_core::AgentUlid,
        ) -> Result<Vec<Message>, neoco_core::errors::StorageError> {
            Ok(vec![])
        }
        async fn delete_prefix(
            &self,
            _: &neoco_core::AgentUlid,
            _: MessageId,
        ) -> Result<(), neoco_core::errors::StorageError> {
            Ok(())
        }
        async fn delete_suffix(
            &self,
            _: &neoco_core::AgentUlid,
            _: MessageId,
        ) -> Result<(), neoco_core::errors::StorageError> {
            Ok(())
        }
    }

    impl TokenCounter for MockTokenCounter {
        fn estimate_tokens(&self, _: &[Message]) -> usize {
            0
        }

        fn estimate_string_tokens(&self, _: &str) -> usize {
            0
        }

        fn estimate_message_tokens(&self, _: &Message) -> usize {
            0
        }
    }

    struct MockModelClient;

    #[async_trait]
    impl neoco_model::client::ModelClient for MockModelClient {
        async fn chat_completion(
            &self,
            _: neoco_model::types::ChatRequest<'_>,
        ) -> Result<neoco_model::types::ChatResponse, neoco_model::error::ModelError> {
            Ok(neoco_model::types::ChatResponse {
                id: "test".to_string(),
                model: "test".to_string(),
                choices: vec![neoco_model::types::Choice {
                    index: 0,
                    message: neoco_model::types::Message {
                        role: "assistant".to_string(),
                        content: "Summary".to_string(),
                        tool_calls: None,
                        tool_call_id: None,
                    },
                    finish_reason: Some("stop".to_string()),
                }],
                usage: neoco_model::types::Usage {
                    prompt_tokens: 0,
                    completion_tokens: 0,
                    total_tokens: 0,
                },
            })
        }

        async fn chat_completion_stream(
            &self,
            _: neoco_model::types::ChatRequest<'_>,
        ) -> Result<
            neoco_model::client::BoxStream<
                Result<neoco_model::types::ChatChunk, neoco_model::error::ModelError>,
            >,
            neoco_model::error::ModelError,
        > {
            Err(neoco_model::error::ModelError::api(
                "dummy",
                "Not implemented",
            ))
        }

        fn capabilities(&self) -> neoco_model::types::ModelCapabilities {
            neoco_model::types::ModelCapabilities {
                streaming: true,
                tools: false,
                functions: false,
                json_mode: false,
                vision: false,
                context_window: 128_000,
            }
        }

        fn provider_name(&self) -> &'static str {
            "dummy"
        }

        fn model_name(&self) -> &'static str {
            "dummy"
        }
    }

    #[test]
    fn test_build_summary_prompt_contains_key_sections() {
        let service = create_test_service();
        let msg1 = Message::new(Role::User, "Hello, how are you?".to_string());
        let msg2 = Message::new(Role::Assistant, "I'm doing well, thank you!".to_string());
        let messages: Vec<&Message> = vec![&msg1, &msg2];

        let prompt = service.build_summary_prompt(&messages);

        assert!(prompt.contains("User:"));
        assert!(prompt.contains("Assistant:"));
        assert!(prompt.contains("Hello, how are you?"));
        assert!(prompt.contains("What is the main topic or goal of this conversation?"));
    }

    #[test]
    fn test_build_summary_prompt_truncates_long_content() {
        let service = create_test_service();
        let long_content = "abcdefghijklmnopqrstuvwxyz".repeat(50);
        let original_len = long_content.len();
        let msg = Message::new(Role::User, long_content);
        let messages: Vec<&Message> = vec![&msg];

        let prompt = service.build_summary_prompt(&messages);

        assert!(prompt.contains("..."));
        assert!(prompt.len() < original_len);
    }

    #[test]
    fn test_compression_result_fields() {
        let result = CompressionResult {
            original_count: 10,
            compacted_count: 3,
            summary: "Test summary".to_string(),
            preserved_ids: vec![],
            token_savings: TokenSavings::new(1000, 300),
            duration: std::time::Duration::from_millis(100),
        };

        assert_eq!(result.original_count, 10);
        assert_eq!(result.compacted_count, 3);
        assert_eq!(result.summary, "Test summary");
    }
}
