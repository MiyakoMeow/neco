//! 服务初始化模块
//!
//! 提供统一的服务初始化和依赖注入，包括：
//! - 存储（文件存储）
//! - Skill 服务初始化
//! - MCP 管理器初始化
//! - 工具注册表完整初始化

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use neoco_agent::AgentEngine;
use neoco_agent::engine::AgentDefinitionRepository;
use neoco_config::Config;
use neoco_context::{CompressionServiceImpl, ContextManagerImpl, SimpleCounter};
use neoco_core::ids::{AgentUlid, MessageId, SessionUlid};
use neoco_core::messages::Message;
use neoco_core::prompt::{PromptBuilderImpl, PromptLoaderImpl};
use neoco_core::traits::{Agent, AgentRepository, MessageRepository};
use neoco_core::{Event, EventFilter, EventPublisher, EventSubscriber, ToolExecutor};
use neoco_mcp::McpManager;
use neoco_model::client::ModelClient;
use neoco_session::SessionManager;
use neoco_session::agent::Agent as SessionAgent;
use neoco_session::storage::{FileStorage as SessionFileStorage, StorageBackend};
use neoco_skill::{DefaultSkillService, SkillService};
use neoco_tool::DefaultToolRegistry;
use neoco_tool::multi_agent::MultiAgentEngine;
use neoco_tool::registry::ToolRegistry as ToolRegistryTrait;
use neoco_tool::registry::ToolRegistry;
use thiserror::Error;

/// 服务初始化错误
#[derive(Debug, Error)]
pub enum ServiceError {
    /// 存储错误
    #[error("存储错误: {0}")]
    Storage(String),

    /// 会话错误
    #[error("会话错误: {0}")]
    Session(String),

    /// 技能服务错误
    #[error("技能服务错误: {0}")]
    Skill(String),

    /// MCP错误
    #[error("MCP错误: {0}")]
    Mcp(String),

    /// 工具注册错误
    #[error("工具注册错误: {0}")]
    ToolRegistry(String),

    /// IO错误
    #[error("IO错误: {0}")]
    Io(#[from] std::io::Error),
}

/// 存储适配器
///
/// 桥接 `FileStorage` 与 `AgentRepository` 和 `MessageRepository` trait
pub struct StorageAdapter {
    /// 文件存储后端
    storage: Arc<SessionFileStorage>,
    /// 会话管理器
    session_manager: Arc<SessionManager<SessionFileStorage>>,
}

impl StorageAdapter {
    /// 创建新的存储适配器
    #[must_use]
    pub fn new(base_path: PathBuf) -> Self {
        let storage = Arc::new(SessionFileStorage::new(base_path));
        let session_manager = Arc::new(SessionManager::new(Arc::clone(&storage)));
        Self {
            storage,
            session_manager,
        }
    }

    /// 克隆存储适配器
    #[must_use]
    pub fn clone_storage(&self) -> Self {
        Self {
            storage: Arc::clone(&self.storage),
            session_manager: Arc::new(SessionManager::new(Arc::clone(&self.storage))),
        }
    }

    /// 获取会话管理器
    #[must_use]
    pub fn session_manager(&self) -> Arc<SessionManager<SessionFileStorage>> {
        Arc::clone(&self.session_manager)
    }

