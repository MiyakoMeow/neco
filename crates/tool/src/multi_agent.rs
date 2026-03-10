//! Multi-agent tools.

use std::sync::{Arc, LazyLock};
use std::time::Duration;

use async_trait::async_trait;
use neoco_core::ids::ToolId;
use neoco_core::{
    AgentUlid, ToolCapabilities, ToolCategory, ToolContext, ToolDefinition, ToolError,
    ToolExecutor, ToolOutput, ToolResult,
};

/// 多 Agent 引擎 trait，用于解耦工具和 `AgentEngine` 的依赖关系。
#[async_trait]
pub trait MultiAgentEngine: Send + Sync {
    /// 创建一个新的子 Agent。
    async fn spawn_child(
        &self,
        parent_ulid: AgentUlid,
        definition_id: String,
        model_group: Option<String>,
        mcp_servers: Option<Vec<String>>,
        skills: Option<Vec<String>>,
    ) -> Result<AgentUlid, String>;

    /// 向子 Agent 发送消息。
    async fn send_message(
        &self,
        from: AgentUlid,
        to: AgentUlid,
        content: String,
    ) -> Result<(), String>;

    /// 报告子 Agent 的进度。
    async fn report_progress(
        &self,
        agent_ulid: AgentUlid,
        progress: f64,
        content: String,
    ) -> Result<(), String>;
}
use serde_json::{Map, Value};

/// 创建子 Agent 参数的 JSON Schema。
fn make_spawn_schema() -> Value {
    let mut map = Map::new();
    map.insert("type".to_string(), Value::String("object".to_string()));
    map.insert(
        "$schema".to_string(),
        Value::String("https://json-schema.org/draft/2020-12/schema".to_string()),
    );
    let mut props = Map::new();

    let mut agent_id_prop = Map::new();
    agent_id_prop.insert("type".to_string(), Value::String("string".to_string()));
    agent_id_prop.insert(
        "description".to_string(),
        Value::String("要生成的Agent标识".to_string()),
    );

    let mut task_prop = Map::new();
    task_prop.insert("type".to_string(), Value::String("string".to_string()));
    task_prop.insert(
        "description".to_string(),
        Value::String("分配给下级Agent的任务描述".to_string()),
    );

    let mut model_group_prop = Map::new();
    model_group_prop.insert("type".to_string(), Value::String("string".to_string()));
    model_group_prop.insert(
        "description".to_string(),
        Value::String("覆盖使用的模型组（可选）。子Agent默认继承父Agent的model_group，可通过此参数覆盖继承的值。\n\n层级继承语义：\n- 如果不提供此参数，子Agent使用父Agent的model_group\n- 如果提供此参数，子Agent使用指定的model_group，忽略继承值\n- model_group命名规则：使用kebab-case格式，如 'gpt-4', 'claude-3-opus'\n- model_group必须在配置文件中预先定义".to_string()),
    );

    let mut mcp_servers_prop = Map::new();
    mcp_servers_prop.insert("type".to_string(), Value::String("array".to_string()));
    mcp_servers_prop.insert(
        "items".to_string(),
        Value::Object(Map::from_iter([(
            "type".to_string(),
            Value::String("string".to_string()),
        )])),
    );
    mcp_servers_prop.insert(
        "description".to_string(),
        Value::String("额外的MCP服务器列表，追加到Agent定义中的mcp_servers（可选）。\n\n指定服务器必须在配置文件中预先定义。".to_string()),
    );

    let mut skills_prop = Map::new();
    skills_prop.insert("type".to_string(), Value::String("array".to_string()));
    skills_prop.insert(
        "items".to_string(),
        Value::Object(Map::from_iter([(
            "type".to_string(),
            Value::String("string".to_string()),
        )])),
    );
    skills_prop.insert(
        "description".to_string(),
        Value::String("额外的Skills列表，追加到Agent定义中的skills（可选）。\n\n指定Skill必须在配置的skills目录中存在。".to_string()),
    );

    props.insert("agent_id".to_string(), Value::Object(agent_id_prop));
    props.insert("task".to_string(), Value::Object(task_prop));
    props.insert("model_group".to_string(), Value::Object(model_group_prop));
    props.insert("mcp_servers".to_string(), Value::Object(mcp_servers_prop));
    props.insert("skills".to_string(), Value::Object(skills_prop));

    map.insert("properties".to_string(), Value::Object(props));
    map.insert(
        "required".to_string(),
        Value::Array(vec![
            Value::String("agent_id".to_string()),
            Value::String("task".to_string()),
        ]),
    );
    Value::Object(map)
}

