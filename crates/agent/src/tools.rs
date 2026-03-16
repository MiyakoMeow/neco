//! Agent 工具实现
//!
//! 提供用于多 Agent 协作的工具实现。

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use neoco_core::ids::AgentUlid;
use neoco_core::ids::ToolId;
use neoco_core::tool::{
    ToolCapabilities, ToolCategory, ToolContext, ToolDefinition, ToolExecutor, ToolOutput,
    ToolResult,
};
use serde_json::Value;

use crate::engine::AgentEngine;

/// 生成子 Agent 工具
///
/// 允许 Agent 创建子 Agent 来执行特定任务。
pub struct SpawnAgentTool {
    /// Agent 引擎引用
    agent_engine: Arc<AgentEngine>,
}

impl SpawnAgentTool {
    /// 创建新的工具实例
    #[must_use]
    pub fn new(agent_engine: Arc<AgentEngine>) -> Self {
        Self { agent_engine }
    }
}

#[async_trait]
impl ToolExecutor for SpawnAgentTool {
    /// 获取工具定义
    fn definition(&self) -> &ToolDefinition {
        static DEF: std::sync::LazyLock<ToolDefinition> = std::sync::LazyLock::new(|| {
            ToolDefinition {
                id: ToolId::from_string("multi_agent::spawn").unwrap(),
                description: "生成一个下级Agent来执行特定任务".to_string(),
                schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "agent_id": {
                            "type": "string",
                            "description": "要生成的Agent标识"
                        },
                        "task": {
                            "type": "string",
                            "description": "分配给下级Agent的任务描述"
                        },
                        "model_group": {
                            "type": "string",
                            "description": "覆盖使用的模型组（可选）。\n子Agent默认继承父Agent的model_group，\n可通过此参数覆盖继承的值。\n\n层级继承语义：\n- 如果不提供此参数，子Agent使用父Agent的model_group\n- 如果提供此参数，子Agent使用指定的model_group，忽略继承值\n- model_group命名规则：使用kebab-case格式，如 'gpt-4', 'claude-3-opus'\n- model_group必须在配置文件中预先定义"
                        },
                        "mcp_servers": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "额外的MCP服务器列表，追加到Agent定义中的mcp_servers（可选）。\n\n指定服务器必须在配置文件中预先定义。"
                        },
                        "skills": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "额外的Skills列表，追加到Agent定义中的skills（可选）。\n\n指定Skill必须在配置的skills目录中存在。"
                        }
                    },
                    "required": ["agent_id", "task"]
                }),
                capabilities: ToolCapabilities::default(),
                timeout: Duration::from_secs(30),
                category: ToolCategory::Common,
                prompt_component: None,
            }
        });
        &DEF
    }

    async fn execute(
        &self,
        context: &ToolContext,
        args: Value,
    ) -> Result<ToolResult, neoco_core::ToolError> {
        let agent_id = args
            .get("agent_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                neoco_core::ToolError::InvalidParameters("agent_id is required".to_string())
            })?;

        let task = args.get("task").and_then(|v| v.as_str()).ok_or_else(|| {
            neoco_core::ToolError::InvalidParameters("task is required".to_string())
        })?;

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

        let result = self
            .agent_engine
            .spawn_child(
                context.agent_ulid,
                agent_id.to_string(),
                model_group,
                mcp_servers,
                skills,
            )
            .await
            .map_err(|e| neoco_core::ToolError::ExecutionFailed(e.to_string()))?;

        Ok(ToolResult {
            output: ToolOutput::Json(serde_json::json!({
                "agent_id": result.to_string(),
                "task": task,
                "status": "spawned"
            })),
            is_error: false,
            prompt_component: None,
        })
    }
}

/// 发送消息工具
///
/// 允许 Agent 向其他 Agent 发送消息。
pub struct SendMessageTool {
    /// Agent 引擎引用
    agent_engine: Arc<AgentEngine>,
}

impl SendMessageTool {
    /// 创建新的工具实例
    #[must_use]
    pub fn new(agent_engine: Arc<AgentEngine>) -> Self {
        Self { agent_engine }
    }
}

