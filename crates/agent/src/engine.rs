//! Agent 引擎实现
//!
//! 提供 Agent 执行的核心引擎，包括 Agent 运行、子 Agent 生成、技能加载等功能。

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use futures::future::join_all;
use neoco_core::EventPublisher;
use neoco_core::events::EventSubscriber;
use neoco_core::ids::AgentUlid;
use neoco_core::kernel::SessionManager;
use neoco_core::messages::Message;
use neoco_core::traits::{AgentRepository, MessageRepository};
use neoco_core::{ToolContext, ToolOutput};
use neoco_model::client::ModelClient;
use neoco_model::types::{ChatRequest, ModelMessage, ToolCall as ModelToolCall};
use neoco_skill::SkillService;
use neoco_tool::MultiAgentEngine;
use neoco_tool::registry::ToolRegistry;
use tokio::sync::Mutex;
use tokio::time::timeout;

use crate::error::AgentError;
use crate::events::{AgentEvent, EventTrigger};
use crate::inter_agent::InterAgentMessage;
use neoco_context::ContextManager;
use neoco_core::events::{EventFilter, EventTypeFilter};
use neoco_core::prompt::PromptBuilder;
use neoco_core::prompt::PromptLoader;

/// Agent 定义
#[derive(Debug, Clone, Default)]
pub struct AgentDefinition {
    /// Agent ID
    pub id: Option<String>,
    /// Agent 描述
    pub description: Option<String>,
    /// Agent 运行模式
    pub mode: Option<String>,
    /// 模型名称
    pub model: Option<String>,
    /// 模型组
    pub model_group: Option<String>,
    /// 自定义系统提示词
    pub system_prompt: Option<String>,
    /// 提示词模板
    pub prompts: Vec<String>,
    /// MCP 服务器列表
    pub mcp_servers: Vec<String>,
    /// 技能列表
    pub skills: Vec<String>,
}

/// Agent 定义仓储 trait
#[async_trait]
pub trait AgentDefinitionRepository: Send + Sync {
    /// 加载 Agent 定义
    async fn load_definition(
        &self,
        definition_id: &str,
    ) -> Result<Option<AgentDefinition>, AgentError>;
}

/// 工具执行结果
///
/// 包含工具输出和提示词组件内容。
pub struct ToolExecuteResult {
    /// 工具输出文本
    pub output: String,
    /// 提示词组件内容
    pub prompt_component: Option<String>,
}

/// Agent 执行结果
///
/// 包含 Agent 运行后的输出、消息历史和工具调用记录。
pub struct AgentResult {
    /// Agent 生成的输出文本
    pub output: String,
    /// 本次运行涉及的消息列表
    pub messages: Vec<Message>,
    /// Agent 调用的工具列表
    pub tool_calls: Vec<neoco_core::messages::ToolCall>,
}

/// 子 Agent 信息
///
/// 表示当前 Agent 的一个子 Agent 的基本信息。
#[derive(Debug, Clone)]
pub struct ChildAgentInfo {
    /// 子 Agent ID
    pub id: String,
    /// 子 Agent 定义 ID
    pub definition_id: String,
    /// 子 Agent 运行模式
    pub mode: String,
}

/// Agent 执行上下文
///
/// 在 Agent 运行过程中传递的上下文信息，包含当前 Agent 及其子 Agent 的基本信息。
#[derive(Debug, Clone)]
pub struct AgentContext {
    /// 当前 Agent ID
    pub agent_id: String,
    /// 父 Agent ID
    pub parent_id: Option<String>,
    /// 子 Agent 列表
    pub children: Vec<ChildAgentInfo>,
}

/// 技能加载状态
///
/// 表示技能在 Agent 执行过程中的加载级别。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SkillLoadState {
    /// 摘要模式：只提供技能的简要描述 (~50 tokens)
    Summary,
    /// 完整指南模式：加载完整的技能指令 (~2K tokens)
    FullGuide,
    /// 关键提醒模式：错误重试时加载关键提醒 (~100 tokens)
    CriticalReminder,
}

/// 技能使用追踪器
///
/// 跟踪技能的使用状态和错误情况，用于动态调整加载策略。
pub struct SkillUsageTracker {
    /// 当前技能加载状态
    state: SkillLoadState,
    /// 连续错误次数
    consecutive_errors: usize,
    /// 上次使用的工具名称
    last_tool_name: Option<String>,
}

impl Default for SkillUsageTracker {
    fn default() -> Self {
        Self {
            state: SkillLoadState::Summary,
            consecutive_errors: 0,
            last_tool_name: None,
        }
    }
}

/// 技能使用上下文
///
/// 提供技能使用时的环境信息，用于决策加载策略。
pub struct UsageContext {
    /// 工具名称
    pub tool_name: String,
    /// 当前回合数
    pub turn: usize,
    /// 最近错误次数
    pub recent_errors: usize,
}

/// 技能内容
///
/// 表示不同加载级别的技能内容。
#[derive(Debug, Clone)]
pub enum SkillContent {
    /// 摘要内容 (~50 tokens)
    Summary(String),
    /// 完整指南内容 (~2K tokens)
    FullGuide(String),
    /// 关键提醒内容 (~100 tokens) - 用于错误重试时
    CriticalReminder(String),
}

/// Agent 引擎
///
/// 负责管理和执行 Agent，包括运行 Agent、生成子 Agent、加载技能等核心功能。
#[allow(dead_code)]
pub struct AgentEngine {
    /// Agent 仓库
    agent_repo: Arc<dyn AgentRepository>,
    /// 消息仓库
    message_repo: Arc<dyn MessageRepository>,
    /// 模型客户端
    model_client: Arc<dyn ModelClient>,
    /// 工具注册表
    tool_registry: Arc<dyn ToolRegistry>,
    /// 事件发布器
    event_publisher: Arc<dyn EventPublisher>,
    /// 技能服务
    skill_service: Arc<dyn SkillService>,
    /// Agent 定义仓储
    agent_definition_repo: Arc<dyn AgentDefinitionRepository>,
    /// 技能使用状态追踪
    skill_usage_states: Mutex<HashMap<String, SkillUsageTracker>>,
    /// 引擎配置
    config: Config,
    /// 上下文管理器（用于自动压缩）
    context_manager: Option<Arc<dyn ContextManager>>,
    /// 触发器列表
    triggers: Vec<EventTrigger>,
    /// 触发器执行器
    trigger_executor: Option<Arc<crate::events::TriggerExecutor>>,
    /// 提示词构建器
    prompt_builder: Option<Arc<dyn PromptBuilder>>,
    /// 提示词加载器
    prompt_loader: Option<Arc<dyn PromptLoader>>,
    /// 会话管理器
    session_manager: Arc<dyn SessionManager>,
}

/// Agent 引擎配置
///
/// 定义 Agent 引擎的运行参数和行为。
#[derive(Debug, Clone)]
pub struct Config {
    /// 最大子 Agent 数量
    pub max_children: usize,
    /// 默认模型名称
    pub default_model: Option<String>,
    /// 最大回合数
    pub max_turns: usize,
    /// 默认超时时间（秒）
    pub default_timeout_secs: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            max_children: 10,
            default_model: None,
            max_turns: 50,
            default_timeout_secs: 300,
        }
    }
}

