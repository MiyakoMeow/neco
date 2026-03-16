//! Context observer module.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use neoco_core::errors::ToolError;
use neoco_core::ids::MessageId;
use neoco_core::messages::Role;
use neoco_core::tool::{
    ResourceLevel, ToolCapabilities, ToolCategory, ToolContext, ToolDefinition, ToolExecutor,
    ToolOutput, ToolResult,
};
use neoco_core::traits::MessageRepository;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

use crate::config::ContextConfig;
use crate::tokenizer::TokenCounter;
use crate::{CompressionService, ContextError};

/// Stage of context pruning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Serialize, Deserialize)]
pub enum PruningStage {
    /// Stage 1: Soft trim.
    Stage1SoftTrim,
    /// Stage 2: Hard clear.
    Stage2HardClear,
    /// Stage 3: Graded compression.
    Stage3Graded,
}

impl PruningStage {
    /// Creates a `PruningStage` from usage percentage.
    pub fn from_usage_percent(percent: f64) -> Option<Self> {
        if percent >= 0.9 {
            Some(Self::Stage3Graded)
        } else if percent >= 0.8 {
            Some(Self::Stage2HardClear)
        } else if percent >= 0.7 {
            Some(Self::Stage1SoftTrim)
        } else {
            None
        }
    }
}

impl std::fmt::Display for PruningStage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Stage1SoftTrim => write!(f, "Stage 1: Soft Trim"),
            Self::Stage2HardClear => write!(f, "Stage 2: Hard Clear"),
            Self::Stage3Graded => write!(f, "Stage 3: Graded Compression"),
        }
    }
}

/// Context statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextStats {
    /// Total message count.
    pub total_messages: usize,
    /// Total character count.
    pub total_chars: usize,
    /// Total token count.
    pub total_tokens: usize,
    /// Usage percentage.
    pub usage_percent: f64,
    /// Role counts.
    pub role_counts: HashMap<Role, usize>,
    /// Steps since last tag.
    pub steps_since_tag: usize,
    /// Last tag.
    pub last_tag: Option<String>,
    /// Current pruning stage.
    pub pruning_stage: Option<PruningStage>,
    /// Estimated turns left.
    pub estimated_turns_left: usize,
}

impl ContextStats {
    /// Creates new `ContextStats`.
    pub fn new() -> Self {
        Self {
            total_messages: 0,
            total_chars: 0,
            total_tokens: 0,
            usage_percent: 0.0,
            role_counts: HashMap::new(),
            steps_since_tag: 0,
            last_tag: None,
            pruning_stage: None,
            estimated_turns_left: 0,
        }
    }

    /// Calculates statistics from messages.
    pub fn calculate_from_messages(
        &mut self,
        messages: &[neoco_core::messages::Message],
        token_counter: &dyn TokenCounter,
        config: &ContextConfig,
    ) {
        self.total_messages = messages.len();
        self.total_tokens = token_counter.estimate_tokens(messages);
        self.total_chars = messages.iter().map(|m| m.content.chars().count()).sum();
        #[allow(clippy::cast_precision_loss)]
        let usage = self.total_tokens as f64 / config.context_window_tokens as f64;
        self.usage_percent = usage;
        self.pruning_stage = PruningStage::from_usage_percent(self.usage_percent);

        let mut counts: HashMap<Role, usize> = HashMap::new();
        for msg in messages {
            *counts.entry(msg.role).or_insert(0) += 1;
        }
        self.role_counts = counts;

        let (steps, last_tag) = Self::calculate_tag_stats(messages);
        self.steps_since_tag = steps;
        self.last_tag = last_tag;

        let avg_tokens_per_turn = 2000;
        if self.total_tokens < config.context_window_tokens {
            let remaining = config.context_window_tokens - self.total_tokens;
            self.estimated_turns_left = remaining / avg_tokens_per_turn;
        } else {
            self.estimated_turns_left = 0;
        }
    }

    /// Calculates tag statistics from messages.
    fn calculate_tag_stats(messages: &[neoco_core::messages::Message]) -> (usize, Option<String>) {
        let tag_pattern = "[TAG:";

        let mut last_tag_idx = None;
        for (idx, msg) in messages.iter().enumerate() {
            if msg.content.starts_with(tag_pattern)
                && let Some(end) = msg.content[tag_pattern.len()..].find(']')
            {
                let tag = msg.content[tag_pattern.len()..tag_pattern.len() + end].to_string();
                last_tag_idx = Some((idx, tag));
            }
        }

        if let Some((idx, tag)) = last_tag_idx {
            let steps = messages.len() - idx - 1;
            (steps, Some(tag))
        } else {
            (messages.len(), None)
        }
    }
}