    /// 获取底层存储
    #[must_use]
    pub fn storage(&self) -> &Arc<SessionFileStorage> {
        &self.storage
    }
}

#[async_trait]
impl AgentRepository for StorageAdapter {
    async fn create(&self, agent: &Agent) -> Result<(), neoco_core::AgentError> {
        let session_agent = SessionAgent {
            id: agent.id,
            parent_ulid: agent.parent_ulid,
            definition_id: agent.definition_id.clone(),
            mode: match agent.mode {
                neoco_core::traits::AgentModeParsed::Primary => {
                    neoco_session::agent::AgentModeParsed::Primary
                },
                neoco_core::traits::AgentModeParsed::SubAgent => {
                    neoco_session::agent::AgentModeParsed::SubAgent
                },
                neoco_core::traits::AgentModeParsed::Multiple => {
                    neoco_session::agent::AgentModeParsed::Multiple(Vec::new())
                },
            },
            model_group: agent.model_group.clone(),
            system_prompt: agent.system_prompt.clone(),
            messages: Vec::new(),
            state: neoco_core::events::AgentState::Idle,
            active_tools: Vec::new(),
            active_mcp: Vec::new(),
            active_skills: Vec::new(),
            created_at: agent.created_at,
            last_activity: agent.last_activity,
        };

        self.storage
            .as_ref()
            .save_agent(&session_agent)
            .await
            .map_err(|e| neoco_core::AgentError::Invalid(e.to_string()))
    }

    async fn find_by_id(&self, id: &AgentUlid) -> Result<Option<Agent>, neoco_core::AgentError> {
        match self.storage.as_ref().load_agent(id).await {
            Ok(session_agent) => {
                let agent = Agent {
                    id: session_agent.id,
                    parent_ulid: session_agent.parent_ulid,
                    definition_id: session_agent.definition_id,
                    mode: match session_agent.mode {
                        neoco_session::agent::AgentModeParsed::Primary => {
                            neoco_core::traits::AgentModeParsed::Primary
                        },
                        neoco_session::agent::AgentModeParsed::SubAgent => {
                            neoco_core::traits::AgentModeParsed::SubAgent
                        },
                        neoco_session::agent::AgentModeParsed::Multiple(_) => {
                            neoco_core::traits::AgentModeParsed::Multiple
                        },
                    },
                    model_group: session_agent.model_group,
                    system_prompt: session_agent.system_prompt,
                    state: session_agent.state,
                    created_at: session_agent.created_at,
                    last_activity: session_agent.last_activity,
                };
                Ok(Some(agent))
            },
            Err(neoco_session::storage::StorageError::NotFound(_)) => Ok(None),
            Err(e) => Err(neoco_core::AgentError::Invalid(e.to_string())),
        }
    }

    async fn update(&self, agent: &Agent) -> Result<(), neoco_core::AgentError> {
        let session_agent = SessionAgent {
            id: agent.id,
            parent_ulid: agent.parent_ulid,
            definition_id: agent.definition_id.clone(),
            mode: match agent.mode {
                neoco_core::traits::AgentModeParsed::Primary => {
                    neoco_session::agent::AgentModeParsed::Primary
                },
                neoco_core::traits::AgentModeParsed::SubAgent => {
                    neoco_session::agent::AgentModeParsed::SubAgent
                },
                neoco_core::traits::AgentModeParsed::Multiple => {
                    neoco_session::agent::AgentModeParsed::Multiple(Vec::new())
                },
            },
            model_group: agent.model_group.clone(),
            system_prompt: agent.system_prompt.clone(),
            messages: Vec::new(),
            state: agent.state.clone(),
            active_tools: Vec::new(),
            active_mcp: Vec::new(),
            active_skills: Vec::new(),
            created_at: agent.created_at,
            last_activity: agent.last_activity,
        };

        self.storage
            .as_ref()
            .save_agent(&session_agent)
            .await
            .map_err(|e| neoco_core::AgentError::Invalid(e.to_string()))
    }

    async fn delete(&self, _id: &AgentUlid) -> Result<(), neoco_core::AgentError> {
        Ok(())
    }