/// Agent 引擎构建器
///
/// 用于构建和配置 `AgentEngine` 实例。
pub struct AgentEngineBuilder {
    /// Agent 仓库
    agent_repo: Option<Arc<dyn AgentRepository>>,
    /// 消息仓库
    message_repo: Option<Arc<dyn MessageRepository>>,
    /// 模型客户端
    model_client: Option<Arc<dyn ModelClient>>,
    /// 工具注册表
    tool_registry: Option<Arc<dyn ToolRegistry>>,
    /// 事件发布器
    event_publisher: Option<Arc<dyn EventPublisher>>,
    /// 技能服务
    skill_service: Option<Arc<dyn SkillService>>,
    /// Agent 定义仓储
    agent_definition_repo: Option<Arc<dyn AgentDefinitionRepository>>,
    /// 引擎配置
    config: Option<Config>,
    /// 上下文管理器
    context_manager: Option<Arc<dyn ContextManager>>,
    /// 初始触发器列表
    triggers: Vec<EventTrigger>,
    /// 触发器执行器
    trigger_executor: Option<Arc<crate::events::TriggerExecutor>>,
    /// 提示词构建器
    prompt_builder: Option<Arc<dyn PromptBuilder>>,
    /// 提示词加载器
    prompt_loader: Option<Arc<dyn PromptLoader>>,
    /// 会话管理器
    session_manager: Option<Arc<dyn SessionManager>>,
}

impl AgentEngineBuilder {
    /// 创建新的构建器
    #[must_use]
    pub fn new() -> Self {
        Self {
            agent_repo: None,
            message_repo: None,
            model_client: None,
            tool_registry: None,
            event_publisher: None,
            skill_service: None,
            agent_definition_repo: None,
            config: None,
            context_manager: None,
            triggers: Vec::new(),
            trigger_executor: None,
            prompt_builder: None,
            prompt_loader: None,
            session_manager: None,
        }
    }

    /// 设置 Agent 仓库
    #[must_use]
    pub fn agent_repo(mut self, repo: Arc<dyn AgentRepository>) -> Self {
        self.agent_repo = Some(repo);
        self
    }

    /// 设置消息仓库
    #[must_use]
    pub fn message_repo(mut self, repo: Arc<dyn MessageRepository>) -> Self {
        self.message_repo = Some(repo);
        self
    }

    /// 设置模型客户端
    #[must_use]
    pub fn model_client(mut self, client: Arc<dyn ModelClient>) -> Self {
        self.model_client = Some(client);
        self
    }

    /// 设置工具注册表
    #[must_use]
    pub fn tool_registry(mut self, registry: Arc<dyn ToolRegistry>) -> Self {
        self.tool_registry = Some(registry);
        self
    }

    /// 设置事件发布器
    #[must_use]
    pub fn event_publisher(mut self, publisher: Arc<dyn EventPublisher>) -> Self {
        self.event_publisher = Some(publisher);
        self
    }

    /// 设置技能服务
    #[must_use]
    pub fn skill_service(mut self, service: Arc<dyn SkillService>) -> Self {
        self.skill_service = Some(service);
        self
    }

    /// 设置 Agent 定义仓储
    #[must_use]
    pub fn agent_definition_repo(mut self, repo: Arc<dyn AgentDefinitionRepository>) -> Self {
        self.agent_definition_repo = Some(repo);
        self
    }

    /// 设置引擎配置
    #[must_use]
    pub fn config(mut self, config: Config) -> Self {
        self.config = Some(config);
        self
    }

    /// 设置上下文管理器
    #[must_use]
    pub fn context_manager(mut self, manager: Arc<dyn ContextManager>) -> Self {
        self.context_manager = Some(manager);
        self
    }

    /// 添加触发器
    #[must_use]
    pub fn add_trigger(mut self, trigger: EventTrigger) -> Self {
        self.triggers.push(trigger);
        self
    }

    /// 设置触发器执行器
    #[must_use]
    pub fn trigger_executor(mut self, executor: Arc<crate::events::TriggerExecutor>) -> Self {
        self.trigger_executor = Some(executor);
        self
    }

    /// 设置提示词构建器
    #[must_use]
    pub fn prompt_builder(mut self, builder: Arc<dyn PromptBuilder>) -> Self {
        self.prompt_builder = Some(builder);
        self
    }

    /// 设置提示词加载器
    #[must_use]
    pub fn prompt_loader(mut self, loader: Arc<dyn PromptLoader>) -> Self {
        self.prompt_loader = Some(loader);
        self
    }

    /// 设置会话管理器
    #[must_use]
    pub fn session_manager(mut self, manager: Arc<dyn SessionManager>) -> Self {
        self.session_manager = Some(manager);
        self
    }

    /// 构建 `AgentEngine` 实例
    ///
    /// # Errors
    ///
    /// 如果缺少必需的依赖项，返回错误。
    pub fn build(self) -> Result<AgentEngine, AgentError> {
        Ok(AgentEngine {
            agent_repo: self
                .agent_repo
                .ok_or_else(|| AgentError::Config("agent_repo is required".to_string()))?,
            message_repo: self
                .message_repo
                .ok_or_else(|| AgentError::Config("message_repo is required".to_string()))?,
            model_client: self
                .model_client
                .ok_or_else(|| AgentError::Config("model_client is required".to_string()))?,
            tool_registry: self
                .tool_registry
                .ok_or_else(|| AgentError::Config("tool_registry is required".to_string()))?,
            event_publisher: self
                .event_publisher
                .ok_or_else(|| AgentError::Config("event_publisher is required".to_string()))?,
            skill_service: self
                .skill_service
                .ok_or_else(|| AgentError::Config("skill_service is required".to_string()))?,
            agent_definition_repo: self.agent_definition_repo.ok_or_else(|| {
                AgentError::Config("agent_definition_repo is required".to_string())
            })?,
            skill_usage_states: Mutex::new(HashMap::new()),
            config: self.config.unwrap_or_default(),
            context_manager: self.context_manager,
            triggers: self.triggers,
            trigger_executor: self.trigger_executor,
            prompt_builder: self.prompt_builder,
            prompt_loader: self.prompt_loader,
            session_manager: self
                .session_manager
                .ok_or_else(|| AgentError::Config("session_manager is required".to_string()))?,
        })
    }
}

impl Default for AgentEngineBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentEngine {
    /// 创建新的构建器
    #[must_use]
    pub fn builder() -> AgentEngineBuilder {
        AgentEngineBuilder::new()
    }

