//! Context manager module.

use async_trait::async_trait;
use std::sync::Arc;
use std::time::Instant;

use neoco_core::AgentUlid;
use neoco_core::ids::MessageId;
use neoco_core::messages::{Message, ModelMessage, Role};
use neoco_core::traits::MessageRepository;

use crate::CompressionService;
use crate::compression::CompactResult;
use crate::config::{ContextConfig, TokenSavings};
use crate::observer::{ContextObserver, ContextObserverImpl, ContextStats, PruningStage};
use crate::tokenizer::TokenCounter;

/// Error type for context management.
#[derive(Debug, thiserror::Error)]
pub enum ContextError {
    /// Agent not found.
    #[error("Agent不存在: {0}")]
    AgentNotFound(AgentUlid),

    /// Model error.
    #[error("模型调用错误: {0}")]
    Model(String),

    /// Nothing to compact.
    #[error("没有可压缩的消息")]
    NothingToCompact,

    /// Token calculation error.
    #[error("Token计算错误: {0}")]
    TokenCalculation(String),

    /// Configuration error.
    #[error("配置错误: {0}")]
    Config(String),

    /// Storage error.
    #[error("存储错误: {0}")]
    Storage(String),
}

/// Context manager trait.
#[async_trait]
pub trait ContextManager: Send + Sync {
    /// Builds context for an agent.
    async fn build_context(
        &self,
        agent_ulid: &AgentUlid,
        max_tokens: usize,
    ) -> Result<Vec<neoco_core::messages::OwnedModelMessage>, ContextError>;

    /// Checks if compaction should be triggered.
    async fn should_compact(&self, agent_ulid: &AgentUlid) -> bool;

    /// Compacts the context.
    async fn compact(
        &self,
        agent_ulid: &AgentUlid,
        tag: Option<&str>,
    ) -> Result<CompactResult, ContextError>;

    /// Gets context statistics.
    async fn get_stats(&self, agent_ulid: &AgentUlid) -> Result<ContextStats, ContextError>;
}

/// Context manager implementation.
pub struct ContextManagerImpl {
    /// Message repository.
    message_repo: Arc<dyn MessageRepository>,
    /// Token counter.
    token_counter: Arc<dyn TokenCounter>,
    /// Compression service.
    compression_service: Arc<dyn CompressionService>,
    /// Configuration.
    config: ContextConfig,
    /// Context observer.
    observer: Arc<ContextObserverImpl>,
}

impl ContextManagerImpl {
    /// Creates a new `ContextManagerImpl`.
    pub fn new(
        message_repo: Arc<dyn MessageRepository>,
        token_counter: Arc<dyn TokenCounter>,
        compression_service: Arc<dyn CompressionService>,
        config: ContextConfig,
    ) -> Self {
        let observer = Arc::new(ContextObserverImpl::new(
            message_repo.clone(),
            token_counter.clone(),
            config.clone(),
        ));
        Self {
            message_repo,
            token_counter,
            compression_service,
            config,
            observer,
        }
    }

    /// Gets the context observer.
    pub fn observer(&self) -> Arc<dyn ContextObserver> {
        self.observer.clone()
    }

    /// Gets messages for an agent.
    async fn get_messages(&self, agent_ulid: &AgentUlid) -> Result<Vec<Message>, ContextError> {
        self.message_repo
            .list(agent_ulid)
            .await
            .map_err(|e| ContextError::Storage(e.to_string()))
    }

    /// Truncates messages by token limit.
    fn truncate_by_tokens(
        &self,
        messages: &[Message],
        max_tokens: usize,
    ) -> Vec<neoco_core::messages::OwnedModelMessage> {
        let mut result = Vec::new();
        let mut current_tokens = 0;

        for msg in messages.iter().rev() {
            let msg_tokens = self.token_counter.estimate_message_tokens(msg);
            if current_tokens + msg_tokens > max_tokens {
                break;
            }
            current_tokens += msg_tokens;
            let model_msg = ModelMessage::from_message(msg);
            result.push(model_msg.into_owned());
        }

        result.reverse();
        result
    }

    /// Selects messages to compact.
    #[allow(dead_code)]
    fn select_messages_to_compact(
        messages: &[Message],
        keep_recent: usize,
    ) -> (Vec<MessageId>, Vec<&Message>) {
        if messages.len() <= keep_recent {
            return (Vec::new(), Vec::new());
        }

        let to_compact = &messages[..messages.len() - keep_recent];
        let preserve_ids: Vec<MessageId> = messages[messages.len() - keep_recent..]
            .iter()
            .map(|m| m.id)
            .collect();

        let compact_refs: Vec<&Message> = to_compact.iter().collect();
        (preserve_ids, compact_refs)
    }