    async fn find_by_session(
        &self,
        session_ulid: &SessionUlid,
    ) -> Result<Vec<Agent>, neoco_core::AgentError> {
        let agent_ids = self
            .storage
            .as_ref()
            .list_agents(session_ulid)
            .await
            .map_err(|e| neoco_core::AgentError::Invalid(e.to_string()))?;

        let mut agents = Vec::new();
        for id in agent_ids {
            if let Ok(session_agent) = self.storage.as_ref().load_agent(&id).await {
                let agent = Agent {
                    id: session_agent.id,
                    parent_ulid: session_agent.parent_ulid,
                    definition_id: session_agent.definition_id,
                    mode: match session_agent.mode {
                        neoco_session::agent::AgentModeParsed::Primary => {
                            neoco_core::traits::AgentModeParsed::Primary
                        },
                        neoco_session::agent::AgentModeParsed::SubAgent => {
                            neoco_core::traits::AgentModeParsed::SubAgent
                        },
                        neoco_session::agent::AgentModeParsed::Multiple(_) => {
                            neoco_core::traits::AgentModeParsed::Multiple
                        },
                    },
                    model_group: session_agent.model_group,
                    system_prompt: session_agent.system_prompt,
                    state: session_agent.state,
                    created_at: session_agent.created_at,
                    last_activity: session_agent.last_activity,
                };
                agents.push(agent);
            }
        }
        Ok(agents)
    }

    async fn find_children(
        &self,
        parent_ulid: &AgentUlid,
    ) -> Result<Vec<Agent>, neoco_core::AgentError> {
        let session_id = parent_ulid.as_session_ulid();
        let agents = self.find_by_session(&session_id).await?;
        Ok(agents
            .into_iter()
            .filter(|a| a.parent_ulid == Some(*parent_ulid))
            .collect())
    }
}

#[async_trait]
impl MessageRepository for StorageAdapter {
    async fn append(
        &self,
        agent_ulid: &AgentUlid,
        message: &Message,
    ) -> Result<(), neoco_core::StorageError> {
        self.storage
            .as_ref()
            .append_message(agent_ulid, message)
            .await
            .map_err(|e| neoco_core::StorageError::OperationFailed(e.to_string()))
    }

    async fn list(&self, agent_ulid: &AgentUlid) -> Result<Vec<Message>, neoco_core::StorageError> {
        self.storage
            .as_ref()
            .load_messages(agent_ulid)
            .await
            .map_err(|e| neoco_core::StorageError::OperationFailed(e.to_string()))
    }

    async fn delete_prefix(
        &self,
        _agent_ulid: &AgentUlid,
        _before_id: MessageId,
    ) -> Result<(), neoco_core::StorageError> {
        Ok(())
    }