    /// 运行 Agent
    ///
    /// 根据 Agent ID 运行指定的 Agent，处理输入并返回结果。
    /// 实现完整的模型-工具调用循环：
    /// 1. 调用模型获取响应
    /// 2. 如果有工具调用，执行工具
    /// 3. 将工具结果添加到上下文
    /// 4. 继续循环直到无工具调用
    ///
    /// # Arguments
    ///
    /// * `agent_ulid` - Agent 的唯一标识符
    /// * `input` - 输入文本
    ///
    /// # Returns
    ///
    /// 返回 Agent 执行结果，包含输出、消息和工具调用。
    ///
    /// # Errors
    ///
    /// 如果 Agent 不存在或执行过程中发生错误，返回错误。
    #[allow(clippy::too_many_lines, clippy::missing_panics_doc)]
    pub async fn run_agent(
        &self,
        agent_ulid: AgentUlid,
        input: String,
    ) -> Result<AgentResult, AgentError> {
        self.init_trigger_subscriptions();

        let agent = self
            .agent_repo
            .find_by_id(&agent_ulid)
            .await
            .map_err(|e| AgentError::NotFound(e.to_string()))?
            .ok_or_else(|| AgentError::NotFound(agent_ulid.to_string()))?;

        let children = self
            .agent_repo
            .find_children(&agent_ulid)
            .await
            .map_err(|e| AgentError::Tool(e.to_string()))?;

        let context = Self::build_context(&agent, &children);

        self.publish_event(crate::events::Event::Agent(AgentEvent::Created {
            id: agent_ulid,
            parent_ulid: agent.parent_ulid,
        }))
        .await;

        let user_message = Message::user(input.clone());

        let mut system_messages = Vec::new();
        self.load_base_prompts(&agent, &context, &mut system_messages)
            .await;

        for system_msg in &system_messages {
            if let Err(e) = self.message_repo.append(&agent_ulid, system_msg).await {
                tracing::warn!(agent_id = %agent_ulid, error = %e, "Failed to append system prompt");
            }
        }

        self.message_repo
            .append(&agent_ulid, &user_message)
            .await
            .map_err(|e| AgentError::Tool(e.to_string()))?;

        self.publish_event(crate::events::Event::Agent(AgentEvent::MessageAdded {
            id: agent_ulid,
            message_id: user_message.id,
        }))
        .await;

        let mut all_messages = system_messages;
        all_messages.push(user_message);
        let mut all_tool_calls = Vec::new();
        let max_turns = self.config.max_turns;
        let mut turn = 0;

        let model_name = self
            .config
            .default_model
            .clone()
            .unwrap_or_else(|| "default".to_string());

        loop {
            if turn >= max_turns {
                tracing::warn!(agent_id = %agent_ulid, turn = %turn, "Max turns reached");
                break;
            }

            let messages = if let Some(ref cm) = self.context_manager {
                cm.build_context(&agent_ulid, 128_000)
                    .await
                    .map_err(|e| AgentError::Tool(e.to_string()))?
            } else {
                self.message_repo
                    .list(&agent_ulid)
                    .await
                    .map_err(|e| AgentError::Tool(e.to_string()))?
                    .into_iter()
                    .map(|msg| neoco_core::messages::OwnedModelMessage {
                        role: msg.role,
                        content: msg.content,
                        tool_calls: msg.tool_calls,
                        tool_call_id: msg.tool_call_id,
                    })
                    .collect()
            };

            if let Some(ref cm) = self.context_manager
                && cm.should_compact(&agent_ulid).await
                && let Err(e) = cm.compact(&agent_ulid, None).await
            {
                tracing::warn!(agent_id = %agent_ulid, error = %e, "Context compaction failed");
            }

            let model_messages: Vec<ModelMessage<'_>> = messages
                .iter()
                .map(|msg| {
                    let model_tool_calls: Option<Vec<ModelToolCall>> =
                        msg.tool_calls.as_ref().map(|tc_list| {
                            tc_list
                                .iter()
                                .map(|tc| ModelToolCall {
                                    id: tc.id.clone(),
                                    name: tc.tool_name.to_string(),
                                    arguments: serde_json::Value::from(tc.arguments.as_str()),
                                })
                                .collect()
                        });
                    ModelMessage {
                        role: msg.role,
                        content: std::borrow::Cow::Borrowed(&msg.content),
                        tool_calls: model_tool_calls.map(std::borrow::Cow::Owned),
                        tool_call_id: msg
                            .tool_call_id
                            .as_ref()
                            .map(|id| std::borrow::Cow::Borrowed(id.as_str())),
                    }
                })
                .collect();

            let request = ChatRequest::new(model_name.clone(), model_messages);

            let response = self.model_client.chat_completion(request).await?;

            let assistant_message = response.choices.first().map_or_else(
                || Message::assistant(""),
                |c| {
                    let core_tool_calls: Option<Vec<neoco_core::messages::ToolCall>> =
                        c.message.tool_calls.as_ref().map(|tc_list| {
                            tc_list
                                .iter()
                                .filter_map(|tc| {
                                    Some(neoco_core::messages::ToolCall {
                                        id: tc.id.clone(),
                                        tool_name: tc.name.parse().ok()?,
                                        arguments: tc.arguments.to_string(),
                                    })
                                })
                                .collect()
                        });
                    Message {
                        id: neoco_core::ids::MessageId::new(),
                        role: neoco_core::messages::Role::Assistant,
                        content: c.message.content.clone(),
                        tool_calls: core_tool_calls,
                        tool_call_id: c.message.tool_call_id.clone(),
                        timestamp: chrono::Utc::now(),
                        metadata: None,
                    }
                },
            );

            let tool_calls = assistant_message.tool_calls.clone().unwrap_or_default();

            if tool_calls.is_empty() {
                self.message_repo
                    .append(&agent_ulid, &assistant_message)
                    .await
                    .map_err(|e| AgentError::Tool(e.to_string()))?;

                all_messages.push(assistant_message);
                break;
            }

            self.message_repo
                .append(&agent_ulid, &assistant_message)
                .await
                .map_err(|e| AgentError::Tool(e.to_string()))?;

            all_messages.push(assistant_message.clone());
            all_tool_calls.extend(tool_calls.clone());

            if !tool_calls.is_empty() {
                for tool_call in &tool_calls {
                    self.publish_event(crate::events::Event::Agent(AgentEvent::ToolCalled {
                        id: agent_ulid,
                        tool_id: tool_call.tool_name.clone(),
                    }))
                    .await;
                }

                let tool_calls_ref = &tool_calls;
                let turn_clone = turn;

                let results: Vec<Result<(Message, Option<SkillContent>), AgentError>> =
                    join_all(tool_calls_ref.iter().map(|tool_call| {
                        let tool_call = tool_call.clone();
                        let turn = turn_clone;
                        async move {
                            let tool_result = self
                                .execute_tool(&tool_call)
                                .await
                                .map_err(|e| AgentError::Tool(e.to_string()))?;

                            let result_content = tool_result.output;

                            let result_message = Message {
                                id: neoco_core::ids::MessageId::new(),
                                role: neoco_core::messages::Role::Tool,
                                content: result_content,
                                tool_calls: None,
                                tool_call_id: Some(tool_call.id.clone()),
                                timestamp: chrono::Utc::now(),
                                metadata: None,
                            };

                            let skill_content = self
                                .load_skill_for_tool(&tool_call.tool_name.to_string(), turn)
                                .await
                                .ok()
                                .flatten();

                            Ok((result_message, skill_content))
                        }
                    }))
                    .await;

                for (idx, result) in results.into_iter().enumerate() {
                    let (result_message, skill_content) =
                        result.map_err(|e| AgentError::Tool(e.to_string()))?;
                    let tool_id = tool_calls.get(idx).map(|c| c.tool_name.clone()).unwrap();

                    self.message_repo
                        .append(&agent_ulid, &result_message)
                        .await
                        .map_err(|e| AgentError::Tool(e.to_string()))?;

                    all_messages.push(result_message);

                    if let Some(skill_content) = skill_content {
                        let (skill_text, header) = match skill_content {
                            SkillContent::FullGuide(g) => (g, "## Skill Guide\n\n"),
                            SkillContent::Summary(_) => (String::new(), "## Skill Guide\n\n"),
                            SkillContent::CriticalReminder(r) => (r, "## Critical Reminder\n\n"),
                        };
                        if !skill_text.is_empty()
                            && let Some(last_msg) = all_messages.last_mut()
                        {
                            last_msg.content.push_str("\n\n---\n\n");
                            last_msg.content.push_str(header);
                            last_msg.content.push_str(&skill_text);
                        }
                    }

                    self.execute_triggers(crate::events::Event::Agent(AgentEvent::ToolResult {
                        id: agent_ulid,
                        tool_id,
                        success: true,
                    }))
                    .await;
                }
            }

            turn += 1;
        }

        let output = all_messages
            .last()
            .map(|m| m.content.clone())
            .unwrap_or_default();

        self.publish_event(crate::events::Event::Agent(AgentEvent::Completed {
            id: agent_ulid,
            output: output.clone(),
        }))
        .await;

        Ok(AgentResult {
            output,
            messages: all_messages,
            tool_calls: all_tool_calls,
        })
    }