impl Default for ContextStats {
    fn default() -> Self {
        Self::new()
    }
}

/// Summary of a message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageSummary {
    /// Message ID.
    pub id: MessageId,
    /// Message role.
    pub role: Role,
    /// Preview of the content.
    pub content_preview: String,
    /// Size in characters.
    pub size_chars: usize,
    /// Size in tokens.
    pub size_tokens: usize,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
}

impl MessageSummary {
    /// Creates a `MessageSummary` from a message.
    pub fn from_message(
        message: &neoco_core::messages::Message,
        token_counter: &dyn TokenCounter,
    ) -> Self {
        let content_preview = if message.content.len() > 100 {
            format!("{}...", &message.content[..100])
        } else {
            message.content.clone()
        };
        Self {
            id: message.id,
            role: message.role,
            content_preview,
            size_chars: message.content.chars().count(),
            size_tokens: token_counter.estimate_message_tokens(message),
            timestamp: message.timestamp,
        }
    }
}

/// Filter for context observation.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ContextFilter {
    /// Filter by roles.
    pub roles: Option<Vec<Role>>,
    /// Minimum message ID.
    pub min_id: Option<MessageId>,
    /// Maximum message ID.
    pub max_id: Option<MessageId>,
    /// Filter by tool calls.
    pub with_tool_calls: Option<bool>,
}

impl ContextFilter {
    /// Checks if a message matches the filter.
    pub fn matches(&self, message: &neoco_core::messages::Message) -> bool {
        if let Some(ref roles) = self.roles
            && !roles.contains(&message.role)
        {
            return false;
        }
        if let Some(min) = self.min_id
            && message.id < min
        {
            return false;
        }
        if let Some(max) = self.max_id
            && message.id > max
        {
            return false;
        }
        if let Some(with_tc) = self.with_tool_calls {
            let has_tc = message.tool_calls.is_some();
            if has_tc != with_tc {
                return false;
            }
        }
        true
    }
}

/// Result of context observation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextObservation {
    /// Messages.
    pub messages: Vec<MessageSummary>,
    /// Statistics.
    pub stats: ContextStats,
}

/// Context observer trait.
#[async_trait]
pub trait ContextObserver: Send + Sync {
    /// Observes the context.
    async fn observe(
        &self,
        agent_ulid: &neoco_core::AgentUlid,
        filter: Option<ContextFilter>,
    ) -> Result<ContextObservation, ContextError>;
}

/// Context observer implementation.
pub struct ContextObserverImpl {
    /// Message repository.
    message_repo: Arc<dyn MessageRepository>,
    /// Token counter.
    token_counter: Arc<dyn TokenCounter>,
    /// Configuration.
    config: ContextConfig,
}

impl ContextObserverImpl {
    /// Creates a new `ContextObserverImpl`.
    pub fn new(
        message_repo: Arc<dyn MessageRepository>,
        token_counter: Arc<dyn TokenCounter>,
        config: ContextConfig,
    ) -> Self {
        Self {
            message_repo,
            token_counter,
            config,
        }
    }
}

#[async_trait]
impl ContextObserver for ContextObserverImpl {
    async fn observe(
        &self,
        agent_ulid: &neoco_core::AgentUlid,
        filter: Option<ContextFilter>,
    ) -> Result<ContextObservation, ContextError> {
        let messages = self
            .message_repo
            .list(agent_ulid)
            .await
            .map_err(|e| ContextError::Storage(e.to_string()))?;

        let filtered: Vec<_> = if let Some(ref f) = filter {
            messages.into_iter().filter(|m| f.matches(m)).collect()
        } else {
            messages
        };

        let mut stats = ContextStats::new();
        stats.calculate_from_messages(&filtered, self.token_counter.as_ref(), &self.config);

        let summaries: Vec<_> = filtered
            .iter()
            .map(|m| MessageSummary::from_message(m, self.token_counter.as_ref()))
            .collect();

        Ok(ContextObservation {
            messages: summaries,
            stats,
        })
    }
}

/// Tool for observing context.
pub struct ContextObserveTool {
    /// Context observer.
    observer: Arc<dyn ContextObserver>,
}

impl ContextObserveTool {
    /// Creates a new `ContextObserveTool`.
    pub fn new(observer: Arc<dyn ContextObserver>) -> Self {
        Self { observer }
    }
}

/// Arguments for observe tool.
#[derive(Debug, Deserialize)]
struct ObserveArgs {
    /// Agent ID to observe.
    agent_id: Option<String>,
}