/// 创建消息发送参数的 JSON Schema。
fn make_send_schema() -> Value {
    let mut map = Map::new();
    map.insert("type".to_string(), Value::String("object".to_string()));
    map.insert(
        "$schema".to_string(),
        Value::String("https://json-schema.org/draft/2020-12/schema".to_string()),
    );
    let mut props = Map::new();
    let mut to_id_prop = Map::new();
    to_id_prop.insert("type".to_string(), Value::String("string".to_string()));
    to_id_prop.insert(
        "description".to_string(),
        Value::String("目标Agent ID".to_string()),
    );
    let mut content_prop = Map::new();
    content_prop.insert("type".to_string(), Value::String("string".to_string()));
    content_prop.insert(
        "description".to_string(),
        Value::String("发送的消息内容".to_string()),
    );
    props.insert("to_id".to_string(), Value::Object(to_id_prop));
    props.insert("content".to_string(), Value::Object(content_prop));
    map.insert("properties".to_string(), Value::Object(props));
    map.insert(
        "required".to_string(),
        Value::Array(vec![
            Value::String("to_id".to_string()),
            Value::String("content".to_string()),
        ]),
    );
    Value::Object(map)
}

/// 创建进度报告参数的 JSON Schema。
fn make_report_schema() -> Value {
    let mut map = Map::new();
    map.insert("type".to_string(), Value::String("object".to_string()));
    map.insert(
        "$schema".to_string(),
        Value::String("https://json-schema.org/draft/2020-12/schema".to_string()),
    );
    let mut props = Map::new();
    let mut progress_prop = Map::new();
    progress_prop.insert("type".to_string(), Value::String("number".to_string()));
    progress_prop.insert(
        "description".to_string(),
        Value::String("进度 (0.0-1.0)".to_string()),
    );
    let mut content_prop = Map::new();
    content_prop.insert("type".to_string(), Value::String("string".to_string()));
    content_prop.insert(
        "description".to_string(),
        Value::String("汇报内容".to_string()),
    );
    props.insert("progress".to_string(), Value::Object(progress_prop));
    props.insert("content".to_string(), Value::Object(content_prop));
    map.insert("properties".to_string(), Value::Object(props));
    map.insert("required".to_string(), Value::Array(vec![]));
    Value::Object(map)
}

/// 子 Agent 生成参数的 JSON Schema。
static SPAWN_SCHEMA: LazyLock<Value> = LazyLock::new(make_spawn_schema);
/// 消息发送参数的 JSON Schema。
static SEND_SCHEMA: LazyLock<Value> = LazyLock::new(make_send_schema);
/// 进度报告参数的 JSON Schema。
static REPORT_SCHEMA: LazyLock<Value> = LazyLock::new(make_report_schema);

/// 子 Agent 生成工具定义。
static SPAWN_DEFINITION: LazyLock<ToolDefinition> = LazyLock::new(|| ToolDefinition {
    id: ToolId::from_string("multi-agent::spawn").unwrap(),
    description: "创建一个新的子Agent".to_string(),
    schema: SPAWN_SCHEMA.clone(),
    capabilities: ToolCapabilities {
        streaming: false,
        requires_network: false,
        resource_level: neoco_core::ResourceLevel::Low,
        concurrent: false,
    },
    timeout: Duration::from_secs(10),
    category: ToolCategory::Common,
    prompt_component: Some("tool::multi-agent::spawn".to_string()),
});

/// 消息发送工具定义。
static SEND_DEFINITION: LazyLock<ToolDefinition> = LazyLock::new(|| ToolDefinition {
    id: ToolId::from_string("multi-agent::send").unwrap(),
    description: "向子Agent发送消息".to_string(),
    schema: SEND_SCHEMA.clone(),
    capabilities: ToolCapabilities {
        streaming: false,
        requires_network: false,
        resource_level: neoco_core::ResourceLevel::Low,
        concurrent: false,
    },
    timeout: Duration::from_secs(30),
    category: ToolCategory::Common,
    prompt_component: Some("tool::multi-agent::send".to_string()),
});

/// 进度报告工具定义。
static REPORT_DEFINITION: LazyLock<ToolDefinition> = LazyLock::new(|| ToolDefinition {
    id: ToolId::from_string("multi-agent::report").unwrap(),
    description: "获取子Agent的执行报告".to_string(),
    schema: REPORT_SCHEMA.clone(),
    capabilities: ToolCapabilities {
        streaming: false,
        requires_network: false,
        resource_level: neoco_core::ResourceLevel::Low,
        concurrent: false,
    },
    timeout: Duration::from_secs(5),
    category: ToolCategory::Common,
    prompt_component: Some("tool::multi-agent::report".to_string()),
});

/// 多 Agent 生成工具。
///
/// 此工具用于创建一个新的子 Agent，可以执行独立的任务。
pub struct MultiAgentSpawnTool {
    /// 多 Agent 引擎实例
    agent_engine: Option<Arc<dyn MultiAgentEngine>>,
}

impl MultiAgentSpawnTool {
    /// 创建新的工具实例
    pub fn new(agent_engine: Arc<dyn MultiAgentEngine>) -> Self {
        Self {
            agent_engine: Some(agent_engine),
        }
    }

    /// 创建空工具实例（用于测试）
    pub fn new_null() -> Self {
        Self { agent_engine: None }
    }
}

