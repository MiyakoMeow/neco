//! NeoCo Kernel - Core trait for the NeoCo application.
//!
//! This module provides the `NeoCoKernel` trait as the unified interface
//! for all runtime modes (CLI, TUI).

use crate::traits::Session;
use async_trait::async_trait;
use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use crate::ToolExecutor;
use crate::ids::{MessageId, SessionUlid};
use crate::messages::Role;

use neoco_config::Config;

/// Kernel errors.
#[derive(Debug, thiserror::Error)]
pub enum KernelError {
    /// Session not found.
    #[error("Session不存在: {0}")]
    SessionNotFound(SessionUlid),

    /// Agent not found.
    #[error("Agent不存在: {0}")]
    AgentNotFound(String),

    /// Kernel not initialized.
    #[error("Kernel未初始化")]
    NotInitialized,

    /// Configuration error.
    #[error("配置错误: {0}")]
    Config(String),

    /// Execution error.
    #[error("执行错误: {0}")]
    Execution(String),

    /// Storage error.
    #[error("存储错误: {0}")]
    Storage(String),
}

/// Session configuration for creating a new session.
#[derive(Debug, Clone, Default)]
pub struct SessionConfig {
    /// Model configuration (model name or model group).
    pub model: Option<String>,
    /// System prompt.
    pub system_prompt: Option<String>,
    /// Skills to activate.
    pub skills: Vec<String>,
    /// MCP servers to connect.
    pub mcp_servers: Vec<String>,
}

/// NeoCo Kernel trait.
///
/// This trait defines the unified interface for all runtime modes:
/// - CLI mode: single command execution
/// - TUI mode: interactive terminal interface
///
/// # Design Note
///
/// The `shutdown()` method uses `async fn` instead of `fn shutdown() -> impl Future`.
/// This is because NeoCoKernel is primarily used internally via direct calls, not as
/// a `dyn NeoCoKernel` trait object. If trait object support is needed in the future,
/// consider using `async_trait` or `Box<dyn Future>`.
pub trait NeoCoKernel: Send + Sync {
    /// Get the agent engine.
    fn agent_engine(&self) -> Arc<dyn AgentEngine>;

    /// Get the tool registry.
    fn tool_registry(&self) -> Arc<dyn ToolRegistry>;

    /// Get the context manager.
    fn context_manager(&self) -> Arc<dyn ContextManager>;

    /// Get the session manager.
    fn session_manager(&self) -> Arc<dyn SessionManager>;

    /// Get the config.
    fn config(&self) -> Arc<Config>;

    /// Run a session with the given input.
    ///
    /// # Arguments
    ///
    /// * `session_id` - The session ID to run
    ///
    /// # Errors
    ///
    /// Returns an error if the session doesn't exist or execution fails.
    fn run(&self, session_id: SessionUlid) -> impl Future<Output = Result<(), KernelError>> + Send;

    /// Create a new session with the given configuration.
    ///
    /// # Arguments
    ///
    /// * `config` - The session configuration
    ///
    /// # Errors
    ///
    /// Returns an error if session creation fails.
    fn create_session(
        &self,
        config: SessionConfig,
    ) -> impl Future<Output = Result<SessionUlid, KernelError>> + Send;

    /// Load an existing session.
    ///
    /// # Arguments
    ///
    /// * `session_id` - The session ID to load
    ///
    /// # Errors
    ///
    /// Returns an error if the session doesn't exist.
    fn load_session(
        &self,
        session_id: SessionUlid,
    ) -> impl Future<Output = Result<Session, KernelError>> + Send;

    /// Send a message to a session.
    ///
    /// # Arguments
    ///
    /// * `session_id` - The session ID
    /// * `message` - The message content
    ///
    /// # Errors
    ///
    /// Returns an error if the session doesn't exist or sending fails.
    fn send_message(
        &self,
        session_id: SessionUlid,
        message: String,
    ) -> impl Future<Output = Result<(), KernelError>> + Send;

    /// Shutdown the kernel and release all resources.
    fn shutdown(&self) -> impl Future<Output = ()> + Send;
}

/// Agent engine trait.
#[async_trait]
pub trait AgentEngine: Send + Sync {
    /// Process an agent with the given session and input.
    async fn process(
        &self,
        session: &crate::traits::Session,
        input: &str,
    ) -> Result<crate::traits::AgentOutput, AgentEngineError>;
}

/// Agent engine error.
#[derive(Debug, thiserror::Error)]
pub enum AgentEngineError {
    /// Agent not found.
    #[error("Agent不存在: {0}")]
    NotFound(String),

    /// Execution failed.
    #[error("执行失败: {0}")]
    ExecutionFailed(String),
}

/// Tool registry trait.
#[async_trait]
pub trait ToolRegistry: Send + Sync {
    /// Register a tool.
    async fn register(&self, tool: Arc<dyn ToolExecutor>);

    /// Unregister a tool.
    async fn unregister(&self, tool_id: &crate::ids::ToolId);

    /// Get a tool by ID.
    async fn get(&self, tool_id: &crate::ids::ToolId) -> Option<Arc<dyn ToolExecutor>>;
}

/// Token savings from compression.
#[derive(Debug, Clone, Default)]
pub struct TokenSavings {
    /// Token count before compression.
    pub before_tokens: u32,
    /// Token count after compression.
    pub after_tokens: u32,
    /// Saved tokens.
    pub saved_tokens: u32,
    /// Savings percentage.
    pub savings_percent: f64,
}

