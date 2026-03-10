//! TUI tools.

use std::sync::LazyLock;
use std::time::Duration;

use async_trait::async_trait;
use neoco_core::ids::ToolId;
use neoco_core::{
    ToolCapabilities, ToolCategory, ToolContext, ToolDefinition, ToolError, ToolExecutor,
    ToolOutput, ToolResult,
};
use serde_json::{Map, Value};
use tokio::sync::mpsc;

/// 创建问题参数的 JSON Schema。
fn make_question_schema() -> Value {
    let mut map = Map::new();
    map.insert("type".to_string(), Value::String("object".to_string()));
    map.insert(
        "$schema".to_string(),
        Value::String("https://json-schema.org/draft/2020-12/schema".to_string()),
    );
    let mut props = Map::new();
    let mut q_prop = Map::new();
    q_prop.insert("type".to_string(), Value::String("string".to_string()));
    q_prop.insert(
        "description".to_string(),
        Value::String("要询问用户的问题".to_string()),
    );
    props.insert("question".to_string(), Value::Object(q_prop));
    let mut timeout_prop = Map::new();
    timeout_prop.insert("type".to_string(), Value::String("integer".to_string()));
    timeout_prop.insert(
        "description".to_string(),
        Value::String("等待用户响应的超时时间（秒），默认30秒".to_string()),
    );
    timeout_prop.insert(
        "default".to_string(),
        Value::Number(serde_json::Number::from(30)),
    );
    props.insert("timeout".to_string(), Value::Object(timeout_prop));
    map.insert("properties".to_string(), Value::Object(props));
    map.insert(
        "required".to_string(),
        Value::Array(vec![Value::String("question".to_string())]),
    );
    Value::Object(map)
}

/// 问题参数的 JSON Schema。
static QUESTION_SCHEMA: LazyLock<Value> = LazyLock::new(make_question_schema);

/// 问题工具定义。
static QUESTION_DEFINITION: LazyLock<ToolDefinition> = LazyLock::new(|| ToolDefinition {
    id: ToolId::from_string("tui::question").unwrap(),
    description: "向用户提问（仅限TUI非no-ask模式）".to_string(),
    schema: QUESTION_SCHEMA.clone(),
    capabilities: ToolCapabilities {
        streaming: false,
        requires_network: false,
        resource_level: neoco_core::ResourceLevel::Low,
        concurrent: false,
    },
    timeout: Duration::from_secs(30),
    category: ToolCategory::Tui,
    prompt_component: Some("tool::tui::question".to_string()),
});

/// TUI 问题询问工具。
///
/// 此工具仅在 TUI 非无询问模式下可用，用于向用户提问并获取响应。
pub struct QuestionAskTool;

#[async_trait]
impl ToolExecutor for QuestionAskTool {
    fn definition(&self) -> &ToolDefinition {
        &QUESTION_DEFINITION
    }

    async fn execute(&self, context: &ToolContext, args: Value) -> Result<ToolResult, ToolError> {
        let question = args
            .get("question")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParameters("question is required".to_string()))?;

        let timeout_secs = args
            .get("timeout")
            .and_then(serde_json::Value::as_u64)
            .map_or(30u64, |v| v.clamp(1, 300));

        let user_interaction_tx = context
            .user_interaction_tx
            .as_ref()
            .ok_or(ToolError::PermissionDenied)?;

        let (response_tx, mut response_rx) = mpsc::channel::<String>(1);

        user_interaction_tx
            .send((question.to_string(), response_tx))
            .await
            .map_err(|_| ToolError::ExecutionFailed("Failed to send question to UI".to_string()))?;

        let response = tokio::select! {
            result = response_rx.recv() => {
                match result {
                    Some(answer) => answer,
                    None => return Err(ToolError::ExecutionFailed(
                        "User interaction channel closed".to_string()
                    )),
                }
            },
            () = tokio::time::sleep(Duration::from_secs(timeout_secs)) => {
                return Err(ToolError::Timeout);
            }
        };

        Ok(ToolResult {
            output: ToolOutput::Text(response),
            is_error: false,
            prompt_component: None,
        })
    }
}
