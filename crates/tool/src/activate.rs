//! Activate tools.

use std::sync::Arc;
use std::sync::LazyLock;
use std::time::Duration;

use async_trait::async_trait;
use neoco_core::ids::SkillUlid;
use neoco_core::ids::ToolId;
use neoco_core::{
    ToolCapabilities, ToolCategory, ToolContext, ToolDefinition, ToolError, ToolExecutor,
    ToolOutput, ToolRegistry, ToolResult,
};
use neoco_mcp::McpManager;
use neoco_mcp::register_mcp_tools;
use neoco_skill::SkillService;
use serde_json::{Map, Value};

/// 创建 Skill 激活参数的 JSON Schema。
fn make_skill_schema() -> Value {
    let mut map = Map::new();
    map.insert(
        "$schema".to_string(),
        Value::String("https://json-schema.org/draft/2020-12/schema".to_string()),
    );
    map.insert("type".to_string(), Value::String("object".to_string()));
    let mut props = Map::new();
    let mut prop = Map::new();
    prop.insert("type".to_string(), Value::String("string".to_string()));
    prop.insert(
        "description".to_string(),
        Value::String("Skill ID".to_string()),
    );
    props.insert("skill_id".to_string(), Value::Object(prop));
    map.insert("properties".to_string(), Value::Object(props));
    map.insert(
        "required".to_string(),
        Value::Array(vec![Value::String("skill_id".to_string())]),
    );
    Value::Object(map)
}

/// 创建 MCP 激活参数的 JSON Schema。
fn make_mcp_schema() -> Value {
    let mut map = Map::new();
    map.insert(
        "$schema".to_string(),
        Value::String("https://json-schema.org/draft/2020-12/schema".to_string()),
    );
    map.insert("type".to_string(), Value::String("object".to_string()));
    let mut props = Map::new();
    let mut prop = Map::new();
    prop.insert("type".to_string(), Value::String("string".to_string()));
    prop.insert(
        "description".to_string(),
        Value::String("MCP服务器名称".to_string()),
    );
    props.insert("server".to_string(), Value::Object(prop));
    map.insert("properties".to_string(), Value::Object(props));
    map.insert(
        "required".to_string(),
        Value::Array(vec![Value::String("server".to_string())]),
    );
    Value::Object(map)
}

/// Skill 激活参数的 JSON Schema。
static SKILL_SCHEMA: LazyLock<Value> = LazyLock::new(make_skill_schema);
/// MCP 激活参数的 JSON Schema。
static MCP_SCHEMA: LazyLock<Value> = LazyLock::new(make_mcp_schema);

/// Skill 激活工具定义。
static SKILL_DEFINITION: LazyLock<ToolDefinition> = LazyLock::new(|| ToolDefinition {
    id: ToolId::from_string("activate::skill").unwrap(),
    description: "激活指定的Skill".to_string(),
    schema: SKILL_SCHEMA.clone(),
    capabilities: ToolCapabilities {
        streaming: false,
        requires_network: false,
        resource_level: neoco_core::ResourceLevel::Low,
        concurrent: false,
    },
    timeout: Duration::from_secs(10),
    category: ToolCategory::Common,
    prompt_component: Some("tool::activate::skill".to_string()),
});

/// MCP 激活工具定义。
static MCP_DEFINITION: LazyLock<ToolDefinition> = LazyLock::new(|| ToolDefinition {
    id: ToolId::from_string("activate::mcp").unwrap(),
    description: "激活指定的MCP服务器".to_string(),
    schema: MCP_SCHEMA.clone(),
    capabilities: ToolCapabilities {
        streaming: false,
        requires_network: false,
        resource_level: neoco_core::ResourceLevel::Low,
        concurrent: false,
    },
    timeout: Duration::from_secs(10),
    category: ToolCategory::Common,
    prompt_component: Some("tool::activate::mcp".to_string()),
});

/// Skill 激活工具。
///
/// 此工具用于激活指定的 Skill，使其功能可用于当前会话。
pub struct ActivateSkillTool {
    /// Skill 服务实例
    skill_service: Option<Arc<dyn SkillService>>,
}