impl TokenSavings {
    /// Creates a new `TokenSavings`.
    pub fn new(before_tokens: u32, after_tokens: u32) -> Self {
        let saved_tokens = before_tokens.saturating_sub(after_tokens);
        #[allow(clippy::cast_precision_loss)]
        let savings_percent = if before_tokens > 0 {
            (f64::from(saved_tokens) / f64::from(before_tokens)) * 100.0
        } else {
            0.0
        };
        Self {
            before_tokens,
            after_tokens,
            saved_tokens,
            savings_percent,
        }
    }
}

/// Pruning stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PruningStage {
    /// Stage 1: Soft trim.
    Stage1SoftTrim,
    /// Stage 2: Hard clear.
    Stage2HardClear,
    /// Stage 3: Graded compression.
    Stage3Graded,
}

/// Context statistics.
#[derive(Debug, Clone, Default)]
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

/// Compact result.
pub struct CompactResult {
    /// Original message count.
    pub original_count: usize,
    /// Compacted message count.
    pub compacted_count: usize,
    /// Summary content.
    pub summary: String,
    /// Preserved message IDs.
    pub preserved_ids: Vec<MessageId>,
    /// Token savings.
    pub token_savings: TokenSavings,
    /// Duration of compression.
    pub duration: Duration,
}

/// Context manager trait.
#[async_trait]
pub trait ContextManager: Send + Sync {
    /// Build context for an agent.
    async fn build_context(
        &self,
        agent_ulid: &crate::ids::AgentUlid,
        max_tokens: usize,
    ) -> Result<Vec<crate::messages::OwnedModelMessage>, ContextError>;

    /// Check if compaction is needed.
    async fn should_compact(&self, agent_ulid: &crate::ids::AgentUlid) -> bool;

    /// Compact context.
    async fn compact(
        &self,
        agent_ulid: &crate::ids::AgentUlid,
        tag: Option<&str>,
    ) -> Result<CompactResult, ContextError>;

    /// Gets context statistics.
    async fn get_stats(
        &self,
        agent_ulid: &crate::ids::AgentUlid,
    ) -> Result<ContextStats, ContextError>;
}

/// Context error.
#[derive(Debug, thiserror::Error)]
pub enum ContextError {
    /// Agent not found.
    #[error("Agent不存在: {0}")]
    AgentNotFound(String),

    /// Computation failed.
    #[error("计算失败: {0}")]
    ComputationFailed(String),
}

/// Session manager trait.
#[async_trait]
pub trait SessionManager: Send + Sync {
    /// Create a new session.
    async fn create_session(
        &self,
        config: SessionConfig,
    ) -> Result<(SessionUlid, crate::ids::AgentUlid), KernelError>;

    /// Load a session.
    async fn load_session(&self, session_id: SessionUlid) -> Result<Session, KernelError>;

    /// Delete a session.
    async fn delete_session(&self, session_id: SessionUlid) -> Result<(), KernelError>;

    /// Get root agent ID for a session.
    async fn get_root_agent(
        &self,
        session_id: SessionUlid,
    ) -> Result<crate::ids::AgentUlid, KernelError>;
}

/// Default implementation of NeoCoKernel.
pub struct DefaultNeoCoKernel {
    /// Agent engine.
    agent_engine: Arc<dyn AgentEngine>,
    /// Tool registry.
    tool_registry: Arc<dyn ToolRegistry>,
    /// Context manager.
    context_manager: Arc<dyn ContextManager>,
    /// Session manager.
    session_manager: Arc<dyn SessionManager>,
    /// Config.
    config: Arc<Config>,
}

impl DefaultNeoCoKernel {
    /// Create a new kernel instance.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        agent_engine: Arc<dyn AgentEngine>,
        tool_registry: Arc<dyn ToolRegistry>,
        context_manager: Arc<dyn ContextManager>,
        session_manager: Arc<dyn SessionManager>,
        config: Arc<Config>,
    ) -> Self {
        Self {
            agent_engine,
            tool_registry,
            context_manager,
            session_manager,
            config,
        }
    }
}

impl NeoCoKernel for DefaultNeoCoKernel {
    fn agent_engine(&self) -> Arc<dyn AgentEngine> {
        self.agent_engine.clone()
    }

    fn tool_registry(&self) -> Arc<dyn ToolRegistry> {
        self.tool_registry.clone()
    }

    fn context_manager(&self) -> Arc<dyn ContextManager> {
        self.context_manager.clone()
    }

    fn session_manager(&self) -> Arc<dyn SessionManager> {
        self.session_manager.clone()
    }

    fn config(&self) -> Arc<Config> {
        self.config.clone()
    }

    fn run(&self, session_id: SessionUlid) -> impl Future<Output = Result<(), KernelError>> + Send {
        let session_manager = self.session_manager.clone();
        async move {
            let _session = session_manager.load_session(session_id).await?;
            Ok(())
        }
    }

    fn create_session(
        &self,
        config: SessionConfig,
    ) -> impl Future<Output = Result<SessionUlid, KernelError>> + Send {
        let session_manager = self.session_manager.clone();
        async move {
            let (session_id, _) = session_manager.create_session(config).await?;
            Ok(session_id)
        }
    }

    fn load_session(
        &self,
        session_id: SessionUlid,
    ) -> impl Future<Output = Result<Session, KernelError>> + Send {
        let session_manager = self.session_manager.clone();
        async move { session_manager.load_session(session_id).await }
    }

    fn send_message(
        &self,
        session_id: SessionUlid,
        message: String,
    ) -> impl Future<Output = Result<(), KernelError>> + Send {
        let session_manager = self.session_manager.clone();
        let agent_engine = self.agent_engine.clone();
        async move {
            let session = session_manager.load_session(session_id).await?;

            let _output = agent_engine
                .process(&session, &message)
                .await
                .map_err(|e| KernelError::Execution(e.to_string()))?;

            Ok(())
        }
    }

    async fn shutdown(&self) {}
}