    async fn delete_suffix(
        &self,
        _agent_ulid: &AgentUlid,
        _after_id: MessageId,
    ) -> Result<(), neoco_core::StorageError> {
        Ok(())
    }
}

/// 简单事件发布者实现
pub struct SimpleEventPublisher;

impl SimpleEventPublisher {
    /// 创建新的事件发布者
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl Default for SimpleEventPublisher {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl EventPublisher for SimpleEventPublisher {
    async fn publish(&self, _event: Event) {}

    fn subscribe(&self, _filter: EventFilter) -> Arc<dyn EventSubscriber> {
        Arc::new(DummyEventSubscriber)
    }
}

/// Dummy event subscriber that accepts all events.
struct DummyEventSubscriber;

#[async_trait]
impl EventSubscriber for DummyEventSubscriber {
    async fn on_event(&self, _event: Event) {}

    fn matches(&self, _event: &Event) -> bool {
        true
    }
}

/// 服务上下文
///
/// 包含所有初始化后的服务实例
pub struct ServiceContext {
    /// Agent 仓库
    pub agent_repo: Arc<dyn AgentRepository>,
    /// 消息仓库
    pub message_repo: Arc<dyn MessageRepository>,
    /// 事件发布者
    pub event_publisher: Arc<dyn EventPublisher>,
    /// 技能服务
    pub skill_service: Arc<dyn SkillService>,
    /// MCP 管理器
    pub mcp_manager: Option<Arc<McpManager>>,
    /// 工具注册表
    pub tool_registry: Arc<dyn ToolRegistryTrait>,
    /// Session 管理器
    pub session_manager: Arc<SessionManager<SessionFileStorage>>,
    /// 上下文管理器
    pub context_manager: Option<Arc<dyn neoco_context::ContextManager>>,
}

/// 服务初始化器
pub struct ServiceInitializer;

impl ServiceInitializer {
    /// 初始化完整的应用服务
    ///
    /// # 参数
    ///
    /// * `config` - 应用配置
    /// * `model_client` - 模型客户端
    /// * `agent_definition_repo` - Agent 定义仓库
    ///
    /// # 返回
    ///
    /// 返回包含所有服务的上下文和引擎
    ///
    /// # Errors
    ///
    /// 如果服务初始化失败，返回 `ServiceError`
    pub async fn init(
        config: &Config,
        model_client: Arc<dyn ModelClient>,
        agent_definition_repo: Arc<dyn AgentDefinitionRepository>,
    ) -> Result<(ServiceContext, Arc<AgentEngine>), ServiceError> {
        let working_dir = config.system.storage.session_dir.clone();

        let storage_adapter = StorageAdapter::new(working_dir);
        let storage_for_agent = storage_adapter.clone_storage();
        let storage_for_message = storage_adapter.clone_storage();
        let agent_repo: Arc<dyn AgentRepository> = Arc::new(storage_for_agent);
        let message_repo: Arc<dyn MessageRepository> = Arc::new(storage_for_message);

        let event_publisher: Arc<dyn EventPublisher> = Arc::new(SimpleEventPublisher::new());

        let context_config = neoco_context::ContextConfig {
            auto_compact_enabled: config.system.context.auto_compact_enabled,
            auto_compact_threshold: config.system.context.auto_compact_threshold,
            compact_model_group: neoco_context::ModelGroupRef::new(
                &config.system.context.compact_model_group,
            ),
            keep_recent_messages: config.system.context.keep_recent_messages,
            ..Default::default()
        };

        let tool_registry = Arc::new(DefaultToolRegistry::new()) as Arc<dyn ToolRegistryTrait>;

        let skill_service = Self::init_skill_service(config, tool_registry.clone())?;
        let mcp_manager = Self::init_mcp_manager(config)?;

        let prompt_loader = Arc::new(PromptLoaderImpl::new_with_default_dirs())
            as Arc<dyn neoco_core::prompt::PromptLoader>;

        let context_manager = Self::init_context_manager(
            &message_repo,
            model_client.clone(),
            &context_config,
            prompt_loader.clone(),
        );
        let prompt_builder: Arc<dyn neoco_core::prompt::PromptBuilder> =
            Arc::new(PromptBuilderImpl::from_config_dirs(vec![]));

        Self::init_tool_registry(
            tool_registry.clone(),
            message_repo.clone(),
            model_client.clone(),
            context_config,
            skill_service.clone(),
            mcp_manager.clone(),
            prompt_loader.clone(),
        )
        .await;

        let model_name = model_client.model_name().to_string();
        let engine_config = neoco_agent::engine::Config {
            default_model: Some(model_name),
            ..Default::default()
        };

        let mut engine_builder = AgentEngine::builder()
            .agent_repo(agent_repo.clone())
            .message_repo(message_repo.clone())
            .model_client(model_client)
            .tool_registry(tool_registry.clone() as Arc<dyn neoco_tool::registry::ToolRegistry>)
            .event_publisher(event_publisher.clone())
            .skill_service(skill_service.clone())
            .agent_definition_repo(agent_definition_repo)
            .config(engine_config)
            .prompt_builder(prompt_builder)
            .prompt_loader(prompt_loader)
            .session_manager(
                storage_adapter.session_manager() as Arc<dyn neoco_core::kernel::SessionManager>
            );

        if let Some(ref cm) = context_manager {
            engine_builder = engine_builder.context_manager(Arc::clone(cm));
        }

        let engine = engine_builder
            .build()
            .map_err(|e| ServiceError::ToolRegistry(e.to_string()))?;

        let engine: Arc<AgentEngine> = Arc::new(engine);

        Self::register_multi_agent_engine_tools(Arc::clone(&engine), tool_registry.clone()).await;

        let session_manager = storage_adapter.session_manager();

        Ok((
            ServiceContext {
                agent_repo,
                message_repo,
                event_publisher,
                skill_service,
                mcp_manager,
                tool_registry,
                session_manager,
                context_manager,
            },
            engine,
        ))
    }

    /// 注册多Agent引擎工具
    async fn register_multi_agent_engine_tools(
        engine: Arc<AgentEngine>,
        tool_registry: Arc<dyn ToolRegistryTrait>,
    ) {
        use neoco_tool::{MultiAgentReportTool, MultiAgentSendTool, MultiAgentSpawnTool};

        let registry = tool_registry;

        registry
            .register(Arc::new(MultiAgentSpawnTool::new(
                Arc::clone(&engine) as Arc<dyn MultiAgentEngine>
            )) as Arc<dyn ToolExecutor>)
            .await;
        registry
            .register(Arc::new(MultiAgentSendTool::new(
                Arc::clone(&engine) as Arc<dyn MultiAgentEngine>
            )) as Arc<dyn ToolExecutor>)
            .await;
        registry
            .register(Arc::new(MultiAgentReportTool::new(
                Arc::clone(&engine) as Arc<dyn MultiAgentEngine>
            )) as Arc<dyn ToolExecutor>)
            .await;
    }

    /// 初始化 Skill 服务
    fn init_skill_service(
        _config: &Config,
        tool_registry: Arc<dyn ToolRegistryTrait>,
    ) -> Result<Arc<dyn SkillService>, ServiceError> {
        let paths = neoco_config::Paths::new();

        std::fs::create_dir_all(&paths.skills_dir)?;
        std::fs::create_dir_all(&paths.config_dir)?;

        let tool_registry: Option<Arc<dyn ToolRegistry>> =
            Some(tool_registry as Arc<dyn ToolRegistry>);

        let service = DefaultSkillService::new(paths.skills_dir, paths.config_dir, tool_registry);

        Ok(Arc::new(service))
    }

    /// 初始化 MCP 管理器
    #[allow(clippy::unnecessary_wraps)]
    fn init_mcp_manager(config: &Config) -> Result<Option<Arc<McpManager>>, ServiceError> {
        if config.mcp_servers.is_empty() {
            return Ok(None);
        }

        let mcp_config: std::collections::HashMap<String, neoco_config::McpServerConfig> = config
            .mcp_servers
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect();

        let manager = McpManager::new(mcp_config);
        Ok(Some(Arc::new(manager)))
    }

    /// 初始化上下文管理器
    fn init_context_manager(
        message_repo: &Arc<dyn MessageRepository>,
        model_client: Arc<dyn ModelClient>,
        context_config: &neoco_context::ContextConfig,
        prompt_loader: Arc<dyn neoco_core::prompt::PromptLoader>,
    ) -> Option<Arc<dyn neoco_context::ContextManager>> {
        if !context_config.auto_compact_enabled {
            return None;
        }

        let token_counter = Arc::new(SimpleCounter::new());
        let message_repo_for_compression = Arc::clone(message_repo);
        let context_config_for_compression = context_config.clone();

        let compression_service = Arc::new(CompressionServiceImpl::new(
            message_repo_for_compression,
            token_counter.clone(),
            model_client,
            context_config_for_compression,
            prompt_loader,
        ));

        let context_manager = ContextManagerImpl::new(
            Arc::clone(message_repo),
            token_counter.clone(),
            compression_service,
            context_config.clone(),
        );

        Some(Arc::new(context_manager) as Arc<dyn neoco_context::ContextManager>)
    }

    /// 初始化工具注册表
    async fn init_tool_registry(
        registry: Arc<dyn ToolRegistryTrait>,
        message_repo: Arc<dyn MessageRepository>,
        model_client: Arc<dyn ModelClient>,
        context_config: neoco_context::ContextConfig,
        skill_service: Arc<dyn SkillService>,
        mcp_manager: Option<Arc<McpManager>>,
        prompt_loader: Arc<dyn neoco_core::prompt::PromptLoader>,
    ) {
        Self::register_file_tools(Arc::clone(&registry)).await;
        Self::register_context_tools(
            Arc::clone(&registry),
            message_repo,
            model_client,
            context_config,
            prompt_loader,
        )
        .await;
        Self::register_skill_tools(Arc::clone(&registry), skill_service).await;
        Self::register_mcp_tools(Arc::clone(&registry), mcp_manager).await;
        Self::register_workflow_tools(Arc::clone(&registry)).await;
        Self::register_tui_tools(Arc::clone(&registry)).await;
    }

    /// 注册文件操作工具
    async fn register_file_tools(registry: Arc<dyn ToolRegistryTrait>) {
        use neoco_tool::{
            FileDeleteTool, FileEditTool, FileListTool, FileLsTool, FileReadTool, FileRmTool,
            FileWriteTool,
        };

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
    }

    /// 注册上下文管理工具
    async fn register_context_tools(
        registry: Arc<dyn ToolRegistryTrait>,
        message_repo: Arc<dyn MessageRepository>,
        model_client: Arc<dyn ModelClient>,
        context_config: neoco_context::ContextConfig,
        prompt_loader: Arc<dyn neoco_core::prompt::PromptLoader>,
    ) {
        use neoco_tool::context::DummyContextObserver;
        use neoco_tool::{ContextCompactTool, ContextObserveTool};

        registry
            .register(
                Arc::new(ContextObserveTool::new(Arc::new(DummyContextObserver)))
                    as Arc<dyn ToolExecutor>,
            )
            .await;

        let token_counter = Arc::new(SimpleCounter::new());
        let compression_service = Arc::new(CompressionServiceImpl::new(
            message_repo,
            token_counter,
            model_client,
            context_config,
            prompt_loader,
        ));
        registry
            .register(
                Arc::new(ContextCompactTool::new(compression_service)) as Arc<dyn ToolExecutor>
            )
            .await;
    }

    /// 注册多Agent工具（null 版本，用于兼容）
    #[allow(dead_code)]
    async fn register_multi_agent_tools(registry: Arc<dyn ToolRegistryTrait>) {
        use neoco_tool::{MultiAgentReportTool, MultiAgentSendTool, MultiAgentSpawnTool};

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

    /// 注册技能工具
    async fn register_skill_tools(
        registry: Arc<dyn ToolRegistryTrait>,
        skill_service: Arc<dyn SkillService>,
    ) {
        use neoco_tool::ActivateSkillTool;

        registry
            .register(Arc::new(ActivateSkillTool::new(skill_service)) as Arc<dyn ToolExecutor>)
            .await;
    }

    /// 注册 MCP 工具
    async fn register_mcp_tools(
        registry: Arc<dyn ToolRegistryTrait>,
        mcp_manager: Option<Arc<McpManager>>,
    ) {
        use neoco_tool::ActivateMcpTool;

        if let Some(manager) = mcp_manager {
            registry
                .register(
                    Arc::new(ActivateMcpTool::new(manager, Arc::clone(&registry)))
                        as Arc<dyn ToolExecutor>,
                )
                .await;
        } else {
            registry
                .register(Arc::new(ActivateMcpTool::new_null()) as Arc<dyn ToolExecutor>)
                .await;
        }
    }

    /// 注册工作流工具
    async fn register_workflow_tools(registry: Arc<dyn ToolRegistryTrait>) {
        use neoco_tool::{WorkflowOptionTool, WorkflowPassTool};

        registry
            .register(Arc::new(WorkflowPassTool) as Arc<dyn ToolExecutor>)
            .await;
        registry
            .register(Arc::new(WorkflowOptionTool) as Arc<dyn ToolExecutor>)
            .await;
    }

    /// 注册 TUI 工具
    async fn register_tui_tools(registry: Arc<dyn ToolRegistryTrait>) {
        use neoco_tool::QuestionAskTool;

        registry
            .register(Arc::new(QuestionAskTool) as Arc<dyn ToolExecutor>)
            .await;
    }
}