    /// 执行工具调用
    async fn execute_tool(
        &self,
        tool_call: &neoco_core::messages::ToolCall,
    ) -> Result<ToolExecuteResult, AgentError> {
        let tool_id = tool_call.tool_name.clone();

        let args: serde_json::Value = serde_json::from_str(&tool_call.arguments)
            .map_err(|e| AgentError::Tool(format!("Invalid tool arguments: {e}")))?;

        let tool_executor = self
            .tool_registry
            .get(&tool_id)
            .await
            .ok_or_else(|| AgentError::Tool(format!("Tool not found: {tool_id}")))?;

        let session_ulid = neoco_core::ids::SessionUlid::new();
        let context = ToolContext {
            session_ulid,
            agent_ulid: AgentUlid::new_root(&session_ulid),
            working_dir: std::path::PathBuf::new(),
            user_interaction_tx: None,
        };

        let timeout_duration = self.tool_registry.timeout(&tool_id).await;

        let result = match timeout_duration {
            Some(dur) => timeout(dur, tool_executor.execute(&context, args))
                .await
                .map_err(|_| AgentError::Tool("Tool execution timed out".to_string()))?,
            None => tool_executor.execute(&context, args).await,
        }
        .map_err(|e| AgentError::Tool(e.to_string()))?;

        let output = match result.output {
            ToolOutput::Text(s) => s,
            ToolOutput::Json(j) => j.to_string(),
            ToolOutput::Binary(b) => format!("[binary {} bytes]", b.len()),
            ToolOutput::Empty => String::new(),
        };

        Ok(ToolExecuteResult {
            output,
            prompt_component: result.prompt_component,
        })
    }

    /// 生成子 Agent
    ///
    /// 为指定的父 Agent 创建一个子 Agent。
    ///
    /// # Arguments
    ///
    /// * `parent_ulid` - 父 Agent 的唯一标识符
    /// * `definition_id` - Agent definition ID for loading configuration
    /// * `model_group` - 可选的模型组覆盖
    /// * `mcp_servers` - 可选的额外MCP服务器列表
    /// * `skills` - 可选的额外Skills列表
    ///
    /// # Errors
    ///
    /// 如果父 Agent 不存在或已达到最大子 Agent 数量，返回错误。
    pub async fn spawn_child(
        &self,
        parent_ulid: AgentUlid,
        definition_id: String,
        model_group: Option<String>,
        _mcp_servers: Option<Vec<String>>,
        _skills: Option<Vec<String>>,
    ) -> Result<AgentUlid, AgentError> {
        let _parent = self
            .agent_repo
            .find_by_id(&parent_ulid)
            .await
            .map_err(|_e| AgentError::ParentNotFound)?
            .ok_or(AgentError::ParentNotFound)?;

        let children = self
            .agent_repo
            .find_children(&parent_ulid)
            .await
            .map_err(|e| AgentError::Tool(e.to_string()))?;

        if children.len() >= self.config.max_children {
            return Err(AgentError::MaxChildrenReached);
        }

        let agent_definition = self
            .agent_definition_repo
            .load_definition(&definition_id)
            .await
            .map_err(|e| AgentError::DefinitionNotFound(e.to_string()))?;

        if agent_definition.is_none() {
            return Err(AgentError::DefinitionNotFound(definition_id));
        }

        let child_ulid = AgentUlid::new_child(&parent_ulid);

        let child_agent = neoco_core::traits::Agent::new(
            child_ulid,
            Some(parent_ulid),
            None,
            neoco_core::traits::AgentModeParsed::SubAgent,
            model_group,
            None,
        );

        self.agent_repo
            .create(&child_agent)
            .await
            .map_err(|e| AgentError::Tool(e.to_string()))?;

        self.publish_event(crate::events::Event::Agent(AgentEvent::Created {
            id: child_ulid,
            parent_ulid: Some(parent_ulid),
        }))
        .await;

        Ok(child_ulid)
    }

    /// 获取 Agent 信息
    ///
    /// 根据 Agent ID 获取 Agent 的详细信息。
    ///
    /// # Arguments
    ///
    /// * `agent_ulid` - Agent 的唯一标识符
    ///
    /// # Returns
    ///
    /// 返回 Agent 信息。
    ///
    /// # Errors
    ///
    /// 如果 Agent 不存在，返回错误。
    pub async fn get_agent(
        &self,
        agent_ulid: AgentUlid,
    ) -> Result<neoco_core::traits::Agent, AgentError> {
        self.agent_repo
            .find_by_id(&agent_ulid)
            .await
            .map_err(|e| AgentError::NotFound(e.to_string()))?
            .ok_or_else(|| AgentError::NotFound(agent_ulid.to_string()))
    }

    /// 获取子 Agent 列表
    ///
    /// 获取指定父 Agent 的所有子 Agent。
    ///
    /// # Arguments
    ///
    /// * `parent_ulid` - 父 Agent 的唯一标识符
    ///
    /// # Returns
    ///
    /// 返回子 Agent 列表。
    ///
    /// # Errors
    ///
    /// 如果查询失败，返回错误。
    pub async fn get_children(
        &self,
        parent_ulid: &AgentUlid,
    ) -> Result<Vec<neoco_core::traits::Agent>, AgentError> {
        self.agent_repo
            .find_children(parent_ulid)
            .await
            .map_err(|e| AgentError::Tool(e.to_string()))
    }

    /// 构建 Agent 上下文信息
    ///
    /// 从 Agent 及其子 Agent 信息构建上下文，供提示词使用。
    fn build_context(
        agent: &neoco_core::traits::Agent,
        children: &[neoco_core::traits::Agent],
    ) -> AgentContext {
        let mut children_info = Vec::new();
        for child in children {
            children_info.push(ChildAgentInfo {
                id: child.id.to_string(),
                definition_id: child.definition_id.clone().unwrap_or_default(),
                mode: format!("{:?}", child.mode),
            });
        }

        AgentContext {
            agent_id: agent.id.to_string(),
            parent_id: agent.parent_ulid.map(|p| p.to_string()),
            children: children_info,
        }
    }