#[async_trait]
impl ToolExecutor for SendMessageTool {
    /// 获取工具定义
    fn definition(&self) -> &ToolDefinition {
        static DEF: std::sync::LazyLock<ToolDefinition> =
            std::sync::LazyLock::new(|| ToolDefinition {
                id: ToolId::from_string("multi_agent::send").unwrap(),
                description: "向指定Agent发送消息".to_string(),
                schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "target_agent": {
                            "type": "string",
                            "description": "目标Agent的ID"
                        },
                        "message": {
                            "type": "string",
                            "description": "消息内容"
                        },
                        "message_type": {
                            "type": "string",
                            "enum": ["task", "query", "response", "general"],
                            "description": "消息类型"
                        }
                    },
                    "required": ["target_agent", "message"]
                }),
                capabilities: ToolCapabilities::default(),
                timeout: Duration::from_secs(30),
                category: ToolCategory::Common,
                prompt_component: None,
            });
        &DEF
    }

    async fn execute(
        &self,
        context: &ToolContext,
        args: Value,
    ) -> Result<ToolResult, neoco_core::ToolError> {
        let target_agent_str = args
            .get("target_agent")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                neoco_core::ToolError::InvalidParameters("target_agent is required".to_string())
            })?;

        let target_agent = AgentUlid::from_string(target_agent_str).map_err(|e| {
            neoco_core::ToolError::InvalidParameters(format!("invalid agent id: {e}"))
        })?;

        let message = args
            .get("message")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                neoco_core::ToolError::InvalidParameters("message is required".to_string())
            })?;

        let _message_type = args
            .get("message_type")
            .and_then(|v| v.as_str())
            .unwrap_or("general");

        self.agent_engine
            .send_message(context.agent_ulid, target_agent, message.to_string())
            .await
            .map_err(|e| neoco_core::ToolError::ExecutionFailed(e.to_string()))?;

        Ok(ToolResult {
            output: ToolOutput::Json(serde_json::json!({
                "status": "sent",
                "target": target_agent_str
            })),
            is_error: false,
            prompt_component: None,
        })
    }
}

/// 汇报工具
///
/// 允许 Agent 向父 Agent 汇报进度和结果。
pub struct ReportTool {
    /// Agent 引擎引用
    agent_engine: Arc<AgentEngine>,
}

impl ReportTool {
    /// 创建新的工具实例
    #[must_use]
    pub fn new(agent_engine: Arc<AgentEngine>) -> Self {
        Self { agent_engine }
    }
}

