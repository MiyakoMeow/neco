//! Tool executor implementation with type-state pattern.
//!
//! This module provides backward-compatible wrapper while using type-state internally.

use std::sync::Arc;

use async_trait::async_trait;
use neoco_core::prompt::PromptLoader;
use neoco_core::{
    SecurityContext, SecurityManager, ToolContext, ToolError, ToolExecutor, ToolResult,
};
use serde_json::Value;

use crate::state::ExecutionState;

/// Legacy-compatible tool executor that wraps a typed executor internally.
///
/// This provides backward compatibility while using type-state pattern.
pub struct ToolExecutorImpl {
    /// Inner tool executor.
    inner: Arc<dyn ToolExecutor>,
    /// Execution state.
    state: std::sync::Mutex<ExecutionState>,
    /// Prompt loader.
    prompt_loader: Option<Arc<dyn PromptLoader>>,
    /// Security manager.
    security_manager: Option<Arc<SecurityManager>>,
}

impl ToolExecutorImpl {
    /// Create a new tool executor wrapper.
    pub fn new(inner: Arc<dyn ToolExecutor>) -> Self {
        Self {
            inner,
            state: std::sync::Mutex::new(ExecutionState::Idle),
            prompt_loader: None,
            security_manager: None,
        }
    }

    /// Create a new tool executor with prompt loader.
    pub fn with_prompt_loader(
        inner: Arc<dyn ToolExecutor>,
        prompt_loader: Arc<dyn PromptLoader>,
    ) -> Self {
        Self {
            inner,
            state: std::sync::Mutex::new(ExecutionState::Idle),
            prompt_loader: Some(prompt_loader),
            security_manager: None,
        }
    }

    /// Create a new tool executor with security manager.
    pub fn with_security_manager(
        inner: Arc<dyn ToolExecutor>,
        security_manager: Arc<SecurityManager>,
    ) -> Self {
        Self {
            inner,
            state: std::sync::Mutex::new(ExecutionState::Idle),
            prompt_loader: None,
            security_manager: Some(security_manager),
        }
    }

    /// Returns the current execution state.
    ///
    /// # Panics
    /// Panics if the internal mutex is poisoned.
    #[must_use]
    pub fn state(&self) -> ExecutionState {
        *self.state.lock().expect("lock poisoned")
    }

    /// Load tool prompt for a given tool ID.
    #[allow(dead_code)]
    fn load_tool_prompt(&self, tool_id: &str) -> Option<String> {
        if let Some(ref loader) = self.prompt_loader {
            loader.load_for_tool(tool_id).unwrap_or_default()
        } else {
            None
        }
    }

    /// Execute tool with type-state pattern.
    async fn execute_typed(
        inner: Arc<dyn ToolExecutor>,
        context: &ToolContext,
        args: Value,
        prompt_loader: Option<Arc<dyn PromptLoader>>,
        security_manager: Option<Arc<SecurityManager>>,
    ) -> Result<ToolResult, ToolError> {
        let tool_id = inner.definition().id.to_string();
        let prompt_component = inner.definition().prompt_component.clone();
        let tool_prompt = if let Some(ref loader) = prompt_loader {
            loader.load_for_tool(&tool_id).unwrap_or_default()
        } else {
            None
        };

        if let Some(ref prompt) = tool_prompt {
            tracing::debug!(
                "Loaded prompt component for tool '{}': {}",
                tool_id,
                prompt.chars().take(100).collect::<String>()
            );
        }

        let mut extended_args = args.clone();
        if let Some(ref component) = prompt_component
            && let Some(obj) = extended_args.as_object_mut()
        {
            obj.insert(
                "_prompt_component".to_string(),
                serde_json::json!(component),
            );
        }

        if let Some(ref security_mgr) = security_manager {
            let args_str = serde_json::to_string(&extended_args).unwrap_or_default();
            let security_ctx = SecurityContext::new()
                .with_tool_name(&tool_id)
                .with_input(&args_str)
                .with_session_id(context.session_ulid.to_string())
                .with_user_id(context.agent_ulid.to_string());

            let result = security_mgr.check(&security_ctx);

            if !result.is_allowed() {
                return Err(ToolError::ExecutionFailed(format!(
                    "Security check failed: {result:?}"
                )));
            }
        }

        let mut result = inner.execute(context, extended_args).await;

        if let Ok(ref mut r) = result {
            r.prompt_component = tool_prompt;
        }

        result
    }
}

#[async_trait]
impl ToolExecutor for ToolExecutorImpl {
    fn definition(&self) -> &neoco_core::ToolDefinition {
        self.inner.definition()
    }

    async fn execute(&self, context: &ToolContext, args: Value) -> Result<ToolResult, ToolError> {
        {
            let mut state = self.state.lock().expect("lock poisoned");
            if !state.can_execute() {
                return Err(ToolError::ExecutionFailed(format!(
                    "Tool is in state {state} and cannot execute"
                )));
            }
            *state = state.next_success();
        }

        let result = Self::execute_typed(
            Arc::clone(&self.inner),
            context,
            args,
            self.prompt_loader.clone(),
            self.security_manager.clone(),
        )
        .await;

        let mut state = self.state.lock().expect("lock poisoned");
        if result.is_ok() {
            *state = state.next_success();
        } else {
            *state = state.next_failure();
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use neoco_core::ids::ToolId;

    struct DummyTool;

    #[async_trait]
    impl ToolExecutor for DummyTool {
        fn definition(&self) -> &neoco_core::ToolDefinition {
            static DEF: std::sync::LazyLock<neoco_core::ToolDefinition> =
                std::sync::LazyLock::new(|| neoco_core::ToolDefinition {
                    id: ToolId::from_string("test::dummy").unwrap(),
                    description: "Test tool".to_string(),
                    schema: serde_json::json!({"type": "object"}),
                    capabilities: neoco_core::ToolCapabilities::default(),
                    timeout: std::time::Duration::from_secs(5),
                    category: neoco_core::ToolCategory::Common,
                    prompt_component: None,
                });
            &DEF
        }

        async fn execute(
            &self,
            _context: &ToolContext,
            _args: Value,
        ) -> Result<ToolResult, ToolError> {
            Ok(ToolResult {
                output: neoco_core::ToolOutput::Text("ok".to_string()),
                is_error: false,
                prompt_component: None,
            })
        }
    }

    #[tokio::test]
    async fn test_state_tracking() {
        let executor = ToolExecutorImpl::new(Arc::new(DummyTool));

        assert_eq!(executor.state(), ExecutionState::Idle);

        let ctx = ToolContext {
            session_ulid: neoco_core::SessionUlid::new(),
            agent_ulid: neoco_core::AgentUlid::new_root(&neoco_core::SessionUlid::new()),
            working_dir: std::path::PathBuf::from("/tmp"),
            user_interaction_tx: None,
        };

        let result = executor.execute(&ctx, serde_json::json!({})).await;
        result.unwrap();

        assert_eq!(executor.state(), ExecutionState::Validating);
    }
}