    /// 发送 Agent 间消息
    ///
    /// 从一个 Agent 向另一个 Agent 发送消息，并将消息持久化到两个 Agent 的消息历史中。
    ///
    /// # Arguments
    ///
    /// * `from` - 发送方 Agent ID
    /// * `to` - 接收方 Agent ID
    /// * `content` - 消息内容
    ///
    /// # Returns
    ///
    /// 返回创建的消息对象。
    ///
    /// # Errors
    ///
    /// 如果 Agent 不存在或通信权限不足，返回错误。
    pub async fn send_message(
        &self,
        from: AgentUlid,
        to: AgentUlid,
        content: String,
    ) -> Result<InterAgentMessage, AgentError> {
        let from_agent = self
            .agent_repo
            .find_by_id(&from)
            .await
            .map_err(|e| AgentError::NotFound(e.to_string()))?
            .ok_or_else(|| AgentError::NotFound(from.to_string()))?;

        let to_agent = self
            .agent_repo
            .find_by_id(&to)
            .await
            .map_err(|e| AgentError::NotFound(e.to_string()))?
            .ok_or_else(|| AgentError::NotFound(to.to_string()))?;

        if from_agent.parent_ulid != Some(to) && to_agent.parent_ulid != Some(from) {
            return Err(AgentError::PermissionDenied);
        }

        let message = InterAgentMessage::new(
            from,
            to,
            crate::inter_agent::MessageType::General,
            content.clone(),
        );

        let from_message = Message {
            id: neoco_core::ids::MessageId::new(),
            role: neoco_core::messages::Role::Assistant,
            content: content.clone(),
            tool_calls: None,
            tool_call_id: None,
            timestamp: chrono::Utc::now(),
            metadata: None,
        };

        self.message_repo
            .append(&from, &from_message)
            .await
            .map_err(|e| AgentError::Tool(e.to_string()))?;

        let to_message = Message {
            id: neoco_core::ids::MessageId::new(),
            role: neoco_core::messages::Role::User,
            content,
            tool_calls: None,
            tool_call_id: None,
            timestamp: chrono::Utc::now(),
            metadata: None,
        };

        self.message_repo
            .append(&to, &to_message)
            .await
            .map_err(|e| AgentError::Tool(e.to_string()))?;

        self.publish_event(crate::events::Event::Agent(AgentEvent::MessageAdded {
            id: from,
            message_id: from_message.id,
        }))
        .await;

        self.publish_event(crate::events::Event::Agent(AgentEvent::MessageAdded {
            id: to,
            message_id: to_message.id,
        }))
        .await;

        Ok(message)
    }

    /// 汇报进度
    ///
    /// Agent 向其父 Agent 汇报任务进度。
    ///
    /// # Arguments
    ///
    /// * `agent_ulid` -汇报 Agent 的 ID
    /// * `progress` - 进度值（0.0-1.0）
    /// * `content` - 汇报内容
    ///
    /// # Returns
    ///
    /// 返回创建的进度汇报消息。
    ///
    /// # Errors
    ///
    /// 如果 Agent 不存在或没有父 Agent，返回错误。
    pub async fn report_progress(
        &self,
        agent_ulid: AgentUlid,
        progress: f64,
        content: String,
    ) -> Result<InterAgentMessage, AgentError> {
        let agent = self
            .agent_repo
            .find_by_id(&agent_ulid)
            .await
            .map_err(|e| AgentError::NotFound(e.to_string()))?
            .ok_or_else(|| AgentError::NotFound(agent_ulid.to_string()))?;

        let parent_ulid = agent.parent_ulid.ok_or(AgentError::NoParentAgent)?;

        let message = InterAgentMessage::new(
            agent_ulid,
            parent_ulid,
            crate::inter_agent::MessageType::ProgressReport {
                task_id: agent_ulid.to_string(),
                progress,
                status: if progress >= 1.0 {
                    crate::inter_agent::TaskStatus::Completed
                } else {
                    crate::inter_agent::TaskStatus::InProgress
                },
            },
            content,
        );

        Ok(message)
    }

    /// 发布事件
    ///
    /// 将 Agent 事件发布到事件系统。
    async fn publish_event(&self, event: crate::events::Event) {
        let core_event: neoco_core::events::Event = match event {
            crate::events::Event::Agent(ae) => neoco_core::events::Event::Agent(match ae {
                AgentEvent::Created { id, parent_ulid } => {
                    neoco_core::events::AgentEvent::Created { id, parent_ulid }
                },
                AgentEvent::StateChanged { id, old, new } => {
                    neoco_core::events::AgentEvent::StateChanged { id, old, new }
                },
                AgentEvent::MessageAdded { id, message_id } => {
                    neoco_core::events::AgentEvent::MessageAdded { id, message_id }
                },
                AgentEvent::ToolCalled { id, tool_id } => {
                    neoco_core::events::AgentEvent::ToolCalled { id, tool_id }
                },
                AgentEvent::ToolResult {
                    id,
                    tool_id,
                    success,
                } => neoco_core::events::AgentEvent::ToolResult {
                    id,
                    tool_id,
                    success,
                },
                AgentEvent::Completed { id, output } => {
                    neoco_core::events::AgentEvent::Completed { id, output }
                },
                AgentEvent::Error { id, error } => {
                    neoco_core::events::AgentEvent::Error { id, error }
                },
            }),
            _ => return,
        };
        self.event_publisher.publish(core_event).await;
    }

    /// 加载基础提示词
    ///
    /// 根据 Agent 加载基础提示词组件，如 base、multi-agent 等。
    /// 这些提示词会被添加到消息列表的最前面。
    ///
    /// # Arguments
    ///
    /// * `agent` - Agent 信息
    /// * `context` - Agent 上下文，包含子Agent信息
    /// * `messages` - 消息列表，用于添加系统提示词
    ///
    /// # Errors
    ///
    /// 如果加载提示词失败，返回错误。
    async fn load_base_prompts(
        &self,
        agent: &neoco_core::traits::Agent,
        context: &AgentContext,
        messages: &mut Vec<Message>,
    ) {
        if let Some(ref builder) = self.prompt_builder {
            let components = if let Some(definition_id) = &agent.definition_id {
                if let Ok(Some(definition)) = self
                    .agent_definition_repo
                    .load_definition(definition_id)
                    .await
                {
                    if definition.prompts.is_empty() {
                        let mut default_components = vec!["base".to_string()];
                        if agent.parent_ulid.is_some() {
                            default_components.push("multi-agent-child".to_string());
                        } else {
                            default_components.push("multi-agent".to_string());
                        }
                        default_components
                    } else {
                        definition.prompts.clone()
                    }
                } else {
                    let mut default_components = vec!["base".to_string()];
                    if agent.parent_ulid.is_some() {
                        default_components.push("multi-agent-child".to_string());
                    } else {
                        default_components.push("multi-agent".to_string());
                    }
                    default_components
                }
            } else {
                let mut default_components = vec!["base".to_string()];
                if agent.parent_ulid.is_some() {
                    default_components.push("multi-agent-child".to_string());
                } else {
                    default_components.push("multi-agent".to_string());
                }
                default_components
            };

            if let Ok(prompt_content) = builder.build(&components)
                && !prompt_content.is_empty()
            {
                let system_message = Message::system(prompt_content);
                messages.push(system_message);
            }

            if !context.children.is_empty() {
                let children_description = Self::build_children_description(context);
                if !children_description.is_empty() {
                    let children_message = Message::system(children_description);
                    messages.push(children_message);
                }
            }
        }

        // 加载工具提示词
        if let Ok(tool_prompts) = self.load_tool_prompts().await
            && !tool_prompts.is_empty()
        {
            messages.push(Message::system(tool_prompts));
        }
    }

