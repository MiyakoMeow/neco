//! Tool types and traits for NeoCo.

use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;

use crate::errors::ToolError;
use crate::ids::{AgentUlid, SessionUlid, ToolId};

use tokio::sync::mpsc;

/// Tool capabilities configuration.
#[derive(Debug, Clone, Default)]
pub struct ToolCapabilities {
    /// Whether the tool supports streaming responses.
    pub streaming: bool,
    /// Whether the tool requires network access.
    pub requires_network: bool,
    /// Resource level requirements for the tool.
    pub resource_level: ResourceLevel,
    /// Whether the tool can be executed concurrently.
    pub concurrent: bool,
}

/// Resource level for tool execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ResourceLevel {
    /// Low resource usage.
    #[default]
    Low,
    /// Medium resource usage.
    Medium,
    /// High resource usage.
    High,
}

/// Tool category for determining availability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ToolCategory {
    /// Available in all contexts.
    #[default]
    Common,
    /// Available in TUI context.
    Tui,
    /// Available in CLI context.
    Cli,
}

/// Tool definition containing metadata and schema.
#[derive(Debug, Clone)]
pub struct ToolDefinition {
    /// ID of the tool (namespace::name format).
    pub id: ToolId,
    /// Description of the tool.
    pub description: String,
    /// JSON schema for tool arguments.
    pub schema: serde_json::Value,
    /// Tool capabilities.
    pub capabilities: ToolCapabilities,
    /// Execution timeout duration.
    pub timeout: Duration,
    /// Tool category.
    pub category: ToolCategory,
    /// Optional prompt component for the tool.
    pub prompt_component: Option<String>,
}

/// Tool execution context containing session and agent information.
pub struct ToolContext {
    /// Session ULID.
    pub session_ulid: SessionUlid,
    /// Agent ULID.
    pub agent_ulid: AgentUlid,
    /// Working directory for tool execution.
    pub working_dir: PathBuf,
    /// Channel for user interaction (used by tui::question tool).
    /// Format: (question, response_sender)
    pub user_interaction_tx: Option<mpsc::Sender<(String, mpsc::Sender<String>)>>,
}

/// Tool execution result.
#[derive(Debug, Clone)]
pub struct ToolResult {
    /// Tool output.
    pub output: ToolOutput,
    /// Whether the result is an error.
    pub is_error: bool,
    /// Prompt component content loaded for this tool execution.
    pub prompt_component: Option<String>,
}

/// Tool output types.
#[derive(Debug, Clone)]
pub enum ToolOutput {
    /// Text output.
    Text(String),
    /// JSON output.
    Json(serde_json::Value),
    /// Binary output.
    Binary(Vec<u8>),
    /// Empty output.
    Empty,
}

/// Tool executor trait for implementing tool logic.
#[async_trait]
pub trait ToolExecutor: Send + Sync {
    /// Returns the tool definition.
    fn definition(&self) -> &ToolDefinition;

    /// Executes the tool with the given context and arguments.
    async fn execute(
        &self,
        context: &ToolContext,
        args: serde_json::Value,
    ) -> Result<ToolResult, ToolError>;
}

/// Tool registry trait for managing tool registration and retrieval.
#[async_trait]
pub trait ToolRegistry: Send + Sync {
    /// Registers a tool with the registry.
    async fn register(&self, tool: std::sync::Arc<dyn ToolExecutor>);

    /// Gets a tool by ID.
    async fn get(&self, id: &ToolId) -> Option<std::sync::Arc<dyn ToolExecutor>>;

    /// Returns all tool definitions.
    async fn definitions(&self) -> Vec<ToolDefinition>;

    /// Returns the timeout for a tool.
    async fn timeout(&self, id: &ToolId) -> Option<Duration>;

    /// Sets the timeout for tools matching a prefix.
    async fn set_timeout(&self, prefix: &str, duration: Duration);

    /// Lists all registered tool IDs.
    async fn list_tools(&self) -> Vec<ToolId>;

    /// Unregisters a tool from the registry.
    async fn unregister(&self, id: &ToolId);
}