#[async_trait]
impl ToolExecutor for ContextObserveTool {
    fn definition(&self) -> &ToolDefinition {
        static DEFINITION: std::sync::OnceLock<ToolDefinition> = std::sync::OnceLock::new();
        DEFINITION.get_or_init(|| ToolDefinition {
            id: "context::observe".parse().unwrap(),
            description: "观测上下文状态，获取内存使用仪表盘".to_string(),
            schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "agent_id": {
                        "type": "string",
                        "description": "Agent ID (可选，默认当前Agent)"
                    }
                }
            }),
            capabilities: ToolCapabilities {
                streaming: false,
                requires_network: false,
                resource_level: ResourceLevel::Low,
                concurrent: false,
            },
            timeout: Duration::from_secs(30),
            category: ToolCategory::Common,
            prompt_component: None,
        })
    }

    async fn execute(&self, context: &ToolContext, args: Value) -> Result<ToolResult, ToolError> {
        let args: ObserveArgs =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidArgs(e.to_string()))?;

        let agent_ulid = if let Some(agent_id) = args.agent_id {
            agent_id
                .parse::<neoco_core::AgentUlid>()
                .map_err(|e| ToolError::InvalidArgs(format!("Invalid agent ID: {e}")))?
        } else {
            context.agent_ulid
        };

        let observation = self
            .observer
            .observe(&agent_ulid, None)
            .await
            .map_err(|e| ToolError::Execution(std::io::Error::other(e.to_string())))?;

        let stats = &observation.stats;
        let output = format!(
            "[Context Dashboard]
• Usage:           {:.1}% ({}/{} tokens)
• Steps since tag: {} {}
• Pruning status:  {}
• Est. turns left: ~{}
• Total messages:  {}",
            stats.usage_percent * 100.0,
            stats.total_tokens,
            128 * 1024,
            stats.steps_since_tag,
            stats
                .last_tag
                .as_ref()
                .map(|t| format!("(last: '{t}')"))
                .unwrap_or_default(),
            stats
                .pruning_stage
                .map_or_else(|| "Normal".to_string(), |s| s.to_string()),
            stats.estimated_turns_left,
            stats.total_messages,
        );

        Ok(ToolResult {
            output: ToolOutput::Text(output),
            is_error: false,
            prompt_component: None,
        })
    }
}

/// Tool for compacting context.
pub struct ContextCompactTool<S: CompressionService> {
    /// Compression service.
    compression_service: Arc<S>,
}

impl<S: CompressionService> ContextCompactTool<S> {
    /// Creates a new `ContextCompactTool`.
    pub fn new(compression_service: Arc<S>) -> Self {
        Self {
            compression_service,
        }
    }
}

/// Arguments for compact tool.
#[derive(Debug, Deserialize)]
struct CompactArgs {
    /// Agent ID to compact.
    agent_id: Option<String>,
    /// Tag to start compression from.
    tag: Option<String>,
}

#[async_trait]
impl<S: CompressionService + 'static> ToolExecutor for ContextCompactTool<S> {
    fn definition(&self) -> &ToolDefinition {
        static DEFINITION: std::sync::OnceLock<ToolDefinition> = std::sync::OnceLock::new();
        DEFINITION.get_or_init(|| ToolDefinition {
            id: "context::compact".parse().unwrap(),
            description: "主动压缩上下文，将历史消息压缩为摘要（Agent主动管理内存）".to_string(),
            schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "agent_id": {
                        "type": "string",
                        "description": "Agent ID (可选，默认当前Agent)"
                    },
                    "tag": {
                        "type": "string",
                        "description": "压缩起点标记，从该标记到当前位置的消息将被压缩"
                    }
                }
            }),
            capabilities: ToolCapabilities {
                streaming: false,
                requires_network: true,
                resource_level: ResourceLevel::High,
                concurrent: false,
            },
            timeout: Duration::from_secs(120),
            category: ToolCategory::Common,
            prompt_component: None,
        })
    }

    async fn execute(&self, context: &ToolContext, args: Value) -> Result<ToolResult, ToolError> {
        let args: CompactArgs =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidArgs(e.to_string()))?;

        let agent_ulid = if let Some(agent_id) = args.agent_id {
            agent_id
                .parse::<neoco_core::AgentUlid>()
                .map_err(|e| ToolError::InvalidArgs(format!("Invalid agent ID: {e}")))?
        } else {
            context.agent_ulid
        };

        let tag = args.tag.as_deref();

        let summary = self
            .compression_service
            .compress(&agent_ulid, tag)
            .await
            .map_err(|e| ToolError::Execution(std::io::Error::other(e)))?;

        let output = format!(
            "Context compacted successfully.\n\nSummary:\n{}",
            if summary.len() > 500 {
                format!("{}...", &summary[..500])
            } else {
                summary
            }
        );

        Ok(ToolResult {
            output: ToolOutput::Text(output),
            is_error: false,
            prompt_component: None,
        })
    }
}