    /// 加载所有工具的提示词组件
    ///
    /// 从工具注册表中获取所有已注册的工具，
    /// 加载每个工具绑定的提示词组件，并返回格式化的提示词内容。
    /// 这些提示词会添加到 system prompt 中，让模型始终知道工具的使用方法。
    ///
    /// # Returns
    ///
    /// 返回所有工具提示词的格式化内容
    async fn load_tool_prompts(&self) -> Result<String, AgentError> {
        let mut all_tool_prompts = Vec::new();

        let tool_definitions = self.tool_registry.definitions().await;

        if let Some(ref loader) = self.prompt_loader {
            for tool_def in tool_definitions {
                if tool_def.prompt_component.is_some() {
                    match loader.load_for_tool(&tool_def.id.to_string()) {
                        Ok(Some(prompt_content)) => {
                            all_tool_prompts.push(format!(
                                "## {} ({})\n\n{}",
                                tool_def.description, tool_def.id, prompt_content
                            ));
                        },
                        Ok(None) => {
                            // 提示词组件不存在，跳过
                        },
                        Err(e) => {
                            tracing::debug!(
                                tool_id = %tool_def.id.to_string(),
                                error = %e,
                                "Failed to load tool prompt"
                            );
                        },
                    }
                }
            }
        }

        if all_tool_prompts.is_empty() {
            return Ok(String::new());
        }

        Ok(format!(
            "# 工具使用指南\n\n{}\n",
            all_tool_prompts.join("\n\n---\n\n")
        ))
    }

    /// 构建子 Agent 描述文本
    ///
    /// 将子 Agent 列表转换为可读的描述字符串。
    fn build_children_description(context: &AgentContext) -> String {
        if context.children.is_empty() {
            return String::new();
        }

        let mut description = String::from("## SubAgents\n\n");
        for child in &context.children {
            use std::fmt::Write;
            let _ = writeln!(
                description,
                "- ID: {}, Definition: {}, Mode: {}",
                child.id, child.definition_id, child.mode
            );
        }
        description
    }

    /// 获取引擎配置
    #[must_use]
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// 获取 Agent 仓库
    #[must_use]
    pub fn agent_repo(&self) -> Arc<dyn AgentRepository> {
        self.agent_repo.clone()
    }

    /// 获取工具注册表
    #[must_use]
    pub fn tool_registry(&self) -> Arc<dyn neoco_tool::registry::ToolRegistry> {
        self.tool_registry.clone()
    }

    /// 判断是否应该重新加载完整指南
    ///
    /// 根据错误次数和连续错误情况，判断是否需要重新加载技能的完整指南。
    ///
    /// # Arguments
    ///
    /// * `skill_name` - 技能名称
    /// * `error_count` - 错误次数
    ///
    /// # Returns
    ///
    /// 如果应该重新加载完整指南，返回 `true`。
    pub async fn should_reload_full_guide(&self, skill_name: &str, error_count: usize) -> bool {
        let mut states = self.skill_usage_states.lock().await;
        let tracker = states.entry(skill_name.to_string()).or_default();

        if error_count >= 2 && tracker.consecutive_errors >= 2 {
            tracker.consecutive_errors = 0;
            tracker.state = SkillLoadState::FullGuide;
            return true;
        }

        false
    }

    /// 判断是否应该加载关键提醒
    ///
    /// 当工具执行出错后重试时，加载关键提醒以帮助模型快速纠正错误。
    /// Layer 3 触发条件：
    /// - 连续错误次数 >= 2
    /// - 当前状态不是 `CriticalReminder`
    ///
    /// # Arguments
    ///
    /// * `skill_name` - 技能名称
    ///
    /// # Returns
    ///
    /// 如果应该加载关键提醒，返回 `true`。
    pub async fn should_load_critical_reminder(&self, skill_name: &str) -> bool {
        let mut states = self.skill_usage_states.lock().await;
        let tracker = states.entry(skill_name.to_string()).or_default();

        if tracker.consecutive_errors >= 2 && tracker.state != SkillLoadState::CriticalReminder {
            tracker.state = SkillLoadState::CriticalReminder;
            return true;
        }

        false
    }

    /// 加载技能的关键提醒
    ///
    /// 加载技能的精简摘要（约100 tokens），包含关键的操作要点和常见错误提示。
    /// 用于错误重试场景，帮助模型快速纠正错误。
    ///
    /// # Arguments
    ///
    /// * `skill_name` - 技能名称
    ///
    /// # Returns
    ///
    /// 返回技能的关键提醒内容。
    ///
    /// # Errors
    ///
    /// 如果技能不存在或加载失败，返回错误。
    pub async fn load_skill_critical_reminder(
        &self,
        skill_name: &str,
    ) -> Result<SkillContent, AgentError> {
        let definitions = self
            .skill_service
            .discover_skills()
            .await
            .map_err(|e| AgentError::DefinitionNotFound(e.to_string()))?;

        let skill_def = definitions
            .iter()
            .find(|s| s.name == skill_name)
            .ok_or_else(|| AgentError::DefinitionNotFound(skill_name.to_string()))?;

        let critical_reminder = self
            .skill_service
            .get_critical_reminder(&skill_def.id)
            .await
            .map_err(|e| AgentError::DefinitionNotFound(e.to_string()))?
            .unwrap_or_else(|| {
                format!(
                    "[Critical Reminder for {}] {}",
                    skill_name,
                    skill_def
                        .description
                        .split('.')
                        .take(2)
                        .collect::<Vec<_>>()
                        .join(".")
                )
            });

        Ok(SkillContent::CriticalReminder(critical_reminder))
    }

    /// 加载技能内容
    ///
    /// 根据当前的加载状态，返回相应的技能内容。
    /// 实现 Demand Paging 三层加载模型：
    /// - Layer 1: 摘要始终加载（已在 system prompt）
    /// - Layer 2: 首次使用工具时，加载完整指南作为 `tool_result`
    /// - Layer 3: 错误重试时，加载关键提醒
    ///
    /// # Arguments
    ///
    /// * `skill_name` - 技能名称
    /// * `usage_context` - 使用上下文，包含工具名、回合数等信息
    ///
    /// # Returns
    ///
    /// 返回技能内容。
    ///
    /// # Errors
    ///
    /// 如果技能不存在或加载失败，返回错误。
    pub async fn load_skill(
        &self,
        skill_name: &str,
        usage_context: &UsageContext,
    ) -> Result<SkillLoadState, AgentError> {
        let current_state = {
            let mut states = self.skill_usage_states.lock().await;
            let tracker = states.entry(skill_name.to_string()).or_default();

            let new_state = if usage_context.recent_errors >= 2 {
                SkillLoadState::CriticalReminder
            } else {
                SkillLoadState::FullGuide
            };

            tracker.state = new_state;
            new_state
        };

        Ok(current_state)
    }

