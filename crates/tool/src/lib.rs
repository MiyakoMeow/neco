//! `NeoCo` Tool - Tool executor and registry implementation.
//!
//! This crate provides tool execution capabilities for the `NeoCo` project.

#![allow(clippy::return_self_not_must_use)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::expect_used)]
#![allow(unused_crate_dependencies)]

pub mod activate;
pub mod context;
pub mod exec;
pub mod fs;
pub mod multi_agent;
pub mod registry;
pub mod security;
pub mod state;
pub mod tui;
pub mod workflow;

pub use activate::{ActivateMcpTool, ActivateSkillTool};
pub use context::{ContextCompactTool, ContextObserveTool};
pub use exec::ToolExecutorImpl;
pub use fs::{
    FileDeleteTool, FileEditTool, FileListTool, FileLsTool, FileReadTool, FileRmTool, FileWriteTool,
};
pub use multi_agent::{
    MultiAgentEngine, MultiAgentReportTool, MultiAgentSendTool, MultiAgentSpawnTool,
};
pub use registry::{DefaultToolRegistry, ToolRegistry};
pub use security::PathValidator;
pub use state::ExecutionState;
pub use tui::QuestionAskTool;
pub use workflow::{WorkflowOptionTool, WorkflowPassTool};

use neoco_config::ToolsConfig;
use neoco_context::{CompressionServiceImpl, ContextConfig, SimpleCounter};
use neoco_core::traits::MessageRepository;
use neoco_core::{PromptLoader, ToolExecutor};
use neoco_mcp::McpManager;
use neoco_model::client::ModelClient;
use neoco_skill::SkillService;
use std::sync::Arc;

/// 配置创建工具注册表所需的参数。
pub struct RegistryConfig {
    /// 消息存储库，用于上下文压缩工具
    pub message_repo: Arc<dyn MessageRepository>,
    /// 模型客户端，用于上下文压缩工具
    pub model_client: Arc<dyn ModelClient>,
    /// 上下文配置，用于上下文压缩工具
    pub context_config: ContextConfig,
    /// 提示加载器
    pub prompt_loader: Arc<dyn PromptLoader>,
    /// 多 agent 引擎
    pub agent_engine: Option<Arc<dyn MultiAgentEngine>>,
    /// Skill 服务
    pub skill_service: Option<Arc<dyn SkillService>>,
    /// MCP 管理器
    pub mcp_manager: Option<Arc<McpManager>>,
    /// 工具配置
    pub tools_config: Option<ToolsConfig>,
}

/// 创建并配置默认的工具注册表。
pub async fn create_registry(config: RegistryConfig) -> Arc<dyn ToolRegistry> {
    let registry: Arc<dyn ToolRegistry> = Arc::new(DefaultToolRegistry::new());

    if let Some(config) = config.tools_config {
        for (tool_prefix, duration) in config.timeouts {
            if let Ok(d) = duration.to_std() {
                registry.set_timeout(&tool_prefix, d).await;
            }
        }
    }

    registry
        .register(Arc::new(FileReadTool) as Arc<dyn ToolExecutor>)
        .await;
    registry
        .register(Arc::new(FileWriteTool) as Arc<dyn ToolExecutor>)
        .await;
    registry
        .register(Arc::new(FileEditTool) as Arc<dyn ToolExecutor>)
        .await;
    registry
        .register(Arc::new(FileDeleteTool) as Arc<dyn ToolExecutor>)
        .await;
    registry
        .register(Arc::new(FileRmTool::new()) as Arc<dyn ToolExecutor>)
        .await;
    registry
        .register(Arc::new(FileListTool) as Arc<dyn ToolExecutor>)
        .await;
    registry
        .register(Arc::new(FileLsTool::new()) as Arc<dyn ToolExecutor>)
        .await;

    registry
        .register(Arc::new(ContextObserveTool::new(Arc::new(
            context::DummyContextObserver,
        ))) as Arc<dyn ToolExecutor>)
        .await;

    let token_counter = Arc::new(SimpleCounter::new());
    let compression_service = Arc::new(CompressionServiceImpl::new(
        config.message_repo,
        token_counter,
        config.model_client,
        config.context_config,
        config.prompt_loader,
    ));
    registry
        .register(Arc::new(ContextCompactTool::new(compression_service)) as Arc<dyn ToolExecutor>)
        .await;

    if let Some(ref engine) = config.agent_engine {
        registry
            .register(
                Arc::new(MultiAgentSpawnTool::new(Arc::clone(engine))) as Arc<dyn ToolExecutor>
            )
            .await;
        registry
            .register(Arc::new(MultiAgentSendTool::new(Arc::clone(engine))) as Arc<dyn ToolExecutor>)
            .await;
        registry
            .register(
                Arc::new(MultiAgentReportTool::new(Arc::clone(engine))) as Arc<dyn ToolExecutor>
            )
            .await;
    } else {
        registry
            .register(Arc::new(MultiAgentSpawnTool::new_null()) as Arc<dyn ToolExecutor>)
            .await;
        registry
            .register(Arc::new(MultiAgentSendTool::new_null()) as Arc<dyn ToolExecutor>)
            .await;
        registry
            .register(Arc::new(MultiAgentReportTool::new_null()) as Arc<dyn ToolExecutor>)
            .await;
    }

    if let Some(ref service) = config.skill_service {
        registry
            .register(Arc::new(ActivateSkillTool::new(Arc::clone(service))) as Arc<dyn ToolExecutor>)
            .await;
    } else {
        registry
            .register(Arc::new(ActivateSkillTool::new_null()) as Arc<dyn ToolExecutor>)
            .await;
    }

    if let Some(ref manager) = config.mcp_manager {
        registry
            .register(Arc::new(ActivateMcpTool::new(
                Arc::clone(manager),
                Arc::clone(&registry),
            )) as Arc<dyn ToolExecutor>)
            .await;
    } else {
        registry
            .register(Arc::new(ActivateMcpTool::new_null()) as Arc<dyn ToolExecutor>)
            .await;
    }

    registry
        .register(Arc::new(WorkflowPassTool) as Arc<dyn ToolExecutor>)
        .await;
    registry
        .register(Arc::new(WorkflowOptionTool) as Arc<dyn ToolExecutor>)
        .await;

    registry
        .register(Arc::new(QuestionAskTool) as Arc<dyn ToolExecutor>)
        .await;

    registry as Arc<dyn ToolRegistry>
}