    /// Generates a prompt for summarization.
    #[allow(dead_code)]
    fn generate_summary_prompt(messages: &[&Message]) -> String {
        let mut prompt = String::from(
            "Please summarize the following conversation history concisely, preserving key information, decisions, and important context:\n\n",
        );

        for msg in messages.iter().take(20) {
            let role_str = match msg.role {
                Role::System => "System",
                Role::User => "User",
                Role::Assistant => "Assistant",
                Role::Tool => "Tool",
            };
            let content = if msg.content.len() > 500 {
                format!("{}...", &msg.content[..500])
            } else {
                msg.content.clone()
            };
            prompt.push_str(role_str);
            prompt.push_str(": ");
            prompt.push_str(&content);
            prompt.push_str("\n\n");
        }

        prompt
    }

    /// Stage 1: Soft trim long tool results.
    #[allow(dead_code)]
    fn prune_messages_stage1(messages: &mut [Message]) {
        for msg in messages.iter_mut() {
            if msg.role == Role::Tool && msg.content.len() > 2000 {
                msg.content = format!("[Tool result truncated: {} chars]", msg.content.len());
            }
        }
    }

    /// Stage 2: Clear all tool results.
    #[allow(dead_code)]
    fn prune_messages_stage2(messages: &mut [Message]) {
        for msg in messages.iter_mut() {
            if msg.role == Role::Tool {
                msg.content = "[Tool result placeholder]".to_string();
            }
        }
    }

    /// Stage 3: Truncate long content.
    fn prune_messages_stage3(messages: &mut [Message]) {
        for msg in messages.iter_mut() {
            if msg.content.len() > 500 {
                msg.content = format!("[Content truncated: {} chars]", msg.content.len());
            }
        }
    }

    /// Applies pruning to messages.
    async fn apply_pruning(
        &self,
        agent_ulid: &AgentUlid,
        stage: PruningStage,
    ) -> Result<(), ContextError> {
        let mut messages = self.get_messages(agent_ulid).await?;

        match stage {
            PruningStage::Stage1SoftTrim => Self::prune_messages_stage1(&mut messages),
            PruningStage::Stage2HardClear => Self::prune_messages_stage2(&mut messages),
            PruningStage::Stage3Graded => Self::prune_messages_stage3(&mut messages),
        }

        for msg in &mut messages {
            self.message_repo
                .append(agent_ulid, msg)
                .await
                .map_err(|e| ContextError::Storage(e.to_string()))?;
        }

        Ok(())
    }

    /// Checks and applies auto-pruning if needed.
    async fn check_and_auto_prune(&self, agent_ulid: &AgentUlid) {
        let Ok(messages) = self.get_messages(agent_ulid).await else {
            return;
        };

        let token_count = self.token_counter.estimate_tokens(&messages);
        #[allow(clippy::cast_precision_loss)]
        let usage_percent = token_count as f64 / self.config.context_window_tokens as f64;

        if let Some(stage) = PruningStage::from_usage_percent(usage_percent)
            && let Err(e) = self.apply_pruning(agent_ulid, stage).await
        {
            tracing::warn!("Auto pruning failed: {}", e);
        }
    }
}

#[async_trait]
impl ContextManager for ContextManagerImpl {
    async fn build_context(
        &self,
        agent_ulid: &AgentUlid,
        max_tokens: usize,
    ) -> Result<Vec<neoco_core::messages::OwnedModelMessage>, ContextError> {
        let messages = self.get_messages(agent_ulid).await?;

        if messages.is_empty() {
            return Ok(Vec::new());
        }

        self.check_and_auto_prune(agent_ulid).await;

        let truncated = self.truncate_by_tokens(&messages, max_tokens);

        Ok(truncated)
    }

    async fn should_compact(&self, agent_ulid: &AgentUlid) -> bool {
        if !self.config.auto_compact_enabled {
            return false;
        }

        let Ok(messages) = self.get_messages(agent_ulid).await else {
            return false;
        };

        let token_count = self.token_counter.estimate_tokens(&messages);
        let threshold = self.config.auto_compact_threshold_tokens();

        token_count >= threshold
    }

    async fn compact(
        &self,
        agent_ulid: &AgentUlid,
        tag: Option<&str>,
    ) -> Result<CompactResult, ContextError> {
        let messages = self.get_messages(agent_ulid).await?;

        if messages.is_empty() {
            return Err(ContextError::NothingToCompact);
        }

        let original_count = messages.len();
        let before_tokens = self.token_counter.estimate_tokens(&messages);
        let start = Instant::now();

        let summary = self
            .compression_service
            .compress(agent_ulid, tag)
            .await
            .map_err(ContextError::Model)?;

        let summary_message = Message::system(summary);

        let compacted_messages = self.get_messages(agent_ulid).await?;
        let after_tokens = self.token_counter.estimate_tokens(&compacted_messages);

        let duration = start.elapsed();
        #[allow(clippy::cast_possible_truncation)]
        let token_savings = TokenSavings::new(before_tokens as u32, after_tokens as u32);

        Ok(CompactResult {
            original_count,
            compacted_count: compacted_messages.len(),
            summary: summary_message.content,
            preserved_ids: Vec::new(),
            token_savings,
            duration,
        })
    }

    async fn get_stats(&self, agent_ulid: &AgentUlid) -> Result<ContextStats, ContextError> {
        let observation = self.observer.observe(agent_ulid, None).await?;
        Ok(observation.stats)
    }
}