    /// 加载技能内容（根据状态返回具体内容）
    ///
    /// 根据指定的状态加载相应的技能内容。
    ///
    /// # Arguments
    ///
    /// * `skill_name` - 技能名称
    /// * `state` - 目标加载状态
    ///
    /// # Returns
    ///
    /// 返回技能内容。
    ///
    /// # Errors
    ///
    /// 如果技能不存在或加载失败，返回错误。
    pub async fn load_skill_content(
        &self,
        skill_name: &str,
        state: SkillLoadState,
    ) -> Result<SkillContent, AgentError> {
        match state {
            SkillLoadState::Summary => {
                let definitions = self
                    .skill_service
                    .discover_skills()
                    .await
                    .map_err(|e| AgentError::DefinitionNotFound(e.to_string()))?;

                let skill_def = definitions
                    .iter()
                    .find(|s| s.name == skill_name)
                    .ok_or_else(|| AgentError::DefinitionNotFound(skill_name.to_string()))?;

                Ok(SkillContent::Summary(skill_def.description.clone()))
            },
            SkillLoadState::FullGuide => {
                let definitions = self
                    .skill_service
                    .discover_skills()
                    .await
                    .map_err(|e| AgentError::DefinitionNotFound(e.to_string()))?;

                let skill_def = definitions
                    .iter()
                    .find(|s| s.name == skill_name)
                    .ok_or_else(|| AgentError::DefinitionNotFound(skill_name.to_string()))?;

                let activated = self
                    .skill_service
                    .activate(&skill_def.id)
                    .await
                    .map_err(|e| AgentError::DefinitionNotFound(e.to_string()))?;

                Ok(SkillContent::FullGuide(activated.instruction))
            },
            SkillLoadState::CriticalReminder => self.load_skill_critical_reminder(skill_name).await,
        }
    }

    /// 加载技能并指定目标状态
    ///
    /// # Arguments
    ///
    /// * `skill_name` - 技能名称
    /// * `context` - 使用上下文
    /// * `target_state` - 目标加载状态
    async fn load_skill_with_state(
        &self,
        skill_name: &str,
        _context: &UsageContext,
        target_state: SkillLoadState,
    ) -> Result<SkillContent, AgentError> {
        let definitions = self
            .skill_service
            .discover_skills()
            .await
            .map_err(|e| AgentError::DefinitionNotFound(e.to_string()))?;

        let skill_def = definitions
            .iter()
            .find(|s| s.name == skill_name)
            .ok_or_else(|| AgentError::DefinitionNotFound(skill_name.to_string()))?;

        match target_state {
            SkillLoadState::Summary => Ok(SkillContent::Summary(skill_def.description.clone())),
            SkillLoadState::FullGuide => {
                let activated = self
                    .skill_service
                    .activate(&skill_def.id)
                    .await
                    .map_err(|e| AgentError::DefinitionNotFound(e.to_string()))?;

                Ok(SkillContent::FullGuide(activated.instruction))
            },
            SkillLoadState::CriticalReminder => self.load_skill_critical_reminder(skill_name).await,
        }
    }

    /// 记录工具错误
    ///
    /// 记录技能使用过程中的工具错误，用于动态调整加载策略。
    ///
    /// # Arguments
    ///
    /// * `skill_name` - 技能名称
    /// * `tool_name` - 工具名称
    pub async fn record_tool_error(&self, skill_name: &str, tool_name: &str) {
        let mut states = self.skill_usage_states.lock().await;
        let tracker = states.entry(skill_name.to_string()).or_default();

        if let Some(ref last_tool) = tracker.last_tool_name {
            if last_tool == tool_name {
                tracker.consecutive_errors += 1;
            } else {
                tracker.consecutive_errors = 1;
                tracker.last_tool_name = Some(tool_name.to_string());
            }
        } else {
            tracker.consecutive_errors = 1;
            tracker.last_tool_name = Some(tool_name.to_string());
        }
    }

    /// 根据工具加载相关技能
    ///
    /// 当工具被调用时，触发技能的按需加载。
    /// 技能使用 Demand Paging 机制动态调整加载级别。
    ///
    /// # Arguments
    ///
    /// * `tool_name` - 被调用的工具名称
    /// * `turn` - 当前回合数
    ///
    /// # Returns
    ///
    /// 返回加载的技能内容，如果没有则返回 None
    async fn load_skill_for_tool(
        &self,
        tool_name: &str,
        turn: usize,
    ) -> Result<Option<SkillContent>, AgentError> {
        let should_load_full_guide = tool_name.starts_with("activate::");

        let definitions = self
            .skill_service
            .discover_skills()
            .await
            .map_err(|e| AgentError::DefinitionNotFound(e.to_string()))?;

        let mut skill_content = None;

        for skill_def in definitions {
            let usage_context = UsageContext {
                tool_name: tool_name.to_string(),
                turn,
                recent_errors: 0,
            };

            let target_state = if should_load_full_guide {
                SkillLoadState::FullGuide
            } else {
                SkillLoadState::Summary
            };

            match self
                .load_skill_with_state(&skill_def.name, &usage_context, target_state)
                .await
            {
                Ok(content) => {
                    let state_str = match &content {
                        SkillContent::Summary(_) => "Summary",
                        SkillContent::FullGuide(_) => "FullGuide",
                        SkillContent::CriticalReminder(_) => "CriticalReminder",
                    };
                    tracing::debug!(
                        skill_name = %skill_def.name,
                        tool_name = %tool_name,
                        state = state_str,
                        "Skill loaded for tool"
                    );
                    if should_load_full_guide && matches!(content, SkillContent::FullGuide(_)) {
                        skill_content = Some(content);
                    }
                },
                Err(e) => {
                    tracing::debug!(skill_name = %skill_def.name, error = %e, "Skill load skipped");
                },
            }
        }

        Ok(skill_content)
    }

    /// 根据工具加载相关技能（带错误计数）
    ///
    /// 当工具执行出错后重试时，触发 Layer 3 关键提醒加载。
    ///
    /// # Arguments
    ///
    /// * `tool_name` - 被调用的工具名称
    /// * `turn` - 当前回合数
    /// * `error_count` - 连续错误次数
    ///
    /// # Returns
    ///
    /// 返回加载的技能内容，如果没有则返回 None
    #[allow(dead_code)]
    async fn load_skill_for_tool_with_retry(
        &self,
        tool_name: &str,
        turn: usize,
        error_count: usize,
    ) -> Result<Option<SkillContent>, AgentError> {
        let definitions = self
            .skill_service
            .discover_skills()
            .await
            .map_err(|e| AgentError::DefinitionNotFound(e.to_string()))?;

        let mut skill_content = None;

        for skill_def in definitions {
            let usage_context = UsageContext {
                tool_name: tool_name.to_string(),
                turn,
                recent_errors: error_count,
            };

            let target_state = if error_count >= 2 {
                SkillLoadState::CriticalReminder
            } else if tool_name.starts_with("activate::") {
                SkillLoadState::FullGuide
            } else {
                SkillLoadState::Summary
            };

            match self
                .load_skill_with_state(&skill_def.name, &usage_context, target_state)
                .await
            {
                Ok(content) => {
                    if matches!(content, SkillContent::CriticalReminder(_)) {
                        skill_content = Some(content);
                    }
                },
                Err(e) => {
                    tracing::debug!(skill_name = %skill_def.name, error = %e, "Skill load skipped");
                },
            }
        }

        Ok(skill_content)
    }