#[async_trait]
impl ToolExecutor for ReportTool {
    /// 获取工具定义
    fn definition(&self) -> &ToolDefinition {
        static DEF: std::sync::LazyLock<ToolDefinition> =
            std::sync::LazyLock::new(|| ToolDefinition {
                id: ToolId::from_string("multi_agent::report").unwrap(),
                description: "向上级Agent汇报任务进度或结果".to_string(),
                schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "report_type": {
                            "type": "string",
                            "enum": ["progress", "result", "question"],
                            "description": "汇报类型"
                        },
                        "content": {
                            "type": "string",
                            "description": "汇报内容"
                        },
                        "progress": {
                            "type": "number",
                            "description": "进度百分比（0-100）"
                        }
                    },
                    "required": ["report_type", "content"]
                }),
                capabilities: ToolCapabilities::default(),
                timeout: Duration::from_secs(30),
                category: ToolCategory::Common,
                prompt_component: None,
            });
        &DEF
    }

    async fn execute(
        &self,
        context: &ToolContext,
        args: Value,
    ) -> Result<ToolResult, neoco_core::ToolError> {
        let report_type = args
            .get("report_type")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                neoco_core::ToolError::InvalidParameters("report_type is required".to_string())
            })?;

        let report_content = args
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                neoco_core::ToolError::InvalidParameters("content is required".to_string())
            })?;

        let progress = args
            .get("progress")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(0.0)
            / 100.0;

        match report_type {
            "progress" => {
                self.agent_engine
                    .report_progress(context.agent_ulid, progress, report_content.to_string())
                    .await
                    .map_err(|e| neoco_core::ToolError::ExecutionFailed(e.to_string()))?;
            },
            "result" => {
                self.agent_engine
                    .report_progress(context.agent_ulid, 1.0, report_content.to_string())
                    .await
                    .map_err(|e| neoco_core::ToolError::ExecutionFailed(e.to_string()))?;
            },
            "question" => {
                self.agent_engine
                    .report_progress(context.agent_ulid, progress, report_content.to_string())
                    .await
                    .map_err(|e| neoco_core::ToolError::ExecutionFailed(e.to_string()))?;
            },
            _ => {
                return Err(neoco_core::ToolError::InvalidParameters(
                    "report_type must be one of: progress, result, question".to_string(),
                ));
            },
        }

        Ok(ToolResult {
            output: ToolOutput::Json(serde_json::json!({
                "status": "reported",
                "report_type": report_type
            })),
            is_error: false,
            prompt_component: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use neoco_core::EventPublisher;
    use neoco_core::ids::AgentUlid;
    use neoco_core::messages::Message;
    use neoco_core::traits::{AgentRepository, MessageRepository};
    use neoco_model::client::ModelClient;
    use neoco_tool::registry::ToolRegistry;
    use std::sync::Arc;

    #[allow(clippy::too_many_lines)]
    fn create_mock_engine() -> AgentEngine {
        struct DummyRepo;
        #[async_trait]
        impl AgentRepository for DummyRepo {
            async fn create(
                &self,
                _: &neoco_core::traits::Agent,
            ) -> Result<(), neoco_core::AgentError> {
                Ok(())
            }
            async fn find_by_id(
                &self,
                _: &AgentUlid,
            ) -> Result<Option<neoco_core::traits::Agent>, neoco_core::AgentError> {
                Ok(None)
            }
            async fn update(
                &self,
                _: &neoco_core::traits::Agent,
            ) -> Result<(), neoco_core::AgentError> {
                Ok(())
            }
            async fn delete(&self, _: &AgentUlid) -> Result<(), neoco_core::AgentError> {
                Ok(())
            }
            async fn find_by_session(
                &self,
                _: &neoco_core::ids::SessionUlid,
            ) -> Result<Vec<neoco_core::traits::Agent>, neoco_core::AgentError> {
                Ok(vec![])
            }
            async fn find_children(
                &self,
                _: &AgentUlid,
            ) -> Result<Vec<neoco_core::traits::Agent>, neoco_core::AgentError> {
                Ok(vec![])
            }
        }
        struct DummyMsgRepo;
        #[async_trait]
        impl MessageRepository for DummyMsgRepo {
            async fn append(
                &self,
                _: &AgentUlid,
                _: &Message,
            ) -> Result<(), neoco_core::StorageError> {
                Ok(())
            }
            async fn list(&self, _: &AgentUlid) -> Result<Vec<Message>, neoco_core::StorageError> {
                Ok(vec![])
            }
            async fn delete_prefix(
                &self,
                _: &AgentUlid,
                _: neoco_core::ids::MessageId,
            ) -> Result<(), neoco_core::StorageError> {
                Ok(())
            }
            async fn delete_suffix(
                &self,
                _: &AgentUlid,
                _: neoco_core::ids::MessageId,
            ) -> Result<(), neoco_core::StorageError> {
                Ok(())
            }
        }
        struct DummyModel;
        #[async_trait]
        impl ModelClient for DummyModel {
            async fn chat_completion(
                &self,
                _: neoco_model::types::ChatRequest<'_>,
            ) -> Result<neoco_model::types::ChatResponse, neoco_model::error::ModelError>
            {
                Ok(neoco_model::types::ChatResponse {
                    id: "test".to_string(),
                    model: "dummy".to_string(),
                    choices: vec![],
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
                Err(neoco_model::error::ModelError::Stream(
                    "streaming not supported in test".to_string(),
                ))
            }
            fn capabilities(&self) -> neoco_model::types::ModelCapabilities {
                neoco_model::types::ModelCapabilities::default()
            }
            fn provider_name(&self) -> &'static str {
                "dummy"
            }
            fn model_name(&self) -> &'static str {
                "dummy"
            }
        }
        struct DummyToolReg;
        #[async_trait]
        impl ToolRegistry for DummyToolReg {
            async fn register(&self, _: std::sync::Arc<dyn neoco_core::ToolExecutor>) {}
            async fn get(
                &self,
                _: &neoco_core::ids::ToolId,
            ) -> Option<std::sync::Arc<dyn neoco_core::ToolExecutor>> {
                None
            }
            async fn definitions(&self) -> Vec<neoco_core::ToolDefinition> {
                vec![]
            }
            async fn timeout(&self, _: &neoco_core::ids::ToolId) -> Option<std::time::Duration> {
                None
            }
            async fn set_timeout(&self, _: &str, _: std::time::Duration) {}
            async fn unregister(&self, _: &neoco_core::ids::ToolId) {}
            async fn list_tools(&self) -> Vec<neoco_core::ids::ToolId> {
                vec![]
            }
        }
        struct DummyPub;
        #[async_trait]
        impl EventPublisher for DummyPub {
            async fn publish(&self, _: neoco_core::events::Event) {}
            fn subscribe(
                &self,
                _: neoco_core::events::EventFilter,
            ) -> Arc<dyn neoco_core::events::EventSubscriber> {
                Arc::new(DummySub)
            }
        }
        struct DummySub;
        #[async_trait]
        impl neoco_core::events::EventSubscriber for DummySub {
            async fn on_event(&self, _: neoco_core::events::Event) {}
            fn matches(&self, _: &neoco_core::events::Event) -> bool {
                true
            }
        }
        struct DummySkillService;
        #[async_trait]
        impl neoco_skill::SkillService for DummySkillService {
            async fn discover_skills(
                &self,
            ) -> Result<Vec<neoco_skill::SkillDefinition>, neoco_skill::SkillServiceError>
            {
                Ok(vec![])
            }
            async fn activate(
                &self,
                _: &neoco_core::ids::SkillUlid,
            ) -> Result<neoco_skill::ActivatedSkill, neoco_skill::SkillServiceError> {
                Err(neoco_skill::SkillServiceError::NotFound(
                    "dummy".to_string(),
                ))
            }
            async fn deactivate(
                &self,
                _: &neoco_core::ids::SkillUlid,
            ) -> Result<(), neoco_skill::SkillServiceError> {
                Ok(())
            }
            async fn get_critical_reminder(
                &self,
                _: &neoco_core::ids::SkillUlid,
            ) -> Result<Option<String>, neoco_skill::SkillServiceError> {
                Ok(None)
            }
        }
        struct DummyAgentDefRepo;
        #[async_trait]
        impl crate::engine::AgentDefinitionRepository for DummyAgentDefRepo {
            async fn load_definition(
                &self,
                _: &str,
            ) -> Result<Option<crate::engine::AgentDefinition>, crate::AgentError> {
                Ok(None)
            }
        }
        struct DummySessionManager;
        #[async_trait]
        impl neoco_core::kernel::SessionManager for DummySessionManager {
            async fn create_session(
                &self,
                _: neoco_core::kernel::SessionConfig,
            ) -> Result<
                (neoco_core::ids::SessionUlid, neoco_core::ids::AgentUlid),
                neoco_core::kernel::KernelError,
            > {
                let session_ulid = neoco_core::ids::SessionUlid::new();
                let agent_ulid = neoco_core::ids::AgentUlid::new_root(&session_ulid);
                Ok((session_ulid, agent_ulid))
            }
            async fn load_session(
                &self,
                session_id: neoco_core::ids::SessionUlid,
            ) -> Result<neoco_core::traits::Session, neoco_core::kernel::KernelError> {
                Err(neoco_core::kernel::KernelError::SessionNotFound(session_id))
            }
            async fn delete_session(
                &self,
                _: neoco_core::ids::SessionUlid,
            ) -> Result<(), neoco_core::kernel::KernelError> {
                Ok(())
            }
            async fn get_root_agent(
                &self,
                session_id: neoco_core::ids::SessionUlid,
            ) -> Result<neoco_core::ids::AgentUlid, neoco_core::kernel::KernelError> {
                Ok(neoco_core::ids::AgentUlid::new_root(&session_id))
            }
        }
        AgentEngine::builder()
            .agent_repo(Arc::new(DummyRepo))
            .message_repo(Arc::new(DummyMsgRepo))
            .model_client(Arc::new(DummyModel))
            .tool_registry(Arc::new(DummyToolReg))
            .event_publisher(Arc::new(DummyPub))
            .skill_service(Arc::new(DummySkillService))
            .agent_definition_repo(Arc::new(DummyAgentDefRepo))
            .session_manager(Arc::new(DummySessionManager))
            .build()
            .unwrap()
    }

    #[test]
    fn test_spawn_tool_definition() {
        let tool = SpawnAgentTool::new(Arc::new(create_mock_engine()));
        let def = tool.definition();
        assert_eq!(def.id.to_string(), "multi_agent::spawn");
        assert!(def.description.contains("下级Agent"));
    }

    #[test]
    fn test_send_tool_definition() {
        let tool = SendMessageTool::new(Arc::new(create_mock_engine()));
        let def = tool.definition();
        assert_eq!(def.id.to_string(), "multi_agent::send");
        assert!(def.description.contains("发送消息"));
    }

    #[test]
    fn test_report_tool_definition() {
        let tool = ReportTool::new(Arc::new(create_mock_engine()));
        let def = tool.definition();
        assert_eq!(def.id.to_string(), "multi_agent::report");
        assert!(def.description.contains("汇报"));
    }

    #[test]
    fn test_spawn_tool_schema() {
        let tool = SpawnAgentTool::new(Arc::new(create_mock_engine()));
        let def = tool.definition();
        let schema = &def.schema;
        assert!(schema.get("properties").is_some());
        let props = schema.get("properties").unwrap();
        assert!(props.get("agent_id").is_some());
        assert!(props.get("task").is_some());
    }

    #[test]
    fn test_send_tool_schema() {
        let tool = SendMessageTool::new(Arc::new(create_mock_engine()));
        let def = tool.definition();
        let schema = &def.schema;
        let props = schema.get("properties").unwrap();
        assert!(props.get("target_agent").is_some());
        assert!(props.get("message").is_some());
    }

    #[test]
    fn test_report_tool_schema() {
        let tool = ReportTool::new(Arc::new(create_mock_engine()));
        let def = tool.definition();
        let schema = &def.schema;
        let props = schema.get("properties").unwrap();
        assert!(props.get("report_type").is_some());
        let report_type = props.get("report_type").unwrap();
        assert!(report_type.get("enum").is_some());
    }
}