impl ActivateSkillTool {
    /// Creates a new `ActivateSkillTool` with the given skill service.
    pub fn new(skill_service: Arc<dyn SkillService>) -> Self {
        Self {
            skill_service: Some(skill_service),
        }
    }

    /// Creates a null `ActivateSkillTool` without a skill service.
    pub fn new_null() -> Self {
        Self {
            skill_service: None,
        }
    }
}

#[async_trait]
impl ToolExecutor for ActivateSkillTool {
    fn definition(&self) -> &ToolDefinition {
        &SKILL_DEFINITION
    }

    async fn execute(&self, _context: &ToolContext, args: Value) -> Result<ToolResult, ToolError> {
        let skill_id = args
            .get("skill_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParameters("skill_id is required".to_string()))?;

        let skill_ulid: SkillUlid = skill_id
            .parse()
            .map_err(|_| ToolError::InvalidParameters("Invalid skill_id format".to_string()))?;

        if let Some(ref service) = self.skill_service {
            match service.activate(&skill_ulid).await {
                Ok(activated) => Ok(ToolResult {
                    output: ToolOutput::Text(format!(
                        "Skill activated successfully: {} ({})",
                        activated.name, skill_ulid
                    )),
                    is_error: false,
                    prompt_component: None,
                }),
                Err(e) => Err(ToolError::ExecutionFailed(e.to_string())),
            }
        } else {
            Ok(ToolResult {
                output: ToolOutput::Text(format!("Skill {skill_id} activated (not implemented)")),
                is_error: false,
                prompt_component: None,
            })
        }
    }
}

/// MCP 服务器激活工具。
///
/// 此工具用于激活指定的 MCP (Model Context Protocol) 服务器。
pub struct ActivateMcpTool {
    /// MCP 管理器实例
    mcp_manager: Option<Arc<McpManager>>,
    /// 工具注册表实例
    tool_registry: Option<Arc<dyn ToolRegistry>>,
}

impl ActivateMcpTool {
    /// Creates a new `ActivateMcpTool` with the given MCP manager and tool registry.
    pub fn new(mcp_manager: Arc<McpManager>, tool_registry: Arc<dyn ToolRegistry>) -> Self {
        Self {
            mcp_manager: Some(mcp_manager),
            tool_registry: Some(tool_registry),
        }
    }

    /// Creates a null `ActivateMcpTool` without an MCP manager.
    pub fn new_null() -> Self {
        Self {
            mcp_manager: None,
            tool_registry: None,
        }
    }
}

#[async_trait]
impl ToolExecutor for ActivateMcpTool {
    fn definition(&self) -> &ToolDefinition {
        &MCP_DEFINITION
    }

    async fn execute(&self, _context: &ToolContext, args: Value) -> Result<ToolResult, ToolError> {
        let server = args
            .get("server")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParameters("server is required".to_string()))?;

        if let Some(ref manager) = self.mcp_manager {
            match manager.connect(server).await {
                Ok(_connection) => {
                    let tool_count = if let Some(ref registry) = self.tool_registry {
                        match register_mcp_tools(manager, registry.as_ref(), server).await {
                            Ok(count) => count,
                            Err(e) => {
                                return Err(ToolError::ExecutionFailed(format!(
                                    "Failed to register MCP tools: {e}"
                                )));
                            },
                        }
                    } else {
                        0
                    };

                    Ok(ToolResult {
                        output: ToolOutput::Text(format!(
                            "MCP server '{server}' connected successfully, {tool_count} tools registered",
                        )),
                        is_error: false,
                        prompt_component: None,
                    })
                },
                Err(e) => Err(ToolError::ExecutionFailed(format!(
                    "Failed to connect to MCP server: {e}"
                ))),
            }
        } else {
            Ok(ToolResult {
                output: ToolOutput::Text(format!(
                    "MCP server '{server}' activated (not implemented)"
                )),
                is_error: false,
                prompt_component: None,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_activate_tools_exist() {
        let _: ActivateSkillTool = ActivateSkillTool::new_null();
        let _: ActivateMcpTool = ActivateMcpTool::new_null();
    }
}