/// 创建一个用于测试的默认工具注册表。
///
/// 此函数使用虚拟的消息存储库和模型客户端创建工具注册表，
/// 主要用于测试和演示目的。
///
/// # 返回
///
/// 返回一个已注册所有默认工具的工具注册表。
pub async fn create_default_registry(
    agent_engine: Option<Arc<dyn MultiAgentEngine>>,
    prompt_loader: Arc<dyn PromptLoader>,
) -> Arc<dyn ToolRegistry> {
    struct DummyMessageRepo;

    #[async_trait::async_trait]
    impl MessageRepository for DummyMessageRepo {
        async fn append(
            &self,
            _agent_ulid: &neoco_core::AgentUlid,
            _message: &neoco_core::messages::Message,
        ) -> Result<(), neoco_core::errors::StorageError> {
            Ok(())
        }

        async fn list(
            &self,
            _agent_ulid: &neoco_core::AgentUlid,
        ) -> Result<Vec<neoco_core::messages::Message>, neoco_core::errors::StorageError> {
            Ok(vec![])
        }

        async fn delete_prefix(
            &self,
            _agent_ulid: &neoco_core::AgentUlid,
            _before_id: neoco_core::ids::MessageId,
        ) -> Result<(), neoco_core::errors::StorageError> {
            Ok(())
        }

        async fn delete_suffix(
            &self,
            _agent_ulid: &neoco_core::AgentUlid,
            _after_id: neoco_core::ids::MessageId,
        ) -> Result<(), neoco_core::errors::StorageError> {
            Ok(())
        }
    }

    struct DummyModelClient;

    #[async_trait::async_trait]
    impl ModelClient for DummyModelClient {
        async fn chat_completion(
            &self,
            _request: neoco_model::types::ChatRequest<'_>,
        ) -> Result<neoco_model::types::ChatResponse, neoco_model::error::ModelError> {
            Ok(neoco_model::types::ChatResponse {
                id: "dummy".to_string(),
                model: "dummy".to_string(),
                choices: vec![neoco_model::types::Choice {
                    index: 0,
                    message: neoco_model::types::Message {
                        role: "assistant".to_string(),
                        content: "dummy response".to_string(),
                        tool_calls: None,
                        tool_call_id: None,
                    },
                    finish_reason: Some("stop".to_string()),
                }],
                usage: neoco_model::types::Usage {
                    prompt_tokens: 0,
                    completion_tokens: 0,
                    total_tokens: 0,
                },
            })
        }

        async fn chat_completion_stream(
            &self,
            _request: neoco_model::types::ChatRequest<'_>,
        ) -> Result<
            neoco_model::client::BoxStream<
                Result<neoco_model::types::ChatChunk, neoco_model::error::ModelError>,
            >,
            neoco_model::error::ModelError,
        > {
            Err(neoco_model::error::ModelError::api(
                "dummy",
                "Not implemented",
            ))
        }

        fn capabilities(&self) -> neoco_model::types::ModelCapabilities {
            neoco_model::types::ModelCapabilities {
                streaming: true,
                tools: false,
                functions: false,
                json_mode: false,
                vision: false,
                context_window: 128_000,
            }
        }

        fn provider_name(&self) -> &'static str {
            "dummy"
        }

        fn model_name(&self) -> &'static str {
            "dummy"
        }
    }

    create_registry(RegistryConfig {
        message_repo: Arc::new(DummyMessageRepo),
        model_client: Arc::new(DummyModelClient),
        context_config: ContextConfig::default(),
        prompt_loader,
        agent_engine,
        skill_service: None,
        mcp_manager: None,
        tools_config: None,
    })
    .await
}