#[async_trait]
impl ToolExecutor for MultiAgentSpawnTool {
    fn definition(&self) -> &ToolDefinition {
        &SPAWN_DEFINITION
    }

    async fn execute(&self, _context: &ToolContext, args: Value) -> Result<ToolResult, ToolError> {
        let engine = self.agent_engine.as_ref().ok_or_else(|| {
            ToolError::ExecutionFailed("Agent engine not initialized".to_string())
        })?;

        let parent_id =
            args.get("agent_id")
                .and_then(|v| v.as_str())
                .ok_or(ToolError::InvalidParameters(
                    "agent_id is required".to_string(),
                ))?;

        let parent_ulid: AgentUlid = parent_id
            .parse()
            .map_err(|_| ToolError::InvalidParameters("Invalid agent_id format".to_string()))?;

        let task = args
            .get("task")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParameters("task is required".to_string()))?;

        let model_group = args
            .get("model_group")
            .and_then(|v| v.as_str())
            .map(String::from);

        let mcp_servers = args.get("mcp_servers").and_then(|v| {
            v.as_array().map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
        });

        let skills = args.get("skills").and_then(|v| {
            v.as_array().map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
        });

        let child_ulid = engine
            .spawn_child(
                parent_ulid,
                task.to_string(),
                model_group,
                mcp_servers,
                skills,
            )
            .await
            .map_err(ToolError::ExecutionFailed)?;

        Ok(ToolResult {
            output: ToolOutput::Text(child_ulid.to_string()),
            is_error: false,
            prompt_component: None,
        })
    }
}

/// 多 Agent 发送工具。
///
/// 此工具用于向已创建的子 Agent 发送消息，与其进行通信。
pub struct MultiAgentSendTool {
    /// 多 Agent 引擎实例
    agent_engine: Option<Arc<dyn MultiAgentEngine>>,
}

impl MultiAgentSendTool {
    /// 创建新的工具实例
    pub fn new(agent_engine: Arc<dyn MultiAgentEngine>) -> Self {
        Self {
            agent_engine: Some(agent_engine),
        }
    }

    /// 创建空工具实例（用于测试）
    pub fn new_null() -> Self {
        Self { agent_engine: None }
    }
}

#[async_trait]
impl ToolExecutor for MultiAgentSendTool {
    fn definition(&self) -> &ToolDefinition {
        &SEND_DEFINITION
    }

    async fn execute(&self, context: &ToolContext, args: Value) -> Result<ToolResult, ToolError> {
        let engine = self.agent_engine.as_ref().ok_or_else(|| {
            ToolError::ExecutionFailed("Agent engine not initialized".to_string())
        })?;

        let to_id =
            args.get("to_id")
                .and_then(|v| v.as_str())
                .ok_or(ToolError::InvalidParameters(
                    "to_id is required".to_string(),
                ))?;
        let msg_content =
            args.get("content")
                .and_then(|v| v.as_str())
                .ok_or(ToolError::InvalidParameters(
                    "content is required".to_string(),
                ))?;

        let to_ulid: AgentUlid = to_id
            .parse()
            .map_err(|_| ToolError::InvalidParameters("Invalid to_id format".to_string()))?;

        engine
            .send_message(context.agent_ulid, to_ulid, msg_content.to_string())
            .await
            .map_err(ToolError::ExecutionFailed)?;

        Ok(ToolResult {
            output: ToolOutput::Text("Message sent successfully".to_string()),
            is_error: false,
            prompt_component: None,
        })
    }
}

/// 多 Agent 报告工具。
///
/// 此工具用于获取子 Agent 的执行报告和状态信息。
pub struct MultiAgentReportTool {
    /// 多 Agent 引擎实例
    agent_engine: Option<Arc<dyn MultiAgentEngine>>,
}

impl MultiAgentReportTool {
    /// 创建新的工具实例
    pub fn new(agent_engine: Arc<dyn MultiAgentEngine>) -> Self {
        Self {
            agent_engine: Some(agent_engine),
        }
    }

    /// 创建空工具实例（用于测试）
    pub fn new_null() -> Self {
        Self { agent_engine: None }
    }
}

#[async_trait]
impl ToolExecutor for MultiAgentReportTool {
    fn definition(&self) -> &ToolDefinition {
        &REPORT_DEFINITION
    }

    async fn execute(&self, context: &ToolContext, args: Value) -> Result<ToolResult, ToolError> {
        let engine = self.agent_engine.as_ref().ok_or_else(|| {
            ToolError::ExecutionFailed("Agent engine not initialized".to_string())
        })?;

        let progress = args.get("progress").and_then(Value::as_f64).unwrap_or(0.0);
        let msg_content = args
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        engine
            .report_progress(context.agent_ulid, progress, msg_content)
            .await
            .map_err(ToolError::ExecutionFailed)?;

        Ok(ToolResult {
            output: ToolOutput::Text(format!("Progress reported: {}%", progress * 100.0)),
            is_error: false,
            prompt_component: None,
        })
    }
}
