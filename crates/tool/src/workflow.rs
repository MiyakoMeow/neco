//! Workflow tools.

use std::sync::LazyLock;
use std::time::Duration;

use async_trait::async_trait;
use neoco_core::ids::ToolId;
use neoco_core::{
    ToolCapabilities, ToolCategory, ToolContext, ToolDefinition, ToolError, ToolExecutor,
    ToolOutput, ToolResult,
};
use serde_json::{Map, Value};

/// 创建通过参数的 JSON Schema。
fn make_pass_schema() -> Value {
    let mut map = Map::new();
    map.insert("type".to_string(), Value::String("object".to_string()));
    map.insert(
        "$schema".to_string(),
        Value::String("https://json-schema.org/draft/2020-12/schema".to_string()),
    );
    let mut props = Map::new();
    let mut msg_prop = Map::new();
    msg_prop.insert("type".to_string(), Value::String("string".to_string()));
    msg_prop.insert(
        "description".to_string(),
        Value::String("可选的通过消息".to_string()),
    );
    props.insert("message".to_string(), Value::Object(msg_prop));
    map.insert("properties".to_string(), Value::Object(props));
    Value::Object(map)
}

/// 创建选项参数的 JSON Schema。
fn make_option_schema() -> Value {
    let mut map = Map::new();
    map.insert("type".to_string(), Value::String("object".to_string()));
    map.insert(
        "$schema".to_string(),
        Value::String("https://json-schema.org/draft/2020-12/schema".to_string()),
    );
    let mut props = Map::new();
    let mut opt_prop = Map::new();
    opt_prop.insert("type".to_string(), Value::String("string".to_string()));
    opt_prop.insert(
        "description".to_string(),
        Value::String("选项标识符".to_string()),
    );
    props.insert("option".to_string(), Value::Object(opt_prop));
    map.insert("properties".to_string(), Value::Object(props));
    map.insert(
        "required".to_string(),
        Value::Array(vec![Value::String("option".to_string())]),
    );
    Value::Object(map)
}

/// 通过参数的 JSON Schema。
static PASS_SCHEMA: LazyLock<Value> = LazyLock::new(make_pass_schema);
/// 选项参数的 JSON Schema。
static OPTION_SCHEMA: LazyLock<Value> = LazyLock::new(make_option_schema);

/// 通过工具定义。
static PASS_DEFINITION: LazyLock<ToolDefinition> = LazyLock::new(|| ToolDefinition {
    id: ToolId::from_string("workflow::pass").unwrap(),
    description: "工作流通过，继续执行".to_string(),
    schema: PASS_SCHEMA.clone(),
    capabilities: ToolCapabilities {
        streaming: false,
        requires_network: false,
        resource_level: neoco_core::ResourceLevel::Low,
        concurrent: false,
    },
    timeout: Duration::from_secs(1),
    category: ToolCategory::Common,
    prompt_component: Some("tool::workflow::pass".to_string()),
});

/// 选项工具定义。
static OPTION_DEFINITION: LazyLock<ToolDefinition> = LazyLock::new(|| ToolDefinition {
    id: ToolId::from_string("workflow::option").unwrap(),
    description: "工作流选项，选择指定选项".to_string(),
    schema: OPTION_SCHEMA.clone(),
    capabilities: ToolCapabilities {
        streaming: false,
        requires_network: false,
        resource_level: neoco_core::ResourceLevel::Low,
        concurrent: false,
    },
    timeout: Duration::from_secs(1),
    category: ToolCategory::Common,
    prompt_component: Some("tool::workflow::option".to_string()),
});

/// 工作流通过工具。
///
/// 此工具用于标记工作流步骤为通过状态，允许继续执行下一步。
pub struct WorkflowPassTool;

#[async_trait]
impl ToolExecutor for WorkflowPassTool {
    fn definition(&self) -> &ToolDefinition {
        &PASS_DEFINITION
    }

    async fn execute(&self, _context: &ToolContext, args: Value) -> Result<ToolResult, ToolError> {
        let message = args
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("Pass");

        Ok(ToolResult {
            output: ToolOutput::Text(message.to_string()),
            is_error: false,
            prompt_component: None,
        })
    }
}

/// 工作流选项工具。
///
/// 此工具用于在工作流中选择指定的选项，用于分支决策场景。
pub struct WorkflowOptionTool;

#[async_trait]
impl ToolExecutor for WorkflowOptionTool {
    fn definition(&self) -> &ToolDefinition {
        &OPTION_DEFINITION
    }

    async fn execute(&self, _context: &ToolContext, args: Value) -> Result<ToolResult, ToolError> {
        let option = args
            .get("option")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParameters("option is required".to_string()))?;

        Ok(ToolResult {
            output: ToolOutput::Text(format!("Option selected: {option}")),
            is_error: false,
            prompt_component: None,
        })
    }
}