    /// 注册触发器
    ///
    /// 添加一个新的触发器到引擎。
    ///
    /// # Arguments
    ///
    /// * `trigger` - 要添加的触发器
    pub fn register_trigger(&mut self, trigger: EventTrigger) {
        self.triggers.push(trigger);
    }

    /// 移除触发器
    ///
    /// 根据 ID 移除已注册的触发器。
    ///
    /// # Arguments
    ///
    /// * `id` - 触发器 ID
    ///
    /// # Returns
    ///
    /// 如果找到并移除了触发器，返回 `true`。
    pub fn remove_trigger(&mut self, id: &str) -> bool {
        let initial_len = self.triggers.len();
        self.triggers.retain(|t| t.id() != id);
        self.triggers.len() < initial_len
    }

    /// 获取触发器数量
    ///
    /// # Returns
    ///
    /// 返回已注册的触发器数量。
    #[must_use]
    pub fn trigger_count(&self) -> usize {
        self.triggers.len()
    }

    /// 启用/禁用所有触发器
    ///
    /// # Arguments
    ///
    /// * `enabled` - 是否启用触发器
    pub fn set_triggers_enabled(&mut self, enabled: bool) {
        for trigger in &mut self.triggers {
            if enabled {
                trigger.enable();
            } else {
                trigger.disable();
            }
        }
    }

    /// 初始化触发器订阅
    ///
    /// 将所有已注册的触发器订阅到事件系统，使其能够响应事件。
    /// 应该在 Agent 运行前调用。
    #[allow(unused_variables)]
    pub fn init_trigger_subscriptions(&self) {
        let filter = EventFilter::new()
            .with_event_type(EventTypeFilter::Agent)
            .with_event_type(EventTypeFilter::Tool);

        for trigger in &self.triggers {
            let subscriber = self.event_publisher.subscribe(filter.clone());
        }
    }

    /// 执行触发器
    ///
    /// 手动触发所有匹配的触发器。
    /// 在工具执行完成后调用，以执行与工具调用相关的触发器。
    async fn execute_triggers(&self, event: crate::events::Event) {
        let core_event: neoco_core::events::Event = match event {
            crate::events::Event::Agent(ae) => neoco_core::events::Event::Agent(match ae {
                AgentEvent::Created { id, parent_ulid } => {
                    neoco_core::events::AgentEvent::Created { id, parent_ulid }
                },
                AgentEvent::StateChanged { id, old, new } => {
                    neoco_core::events::AgentEvent::StateChanged { id, old, new }
                },
                AgentEvent::MessageAdded { id, message_id } => {
                    neoco_core::events::AgentEvent::MessageAdded { id, message_id }
                },
                AgentEvent::ToolCalled { id, tool_id } => {
                    neoco_core::events::AgentEvent::ToolCalled { id, tool_id }
                },
                AgentEvent::ToolResult {
                    id,
                    tool_id,
                    success,
                } => neoco_core::events::AgentEvent::ToolResult {
                    id,
                    tool_id,
                    success,
                },
                AgentEvent::Completed { id, output } => {
                    neoco_core::events::AgentEvent::Completed { id, output }
                },
                AgentEvent::Error { id, error } => {
                    neoco_core::events::AgentEvent::Error { id, error }
                },
            }),
            _ => return,
        };

        for trigger in &self.triggers {
            if trigger.is_enabled() && trigger.matches(&core_event) {
                trigger.on_event(core_event.clone()).await;
            }
        }
    }
}

// ============================================================
// MultiAgentEngine trait 实现
// ============================================================

#[async_trait]
impl MultiAgentEngine for AgentEngine {
    async fn spawn_child(
        &self,
        parent_ulid: AgentUlid,
        definition_id: String,
        model_group: Option<String>,
        mcp_servers: Option<Vec<String>>,
        skills: Option<Vec<String>>,
    ) -> Result<AgentUlid, String> {
        AgentEngine::spawn_child(
            self,
            parent_ulid,
            definition_id,
            model_group,
            mcp_servers,
            skills,
        )
        .await
        .map_err(|e| e.to_string())
    }

    async fn send_message(
        &self,
        from: AgentUlid,
        to: AgentUlid,
        content: String,
    ) -> Result<(), String> {
        AgentEngine::send_message(self, from, to, content)
            .await
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    async fn report_progress(
        &self,
        agent_ulid: AgentUlid,
        progress: f64,
        content: String,
    ) -> Result<(), String> {
        AgentEngine::report_progress(self, agent_ulid, progress, content)
            .await
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_skill_load_state_order() {
        assert!(SkillLoadState::Summary < SkillLoadState::FullGuide);
        assert!(SkillLoadState::FullGuide < SkillLoadState::CriticalReminder);
    }

    #[test]
    fn test_skill_content_summary() {
        let content = SkillContent::Summary("Test summary".to_string());
        assert!(matches!(content, SkillContent::Summary(_)));
    }

    #[test]
    fn test_skill_content_fullguide() {
        let content = SkillContent::FullGuide("Full guide content".to_string());
        assert!(matches!(content, SkillContent::FullGuide(_)));
    }

    #[test]
    fn test_skill_content_critical_reminder() {
        let content = SkillContent::CriticalReminder("Critical reminder content".to_string());
        assert!(matches!(content, SkillContent::CriticalReminder(_)));
    }

    #[test]
    fn test_skill_usage_tracker_default() {
        let tracker = SkillUsageTracker::default();
        assert_eq!(tracker.state, SkillLoadState::Summary);
        assert_eq!(tracker.consecutive_errors, 0);
        assert!(tracker.last_tool_name.is_none());
    }

    #[test]
    fn test_usage_context() {
        let context = UsageContext {
            tool_name: "test_tool".to_string(),
            turn: 5,
            recent_errors: 2,
        };
        assert_eq!(context.tool_name, "test_tool");
        assert_eq!(context.turn, 5);
        assert_eq!(context.recent_errors, 2);
    }

    #[tokio::test]
    async fn test_skill_load_state_transitions() {
        let tracker = SkillUsageTracker::default();
        assert_eq!(tracker.state, SkillLoadState::Summary);

        let mut tracker = tracker;
        tracker.state = SkillLoadState::FullGuide;
        assert_eq!(tracker.state, SkillLoadState::FullGuide);

        tracker.state = SkillLoadState::CriticalReminder;
        assert_eq!(tracker.state, SkillLoadState::CriticalReminder);
    }
}
