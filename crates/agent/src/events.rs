//! Agent 驱动架构的事件系统
//!
//! 定义了 Agent、Session、Workflow、Tool 和 System 的事件类型。

#![allow(clippy::missing_panics_doc)]
#![allow(clippy::unused_self)]
#![allow(clippy::unnested_or_patterns)]
#![allow(clippy::match_same_arms)]

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use neoco_core::events::{AgentState, SessionType as CoreSessionType};
use neoco_core::ids::{AgentUlid, SessionUlid, ToolId};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

pub use neoco_core::events::{AgentEvent as CoreAgentEvent, Event as CoreEvent};

/// 领域事件
///
/// 表示系统中发生的各种事件。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Event {
    /// Session 事件
    Session(SessionEvent),
    /// Agent 事件
    Agent(AgentEvent),
    /// Workflow 事件
    Workflow(WorkflowEvent),
    /// 工具事件
    Tool(ToolEvent),
    /// 系统事件
    System(SystemEvent),
}

/// Session 事件
///
/// 表示 Session 生命周期中的各种事件。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SessionEvent {
    /// Session 已创建
    Created {
        /// Session ID
        id: SessionUlid,
        /// Session 类型
        session_type: SessionType,
    },
    /// Session 已更新
    Updated {
        /// Session ID
        id: SessionUlid,
    },
    /// Session 已删除
    Deleted {
        /// Session ID
        id: SessionUlid,
    },
}

/// Session 类型
///
/// 表示 Session 的类型。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionType {
    /// Direct session with optional initial message.
    Direct {
        /// Initial message.
        initial_message: Option<String>,
    },
    /// TUI session.
    #[serde(alias = "repl")]
    Tui,
    /// Workflow session.
    Workflow {
        /// Workflow ID.
        workflow_id: String,
    },
}

impl From<CoreSessionType> for SessionType {
    fn from(st: CoreSessionType) -> Self {
        match st {
            CoreSessionType::Direct { initial_message } => SessionType::Direct { initial_message },
            CoreSessionType::Tui => SessionType::Tui,
            CoreSessionType::Workflow { workflow_id } => SessionType::Workflow { workflow_id },
        }
    }
}

/// Agent 事件
///
/// 表示 Agent 生命周期中的各种事件。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AgentEvent {
    /// Agent 已创建
    Created {
        /// Agent ID
        id: AgentUlid,
        /// 父 Agent ID
        parent_ulid: Option<AgentUlid>,
    },
    /// Agent 状态已改变
    StateChanged {
        /// Agent ID
        id: AgentUlid,
        /// 旧状态
        old: AgentState,
        /// 新状态
        new: AgentState,
    },
    /// 消息已添加
    MessageAdded {
        /// Agent ID
        id: AgentUlid,
        /// 消息 ID
        message_id: neoco_core::ids::MessageId,
    },
    /// 工具已调用
    ToolCalled {
        /// Agent ID
        id: AgentUlid,
        /// 工具 ID
        tool_id: ToolId,
    },
    /// 工具调用结果
    ToolResult {
        /// Agent ID
        id: AgentUlid,
        /// 工具 ID
        tool_id: ToolId,
        /// 是否成功
        success: bool,
    },
    /// Agent 已完成
    Completed {
        /// Agent ID
        id: AgentUlid,
        /// 输出内容
        output: String,
    },
    /// Agent 发生错误
    Error {
        /// Agent ID
        id: AgentUlid,
        /// 错误信息
        error: String,
    },
}

impl From<CoreAgentEvent> for AgentEvent {
    fn from(e: CoreAgentEvent) -> Self {
        match e {
            CoreAgentEvent::Created { id, parent_ulid } => AgentEvent::Created { id, parent_ulid },
            CoreAgentEvent::StateChanged { id, old, new } => {
                AgentEvent::StateChanged { id, old, new }
            },
            CoreAgentEvent::MessageAdded { id, message_id } => {
                AgentEvent::MessageAdded { id, message_id }
            },
            CoreAgentEvent::ToolCalled { id, tool_id } => AgentEvent::ToolCalled { id, tool_id },
            CoreAgentEvent::ToolResult {
                id,
                tool_id,
                success,
            } => AgentEvent::ToolResult {
                id,
                tool_id,
                success,
            },
            CoreAgentEvent::Completed { id, output } => AgentEvent::Completed { id, output },
            CoreAgentEvent::Error { id, error } => AgentEvent::Error { id, error },
        }
    }
}

/// Workflow 事件
///
/// 表示 Workflow 执行过程中的各种事件。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WorkflowEvent {
    /// Workflow 已启动
    Started {
        /// Session ID
        session_ulid: SessionUlid,
        /// 定义
        definition: String,
    },
    /// 节点已启动
    NodeStarted {
        /// Session ID
        session_ulid: SessionUlid,
        /// 节点 ID
        node_ulid: neoco_core::ids::NodeUlid,
    },
    /// 节点已完成
    NodeCompleted {
        /// Session ID
        session_ulid: SessionUlid,
        /// 节点 ID
        node_ulid: neoco_core::ids::NodeUlid,
        /// 结果
        result: String,
    },
    /// 节点转换
    Transition {
        /// Session ID
        session_ulid: SessionUlid,
        /// 源节点 ID
        from: neoco_core::ids::NodeUlid,
        /// 目标节点 ID
        to: neoco_core::ids::NodeUlid,
    },
    /// Workflow 已完成
    Completed {
        /// Session ID
        session_ulid: SessionUlid,
    },
    /// Workflow 失败
    Failed {
        /// Session ID
        session_ulid: SessionUlid,
        /// 错误信息
        error: String,
    },
}

/// 工具事件
///
/// 表示工具相关的各种事件。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ToolEvent {
    /// 工具已注册
    Registered {
        /// 工具 ID
        tool_id: ToolId,
    },
    /// 工具正在执行
    Executing {
        /// 工具 ID
        tool_id: ToolId,
        /// Agent ID
        agent_ulid: AgentUlid,
    },
    /// 工具已执行
    Executed {
        /// 工具 ID
        tool_id: ToolId,
        /// Agent ID
        agent_ulid: AgentUlid,
        /// 是否成功
        success: bool,
    },
    /// 工具发生错误
    Error {
        /// 工具 ID
        tool_id: ToolId,
        /// 错误信息
        error: String,
    },
}

/// 系统事件
///
/// 表示系统级别的各种事件。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SystemEvent {
    /// 系统启动
    Startup,
    /// 系统关闭
    Shutdown,
    /// 系统错误
    Error {
        /// 错误来源
        source: String,
        /// 错误信息
        message: String,
    },
}

impl From<neoco_core::events::SystemEvent> for SystemEvent {
    fn from(event: neoco_core::events::SystemEvent) -> Self {
        match event {
            neoco_core::events::SystemEvent::Startup => SystemEvent::Startup,
            neoco_core::events::SystemEvent::Shutdown => SystemEvent::Shutdown,
            neoco_core::events::SystemEvent::Error { source, message } => {
                SystemEvent::Error { source, message }
            },
        }
    }
}

/// 触发器模式
///
/// 定义触发器匹配事件的条件。
#[derive(Debug, Clone)]
pub enum TriggerPattern {
    /// 匹配所有事件
    All,
    /// 匹配特定的生命周期事件
    Lifecycle {
        /// 要匹配的事件列表
        events: Vec<LifecycleEvent>,
    },
    /// 匹配特定类型的 Agent 生成事件
    AgentSpawned {
        /// Agent 类型（可选）
        agent_type: Option<String>,
    },
    /// 匹配 Agent 终止事件
    AgentTerminated,
    /// 匹配包含特定关键词的事件
    SystemKeyword {
        /// 关键词列表
        keywords: Vec<String>,
    },
    /// 匹配符合正则表达式的内容
    ContentMatch {
        /// 正则表达式模式
        pattern: String,
    },
    /// 收到消息时触发
    OnMessage,
    /// 工具调用时触发
    OnToolCall,
    /// 发生错误时触发
    OnError,
    /// 任务完成时触发
    OnComplete,
    /// 超时时触发
    OnTimeout,
    /// 条件满足时触发
    OnCondition(Condition),
}

impl TriggerPattern {
    /// Check if this pattern matches the given event.
    #[must_use]
    pub fn matches(&self, event: &Event) -> bool {
        match self {
            TriggerPattern::All => true,
            TriggerPattern::Lifecycle { events } => {
                if let Event::Agent(agent_event) = event {
                    let lifecycle_event = match agent_event {
                        AgentEvent::Created { .. } => Some(LifecycleEvent::Created),
                        AgentEvent::StateChanged { .. } => Some(LifecycleEvent::StateChanged),
                        AgentEvent::MessageAdded { .. } => Some(LifecycleEvent::MessageAdded),
                        AgentEvent::ToolCalled { .. } => Some(LifecycleEvent::ToolCalled),
                        AgentEvent::ToolResult { .. } => Some(LifecycleEvent::ToolResult),
                        AgentEvent::Completed { .. } => Some(LifecycleEvent::Completed),
                        AgentEvent::Error { .. } => Some(LifecycleEvent::Error),
                    };
                    events.contains(&lifecycle_event.unwrap())
                } else {
                    false
                }
            },
            TriggerPattern::AgentSpawned { agent_type } => {
                if let Event::Agent(AgentEvent::Created { id, .. }) = event {
                    if let Some(expected_type) = agent_type {
                        id.to_string().contains(expected_type)
                    } else {
                        true
                    }
                } else {
                    false
                }
            },
            TriggerPattern::AgentTerminated => {
                matches!(
                    event,
                    Event::Agent(AgentEvent::Completed { .. } | AgentEvent::Error { .. })
                )
            },
            TriggerPattern::SystemKeyword { keywords } => {
                let content = match event {
                    Event::Agent(AgentEvent::Completed { output, .. }) => output.clone(),
                    Event::Agent(AgentEvent::Error { error, .. }) => error.clone(),
                    Event::System(SystemEvent::Error { message, .. }) => message.clone(),
                    _ => String::new(),
                };
                let content_lower = content.to_lowercase();
                keywords
                    .iter()
                    .any(|k| content_lower.contains(&k.to_lowercase()))
            },
            TriggerPattern::ContentMatch { pattern } => {
                let content = match event {
                    Event::Agent(AgentEvent::Completed { output, .. }) => output.clone(),
                    Event::Agent(AgentEvent::Error { error, .. }) => error.clone(),
                    _ => String::new(),
                };
                regex::Regex::new(pattern)
                    .map(|re| re.is_match(&content))
                    .unwrap_or(false)
            },
            TriggerPattern::OnMessage => {
                matches!(
                    event,
                    Event::Agent(AgentEvent::MessageAdded { .. })
                        | Event::Agent(AgentEvent::Completed { .. })
                )
            },
            TriggerPattern::OnToolCall => {
                matches!(
                    event,
                    Event::Agent(AgentEvent::ToolCalled { .. })
                        | Event::Tool(ToolEvent::Executing { .. })
                )
            },
            TriggerPattern::OnError => {
                matches!(
                    event,
                    Event::Agent(AgentEvent::Error { .. })
                        | Event::Tool(ToolEvent::Error { .. })
                        | Event::System(SystemEvent::Error { .. })
                )
            },
            TriggerPattern::OnComplete => {
                matches!(event, Event::Agent(AgentEvent::Completed { .. }))
            },
            TriggerPattern::OnTimeout => {
                if let Event::Agent(AgentEvent::Error { error, .. }) = event {
                    error.to_lowercase().contains("timeout")
                } else {
                    false
                }
            },
            TriggerPattern::OnCondition(condition) => Self::matches_condition(condition, event),
        }
    }

    /// 检查事件是否匹配条件。
    fn matches_condition(condition: &Condition, event: &Event) -> bool {
        match condition {
            Condition::MessageContains { pattern } => {
                let content = match event {
                    Event::Agent(AgentEvent::Completed { output, .. }) => output.clone(),
                    Event::Agent(AgentEvent::MessageAdded { .. }) => String::new(),
                    _ => String::new(),
                };
                content.to_lowercase().contains(&pattern.to_lowercase())
            },
            Condition::ToolCalled { tool_name } => match event {
                Event::Agent(AgentEvent::ToolCalled { tool_id, .. }) => {
                    tool_id.to_string().contains(tool_name)
                },
                Event::Tool(ToolEvent::Executing { tool_id, .. }) => {
                    tool_id.to_string().contains(tool_name)
                },
                _ => false,
            },
            Condition::ErrorType { error_kind } => {
                let event_error = match event {
                    Event::Agent(AgentEvent::Error { error, .. }) => Some(error.clone()),
                    Event::Tool(ToolEvent::Error { error, .. }) => Some(error.clone()),
                    Event::System(SystemEvent::Error { message, .. }) => Some(message.clone()),
                    _ => None,
                };
                if let Some(err) = event_error {
                    match error_kind {
                        ErrorKind::Model => err.to_lowercase().contains("model"),
                        ErrorKind::Tool => err.to_lowercase().contains("tool"),
                        ErrorKind::Authentication => {
                            err.to_lowercase().contains("auth")
                                || err.to_lowercase().contains("unauthorized")
                        },
                        ErrorKind::Permission => {
                            err.to_lowercase().contains("permission")
                                || err.to_lowercase().contains("denied")
                        },
                        ErrorKind::Timeout => err.to_lowercase().contains("timeout"),
                        ErrorKind::ResourceExhausted => {
                            err.to_lowercase().contains("resource")
                                || err.to_lowercase().contains("quota")
                        },
                        ErrorKind::Unknown => true,
                    }
                } else {
                    false
                }
            },
            Condition::TokenLimit { percent: _ } => false,
        }
    }
}

/// 生命周期事件类型
///
/// 表示 Agent 生命周期中的各种事件类型。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum LifecycleEvent {
    /// Agent 已创建
    Created,
    /// Agent 状态已改变
    StateChanged,
    /// 消息已添加
    MessageAdded,
    /// 工具已调用
    ToolCalled,
    /// 工具调用结果
    ToolResult,
    /// Agent 已完成
    Completed,
    /// Agent 发生错误
    Error,
}

/// 条件结构
///
/// 用于定义更复杂的触发条件。
#[derive(Debug, Clone)]
pub enum Condition {
    /// 消息包含指定内容
    MessageContains {
        /// 要匹配的文本模式
        pattern: String,
    },
    /// 调用了指定工具
    ToolCalled {
        /// 工具名称
        tool_name: String,
    },
    /// 发生指定类型错误
    ErrorType {
        /// 错误类型
        error_kind: ErrorKind,
    },
    /// Token 使用率超过阈值
    TokenLimit {
        /// 使用率阈值 (0.0-1.0)
        percent: f64,
    },
}

/// 错误类型
///
/// 定义不同类型的错误。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ErrorKind {
    /// 模型调用错误
    Model,
    /// 工具执行错误
    Tool,
    /// 认证错误
    Authentication,
    /// 权限错误
    Permission,
    /// 超时错误
    Timeout,
    /// 资源不足
    ResourceExhausted,
    /// 未知错误
    Unknown,
}

/// 触发器处理器
///
/// 根据触发器模式执行相应的动作。
pub struct TriggerHandler {
    /// 触发器 ID
    pub id: String,
    /// 触发器模式
    pub pattern: TriggerPattern,
    /// 触发器动作
    pub action: TriggerAction,
    /// 是否启用
    pub enabled: bool,
    /// 触发器执行器
    executor: Option<Arc<TriggerExecutor>>,
}

impl TriggerHandler {
    /// 创建新的触发器处理器
    #[must_use]
    pub fn new(id: String, pattern: TriggerPattern, action: TriggerAction) -> Self {
        Self {
            id,
            pattern,
            action,
            enabled: true,
            executor: None,
        }
    }

    /// 创建带有执行器的触发器处理器
    #[must_use]
    pub fn with_executor(
        id: String,
        pattern: TriggerPattern,
        action: TriggerAction,
        executor: Arc<TriggerExecutor>,
    ) -> Self {
        Self {
            id,
            pattern,
            action,
            enabled: true,
            executor: Some(executor),
        }
    }

    /// 设置执行器
    pub fn set_executor(&mut self, executor: Arc<TriggerExecutor>) {
        self.executor = Some(executor);
    }

    /// 获取动作引用
    #[must_use]
    pub fn action(&self) -> &TriggerAction {
        &self.action
    }

    /// 判断事件是否匹配触发器模式
    #[must_use]
    pub fn matches(&self, event: &Event) -> bool {
        match &self.pattern {
            TriggerPattern::All => true,
            TriggerPattern::Lifecycle { events } => {
                let event_type = match event {
                    Event::Agent(AgentEvent::Created { .. }) => Some(LifecycleEvent::Created),
                    Event::Agent(AgentEvent::StateChanged { .. }) => {
                        Some(LifecycleEvent::StateChanged)
                    },
                    Event::Agent(AgentEvent::MessageAdded { .. }) => {
                        Some(LifecycleEvent::MessageAdded)
                    },
                    Event::Agent(AgentEvent::ToolCalled { .. }) => Some(LifecycleEvent::ToolCalled),
                    Event::Agent(AgentEvent::ToolResult { .. }) => Some(LifecycleEvent::ToolResult),
                    Event::Agent(AgentEvent::Completed { .. }) => Some(LifecycleEvent::Completed),
                    Event::Agent(AgentEvent::Error { .. }) => Some(LifecycleEvent::Error),
                    _ => None,
                };
                event_type.is_some_and(|e| events.contains(&e))
            },
            TriggerPattern::AgentSpawned { agent_type } => {
                if let Event::Agent(AgentEvent::Created { id, .. }) = event {
                    if let Some(expected_type) = agent_type {
                        id.to_string().contains(expected_type)
                    } else {
                        true
                    }
                } else {
                    false
                }
            },
            TriggerPattern::AgentTerminated => {
                matches!(
                    event,
                    Event::Agent(AgentEvent::Completed { .. } | AgentEvent::Error { .. })
                )
            },
            TriggerPattern::SystemKeyword { keywords } => {
                let content = match event {
                    Event::Agent(AgentEvent::Completed { output, .. }) => output.clone(),
                    Event::Agent(AgentEvent::Error { error, .. }) => error.clone(),
                    Event::System(SystemEvent::Error { message, .. }) => message.clone(),
                    _ => String::new(),
                };
                let content_lower = content.to_lowercase();
                keywords
                    .iter()
                    .any(|k| content_lower.contains(&k.to_lowercase()))
            },
            TriggerPattern::ContentMatch { pattern } => {
                let content = match event {
                    Event::Agent(AgentEvent::Completed { output, .. }) => output.clone(),
                    Event::Agent(AgentEvent::Error { error, .. }) => error.clone(),
                    _ => String::new(),
                };
                regex::Regex::new(pattern)
                    .map(|re| re.is_match(&content))
                    .unwrap_or(false)
            },
            TriggerPattern::OnMessage => {
                matches!(
                    event,
                    Event::Agent(AgentEvent::MessageAdded { .. })
                        | Event::Agent(AgentEvent::Completed { .. })
                )
            },
            TriggerPattern::OnToolCall => {
                matches!(
                    event,
                    Event::Agent(AgentEvent::ToolCalled { .. })
                        | Event::Tool(ToolEvent::Executing { .. })
                )
            },
            TriggerPattern::OnError => {
                matches!(
                    event,
                    Event::Agent(AgentEvent::Error { .. })
                        | Event::Tool(ToolEvent::Error { .. })
                        | Event::System(SystemEvent::Error { .. })
                )
            },
            TriggerPattern::OnComplete => {
                matches!(event, Event::Agent(AgentEvent::Completed { .. }))
            },
            TriggerPattern::OnTimeout => {
                if let Event::Agent(AgentEvent::Error { error, .. }) = event {
                    error.to_lowercase().contains("timeout")
                } else {
                    false
                }
            },
            TriggerPattern::OnCondition(condition) => self.matches_condition(condition, event),
        }
    }

    /// 检查事件是否匹配条件。
    fn matches_condition(&self, condition: &Condition, event: &Event) -> bool {
        match condition {
            Condition::MessageContains { pattern } => {
                let content = match event {
                    Event::Agent(AgentEvent::Completed { output, .. }) => output.clone(),
                    Event::Agent(AgentEvent::MessageAdded { .. }) => String::new(),
                    _ => String::new(),
                };
                content.to_lowercase().contains(&pattern.to_lowercase())
            },
            Condition::ToolCalled { tool_name } => match event {
                Event::Agent(AgentEvent::ToolCalled { tool_id, .. }) => {
                    tool_id.to_string().contains(tool_name)
                },
                Event::Tool(ToolEvent::Executing { tool_id, .. }) => {
                    tool_id.to_string().contains(tool_name)
                },
                _ => false,
            },
            Condition::ErrorType { error_kind } => {
                let event_error = match event {
                    Event::Agent(AgentEvent::Error { error, .. }) => Some(error.clone()),
                    Event::Tool(ToolEvent::Error { error, .. }) => Some(error.clone()),
                    Event::System(SystemEvent::Error { message, .. }) => Some(message.clone()),
                    _ => None,
                };
                if let Some(err) = event_error {
                    match error_kind {
                        ErrorKind::Model => err.to_lowercase().contains("model"),
                        ErrorKind::Tool => err.to_lowercase().contains("tool"),
                        ErrorKind::Authentication => {
                            err.to_lowercase().contains("auth")
                                || err.to_lowercase().contains("unauthorized")
                        },
                        ErrorKind::Permission => {
                            err.to_lowercase().contains("permission")
                                || err.to_lowercase().contains("denied")
                        },
                        ErrorKind::Timeout => err.to_lowercase().contains("timeout"),
                        ErrorKind::ResourceExhausted => {
                            err.to_lowercase().contains("resource")
                                || err.to_lowercase().contains("quota")
                        },
                        ErrorKind::Unknown => true,
                    }
                } else {
                    false
                }
            },
            Condition::TokenLimit { percent: _ } => false,
        }
    }
}

impl Clone for TriggerHandler {
    fn clone(&self) -> Self {
        Self {
            id: self.id.clone(),
            pattern: self.pattern.clone(),
            action: self.action.clone(),
            enabled: self.enabled,
            executor: self.executor.clone(),
        }
    }
}

#[async_trait]
impl TriggerHandlerTrait for TriggerHandler {
    async fn handle_trigger(&self, event: &Event, context: &TriggerContext) -> TriggerResult {
        if !self.enabled {
            return TriggerResult::Skipped;
        }

        if !self.matches(event) {
            return TriggerResult::Skipped;
        }

        if let Some(ref executor) = self.executor {
            executor.execute_action(&self.action, event, context).await
        } else {
            match &self.action {
                TriggerAction::Log { level, message } => {
                    match level {
                        log::Level::Error => tracing::error!("[Trigger] {}", message),
                        log::Level::Warn => tracing::warn!("[Trigger] {}", message),
                        log::Level::Info => tracing::info!("[Trigger] {}", message),
                        log::Level::Debug => tracing::debug!("[Trigger] {}", message),
                        log::Level::Trace => tracing::trace!("[Trigger] {}", message),
                    }
                    TriggerResult::Success
                },
                _ => TriggerResult::Failure {
                    message: "No executor configured".to_string(),
                },
            }
        }
    }

    fn id(&self) -> &str {
        &self.id
    }

    fn pattern(&self) -> &TriggerPattern {
        &self.pattern
    }
}

/// 触发器动作
///
/// 定义触发器匹配后要执行的动作。
#[derive(Debug, Clone)]
pub enum TriggerAction {
    /// 执行工具
    ExecuteTool {
        /// 工具名称
        tool_name: String,
        /// 工具参数
        args: serde_json::Value,
    },
    /// 发送消息
    SendMessage {
        /// 目标 Agent
        target: AgentUlid,
        /// 消息内容
        content: String,
    },
    /// 调用回调
    Callback {
        /// 回调 ID
        callback_id: String,
    },
    /// 记录日志
    Log {
        /// 日志级别
        level: log::Level,
        /// 日志消息
        message: String,
    },
    /// 发出事件
    EmitEvent {
        /// 事件类型
        event_type: String,
        /// 事件载荷
        payload: serde_json::Value,
    },
}

/// 触发器动作执行器 trait
///
/// 定义执行触发器动作的接口。
#[async_trait]
pub trait TriggerActionExecutor: Send + Sync {
    /// 执行工具动作
    async fn execute_tool(
        &self,
        tool_name: &str,
        args: serde_json::Value,
    ) -> Result<String, TriggerExecutorError>;

    /// 发送消息动作
    async fn send_message(
        &self,
        target: AgentUlid,
        content: String,
    ) -> Result<(), TriggerExecutorError>;

    /// 调用回调动作
    async fn call_callback(
        &self,
        callback_id: &str,
        event: &Event,
    ) -> Result<(), TriggerExecutorError>;

    /// 发出事件动作
    async fn emit_event(
        &self,
        event_type: String,
        payload: serde_json::Value,
    ) -> Result<(), TriggerExecutorError>;
}

/// 触发器执行器错误
#[derive(Debug, thiserror::Error)]
pub enum TriggerExecutorError {
    /// 工具执行失败
    #[error("工具执行失败: {0}")]
    ToolError(String),
    /// 消息发送失败
    #[error("消息发送失败: {0}")]
    MessageError(String),
    /// 回调执行失败
    #[error("回调执行失败: {0}")]
    CallbackError(String),
    /// 事件发布失败
    #[error("事件发布失败: {0}")]
    EventError(String),
    /// 执行器未配置
    #[error("执行器未配置: {0}")]
    NotConfigured(String),
}

/// 触发器处理器 trait
///
/// 定义处理触发事件的接口。
#[async_trait]
pub trait TriggerHandlerTrait: Send + Sync {
    /// 处理触发事件
    ///
    /// # Arguments
    ///
    /// * `event` - 触发的事件
    /// * `context` - 触发上下文
    ///
    /// # Returns
    ///
    /// 返回处理结果。
    async fn handle_trigger(&self, event: &Event, context: &TriggerContext) -> TriggerResult;

    /// 获取触发器 ID
    fn id(&self) -> &str;

    /// 获取触发器模式
    fn pattern(&self) -> &TriggerPattern;
}

/// 触发上下文
///
/// 包含触发事件时的上下文信息。
#[derive(Debug, Clone)]
pub struct TriggerContext {
    /// 相关的 Agent ID
    pub agent_id: Option<AgentUlid>,
    /// 相关的 Session ID
    pub session_id: Option<SessionUlid>,
    /// 额外的数据
    pub data: std::collections::HashMap<String, serde_json::Value>,
}

impl TriggerContext {
    /// 创建新的触发上下文
    #[must_use]
    pub fn new() -> Self {
        Self {
            agent_id: None,
            session_id: None,
            data: std::collections::HashMap::new(),
        }
    }

    /// 设置 Agent ID
    #[must_use]
    pub fn with_agent(mut self, agent_id: AgentUlid) -> Self {
        self.agent_id = Some(agent_id);
        self
    }

    /// 设置 Session ID
    #[must_use]
    pub fn with_session(mut self, session_id: SessionUlid) -> Self {
        self.session_id = Some(session_id);
        self
    }

    /// 添加额外数据
    #[must_use]
    pub fn with_data(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.data.insert(key.into(), value);
        self
    }
}

impl Default for TriggerContext {
    fn default() -> Self {
        Self::new()
    }
}

/// 触发结果
///
/// 包含触发处理的结果。
#[derive(Debug, Clone)]
pub enum TriggerResult {
    /// 处理成功
    Success,
    /// 处理失败
    Failure {
        /// 错误信息
        message: String,
    },
    /// 跳过处理
    Skipped,
}

impl TriggerResult {
    /// 检查是否成功
    #[must_use]
    pub fn is_success(&self) -> bool {
        matches!(self, Self::Success)
    }
}

/// 回调函数类型
pub type TriggerCallback = Box<dyn Fn(&Event) -> Result<(), TriggerExecutorError> + Send + Sync>;

/// 触发器执行器
///
/// 统一管理触发器动作的执行。
pub struct TriggerExecutor {
    /// 工具注册表
    tool_registry: Option<Arc<dyn neoco_tool::registry::ToolRegistry>>,
    /// Agent 仓库
    agent_repo: Option<Arc<dyn neoco_core::traits::AgentRepository>>,
    /// 消息仓库
    message_repo: Option<Arc<dyn neoco_core::traits::MessageRepository>>,
    /// 事件发布器
    event_publisher: Option<Arc<dyn neoco_core::EventPublisher>>,
    /// 回调函数映射
    callbacks: Mutex<HashMap<String, TriggerCallback>>,
}

impl TriggerExecutor {
    /// 创建新的触发器执行器
    #[must_use]
    pub fn new() -> Self {
        Self {
            tool_registry: None,
            agent_repo: None,
            message_repo: None,
            event_publisher: None,
            callbacks: Mutex::new(HashMap::new()),
        }
    }

    /// 设置工具注册表
    #[must_use]
    pub fn with_tool_registry(
        mut self,
        registry: Arc<dyn neoco_tool::registry::ToolRegistry>,
    ) -> Self {
        self.tool_registry = Some(registry);
        self
    }

    /// 设置 Agent 仓库
    #[must_use]
    pub fn with_agent_repo(mut self, repo: Arc<dyn neoco_core::traits::AgentRepository>) -> Self {
        self.agent_repo = Some(repo);
        self
    }

    /// 设置消息仓库
    #[must_use]
    pub fn with_message_repo(
        mut self,
        repo: Arc<dyn neoco_core::traits::MessageRepository>,
    ) -> Self {
        self.message_repo = Some(repo);
        self
    }

    /// 设置事件发布器
    #[must_use]
    pub fn with_event_publisher(mut self, publisher: Arc<dyn neoco_core::EventPublisher>) -> Self {
        self.event_publisher = Some(publisher);
        self
    }

    /// 注册回调函数
    pub async fn register_callback<F>(&self, id: String, callback: F)
    where
        F: Fn(&Event) -> Result<(), TriggerExecutorError> + Send + Sync + 'static,
    {
        let mut callbacks = self.callbacks.lock().await;
        callbacks.insert(id, Box::new(callback));
    }

    /// 执行触发动作
    pub async fn execute_action(
        &self,
        action: &TriggerAction,
        event: &Event,
        context: &TriggerContext,
    ) -> TriggerResult {
        match action {
            TriggerAction::ExecuteTool { tool_name, args } => self
                .execute_tool_action(tool_name, args.clone())
                .await
                .into(),
            TriggerAction::SendMessage { target, content } => self
                .send_message_action(*target, content.clone(), context)
                .await
                .into(),
            TriggerAction::Callback { callback_id } => {
                self.callback_action(callback_id, event).await.into()
            },
            TriggerAction::Log { level, message } => {
                self.log_action(*level, message, event, context);
                TriggerResult::Success
            },
            TriggerAction::EmitEvent {
                event_type,
                payload,
            } => self
                .emit_event_action(event_type.clone(), payload.clone())
                .await
                .into(),
        }
    }

    /// Execute tool action
    #[allow(clippy::missing_docs_in_private_items)]
    async fn execute_tool_action(
        &self,
        tool_name: &str,
        args: serde_json::Value,
    ) -> Result<String, TriggerExecutorError> {
        let registry = self
            .tool_registry
            .as_ref()
            .ok_or_else(|| TriggerExecutorError::NotConfigured("tool_registry".to_string()))?;

        let tool_executor =
            registry
                .get(&tool_name.parse().map_err(|e| {
                    TriggerExecutorError::ToolError(format!("Invalid tool name: {e}"))
                })?)
                .await
                .ok_or_else(|| {
                    TriggerExecutorError::ToolError(format!("Tool not found: {tool_name}"))
                })?;

        let session_ulid = SessionUlid::new();
        let tool_context = neoco_core::ToolContext {
            session_ulid,
            agent_ulid: AgentUlid::new_root(&session_ulid),
            working_dir: std::path::PathBuf::new(),
            user_interaction_tx: None,
        };

        let result = tool_executor
            .execute(&tool_context, args)
            .await
            .map_err(|e| TriggerExecutorError::ToolError(e.to_string()))?;

        let output = match result.output {
            neoco_core::ToolOutput::Text(s) => s,
            neoco_core::ToolOutput::Json(j) => j.to_string(),
            neoco_core::ToolOutput::Binary(b) => format!("[binary {} bytes]", b.len()),
            neoco_core::ToolOutput::Empty => String::new(),
        };

        Ok(output)
    }

    /// Send message action
    #[allow(clippy::missing_docs_in_private_items)]
    async fn send_message_action(
        &self,
        target: AgentUlid,
        msg_content: String,
        trigger_ctx: &TriggerContext,
    ) -> Result<(), TriggerExecutorError> {
        let repo = self
            .agent_repo
            .as_ref()
            .ok_or_else(|| TriggerExecutorError::NotConfigured("agent_repo".to_string()))?;

        let msg_repo = self
            .message_repo
            .as_ref()
            .ok_or_else(|| TriggerExecutorError::NotConfigured("message_repo".to_string()))?;

        let source_agent_id = trigger_ctx
            .agent_id
            .ok_or_else(|| TriggerExecutorError::MessageError("No source agent ID".to_string()))?;

        let _ = repo
            .find_by_id(&source_agent_id)
            .await
            .map_err(|e| TriggerExecutorError::MessageError(e.to_string()))?
            .ok_or_else(|| {
                TriggerExecutorError::MessageError(format!(
                    "Source agent not found: {source_agent_id}"
                ))
            })?;

        let _ = repo
            .find_by_id(&target)
            .await
            .map_err(|e| TriggerExecutorError::MessageError(e.to_string()))?
            .ok_or_else(|| {
                TriggerExecutorError::MessageError(format!("Target agent not found: {target}"))
            })?;

        let message = neoco_core::messages::Message {
            id: neoco_core::ids::MessageId::new(),
            role: neoco_core::messages::Role::User,
            content: msg_content,
            tool_calls: None,
            tool_call_id: None,
            timestamp: chrono::Utc::now(),
            metadata: None,
        };

        msg_repo
            .append(&target, &message)
            .await
            .map_err(|e| TriggerExecutorError::MessageError(e.to_string()))?;

        Ok(())
    }

    /// Callback action
    #[allow(clippy::missing_docs_in_private_items)]
    async fn callback_action(
        &self,
        callback_id: &str,
        event: &Event,
    ) -> Result<(), TriggerExecutorError> {
        let callbacks = self.callbacks.lock().await;
        let callback = callbacks.get(callback_id).ok_or_else(|| {
            TriggerExecutorError::CallbackError(format!("Callback not found: {callback_id}"))
        })?;

        callback(event)
    }

    /// Log action
    #[allow(clippy::missing_docs_in_private_items)]
    fn log_action(
        &self,
        level: log::Level,
        message: &str,
        event: &Event,
        context: &TriggerContext,
    ) {
        let event_info = match event {
            Event::Agent(AgentEvent::Created { id, .. }) => {
                format!("Agent created: {id}")
            },
            Event::Agent(AgentEvent::Completed { id, output }) => {
                format!("Agent {id} completed: {output}")
            },
            Event::Agent(AgentEvent::Error { id, error }) => {
                format!("Agent {id} error: {error}")
            },
            Event::Agent(AgentEvent::ToolCalled { id, tool_id }) => {
                format!("Agent {id} called tool: {tool_id}")
            },
            _ => format!("{event:?}"),
        };

        let log_message = if context.agent_id.is_some() || context.session_id.is_some() {
            format!(
                "[Trigger] {message} | Event: {event_info} | Context: agent={:?}, session={:?}",
                context.agent_id, context.session_id
            )
        } else {
            format!("[Trigger] {message} | Event: {event_info}")
        };

        match level {
            log::Level::Error => tracing::error!("{log_message}"),
            log::Level::Warn => tracing::warn!("{log_message}"),
            log::Level::Info => tracing::info!("{log_message}"),
            log::Level::Debug => tracing::debug!("{log_message}"),
            log::Level::Trace => tracing::trace!("{log_message}"),
        }
    }

    /// Emit event action
    #[allow(clippy::missing_docs_in_private_items)]
    async fn emit_event_action(
        &self,
        event_type: String,
        payload: serde_json::Value,
    ) -> Result<(), TriggerExecutorError> {
        let publisher = self
            .event_publisher
            .as_ref()
            .ok_or_else(|| TriggerExecutorError::NotConfigured("event_publisher".to_string()))?;

        let core_event =
            neoco_core::events::Event::System(neoco_core::events::SystemEvent::Error {
                source: format!("trigger:{event_type}"),
                message: payload.to_string(),
            });

        publisher.publish(core_event).await;

        Ok(())
    }
}

impl Default for TriggerExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl From<Result<String, TriggerExecutorError>> for TriggerResult {
    fn from(result: Result<String, TriggerExecutorError>) -> Self {
        match result {
            #[allow(clippy::ignored_unit_patterns)]
            Ok(_) => TriggerResult::Success,
            Err(e) => TriggerResult::Failure {
                message: e.to_string(),
            },
        }
    }
}

impl From<Result<(), TriggerExecutorError>> for TriggerResult {
    fn from(result: Result<(), TriggerExecutorError>) -> Self {
        match result {
            #[allow(clippy::ignored_unit_patterns)]
            Ok(()) => TriggerResult::Success,
            Err(e) => TriggerResult::Failure {
                message: e.to_string(),
            },
        }
    }
}

/// 事件触发器实现
///
/// 实现事件订阅者接口，用于监听和触发事件。
pub struct EventTrigger {
    /// 触发器 ID
    id: String,
    /// 触发器处理器
    handler: Arc<dyn TriggerHandlerTrait>,
    /// 是否启用
    enabled: bool,
}

impl EventTrigger {
    /// 创建新的事件触发器
    #[must_use]
    pub fn new(id: impl Into<String>, handler: Arc<dyn TriggerHandlerTrait>) -> Self {
        Self {
            id: id.into(),
            handler,
            enabled: true,
        }
    }

    /// 创建新的事件触发器（带初始启用状态）
    #[must_use]
    pub fn new_with_enabled(
        id: impl Into<String>,
        handler: Arc<dyn TriggerHandlerTrait>,
        enabled: bool,
    ) -> Self {
        Self {
            id: id.into(),
            handler,
            enabled,
        }
    }

    /// 获取触发器 ID
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// 启用触发器
    pub fn enable(&mut self) {
        self.enabled = true;
    }

    /// 禁用触发器
    pub fn disable(&mut self) {
        self.enabled = false;
    }

    /// 检查触发器是否启用
    #[must_use]
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
}

impl Clone for EventTrigger {
    fn clone(&self) -> Self {
        Self {
            id: self.id.clone(),
            handler: self.handler.clone(),
            enabled: self.enabled,
        }
    }
}

#[async_trait]
impl neoco_core::events::EventSubscriber for EventTrigger {
    async fn on_event(&self, event: neoco_core::events::Event) {
        if !self.enabled {
            return;
        }

        let domain_event: Event = match event {
            neoco_core::events::Event::Agent(ae) => Event::Agent(AgentEvent::from(ae)),
            neoco_core::events::Event::Session(se) => Event::Session(SessionEvent::from(se)),
            neoco_core::events::Event::Tool(te) => Event::Tool(ToolEvent::from(te)),
            neoco_core::events::Event::System(se) => Event::System(SystemEvent::from(se)),
        };

        if self.handler.pattern().matches(&domain_event) {
            let context = TriggerContext::new();
            self.handler.handle_trigger(&domain_event, &context).await;
        }
    }

    fn matches(&self, event: &neoco_core::events::Event) -> bool {
        let domain_event: Option<Event> = match event {
            neoco_core::events::Event::Agent(_) => Some(Event::Agent(AgentEvent::Created {
                id: AgentUlid::new_root(&SessionUlid::new()),
                parent_ulid: None,
            })),
            neoco_core::events::Event::Session(_) => None,
            neoco_core::events::Event::Tool(_) => None,
            neoco_core::events::Event::System(_) => None,
        };

        if let Some(de) = domain_event {
            return self.handler.pattern().matches(&de);
        }
        false
    }
}

impl From<neoco_core::events::ToolEvent> for ToolEvent {
    fn from(te: neoco_core::events::ToolEvent) -> Self {
        match te {
            neoco_core::events::ToolEvent::Registered { tool_id } => {
                ToolEvent::Registered { tool_id }
            },
            neoco_core::events::ToolEvent::Executing {
                tool_id,
                agent_ulid,
            } => ToolEvent::Executing {
                tool_id,
                agent_ulid,
            },
            neoco_core::events::ToolEvent::Executed {
                tool_id,
                agent_ulid,
                success,
            } => ToolEvent::Executed {
                tool_id,
                agent_ulid,
                success,
            },
            neoco_core::events::ToolEvent::Error { tool_id, error } => {
                ToolEvent::Error { tool_id, error }
            },
        }
    }
}

impl From<neoco_core::events::SessionEvent> for SessionEvent {
    fn from(se: neoco_core::events::SessionEvent) -> Self {
        match se {
            neoco_core::events::SessionEvent::Created { id, session_type } => {
                SessionEvent::Created {
                    id,
                    session_type: session_type.into(),
                }
            },
            neoco_core::events::SessionEvent::Updated { id } => SessionEvent::Updated { id },
            neoco_core::events::SessionEvent::Deleted { id } => SessionEvent::Deleted { id },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trigger_pattern_all() {
        let handler = TriggerHandler::new(
            "test".to_string(),
            TriggerPattern::All,
            TriggerAction::Log {
                level: log::Level::Info,
                message: "test".to_string(),
            },
        );

        let events = [
            Event::Agent(AgentEvent::Created {
                id: AgentUlid::new_root(&SessionUlid::new()),
                parent_ulid: None,
            }),
            Event::Session(SessionEvent::Created {
                id: SessionUlid::new(),
                session_type: SessionType::Direct {
                    initial_message: None,
                },
            }),
        ];

        for event in events {
            assert!(handler.matches(&event));
        }
    }

    #[test]
    fn test_trigger_pattern_agent_spawned() {
        let handler = TriggerHandler::new(
            "test".to_string(),
            TriggerPattern::AgentSpawned { agent_type: None },
            TriggerAction::Log {
                level: log::Level::Info,
                message: "test".to_string(),
            },
        );

        let event = Event::Agent(AgentEvent::Created {
            id: AgentUlid::new_root(&SessionUlid::new()),
            parent_ulid: None,
        });

        assert!(handler.matches(&event));
    }

    #[test]
    fn test_trigger_pattern_agent_terminated() {
        let handler = TriggerHandler::new(
            "test".to_string(),
            TriggerPattern::AgentTerminated,
            TriggerAction::Log {
                level: log::Level::Info,
                message: "test".to_string(),
            },
        );

        let completed = Event::Agent(AgentEvent::Completed {
            id: AgentUlid::new_root(&SessionUlid::new()),
            output: "done".to_string(),
        });
        let error = Event::Agent(AgentEvent::Error {
            id: AgentUlid::new_root(&SessionUlid::new()),
            error: "failed".to_string(),
        });

        assert!(handler.matches(&completed));
        assert!(handler.matches(&error));

        let created = Event::Agent(AgentEvent::Created {
            id: AgentUlid::new_root(&SessionUlid::new()),
            parent_ulid: None,
        });
        assert!(!handler.matches(&created));
    }

    #[test]
    fn test_trigger_pattern_system_keyword() {
        let handler = TriggerHandler::new(
            "test".to_string(),
            TriggerPattern::SystemKeyword {
                keywords: vec!["wrong".to_string(), "fail".to_string()],
            },
            TriggerAction::Log {
                level: log::Level::Info,
                message: "test".to_string(),
            },
        );

        let error_event = Event::Agent(AgentEvent::Error {
            id: AgentUlid::new_root(&SessionUlid::new()),
            error: "Something went wrong".to_string(),
        });

        assert!(handler.matches(&error_event));
    }

    #[test]
    fn test_trigger_pattern_content_match() {
        let handler = TriggerHandler::new(
            "test".to_string(),
            TriggerPattern::ContentMatch {
                pattern: r"(?i)(error|fail)".to_string(),
            },
            TriggerAction::Log {
                level: log::Level::Info,
                message: "test".to_string(),
            },
        );

        let error_event = Event::Agent(AgentEvent::Error {
            id: AgentUlid::new_root(&SessionUlid::new()),
            error: "Connection failed".to_string(),
        });

        assert!(handler.matches(&error_event));
    }

    #[test]
    fn test_trigger_executor_log_action() {
        let executor = TriggerExecutor::new();

        let event = Event::Agent(AgentEvent::Completed {
            id: AgentUlid::new_root(&SessionUlid::new()),
            output: "test output".to_string(),
        });

        let context = TriggerContext::new();

        let action = TriggerAction::Log {
            level: log::Level::Info,
            message: "Test log message".to_string(),
        };

        let runtime = tokio::runtime::Runtime::new().unwrap();
        let result = runtime.block_on(executor.execute_action(&action, &event, &context));

        assert!(result.is_success());
    }

    #[test]
    fn test_trigger_result_from_result() {
        let success: Result<String, TriggerExecutorError> = Ok("ok".to_string());
        let result: TriggerResult = success.into();
        assert!(result.is_success());

        let failure: Result<String, TriggerExecutorError> =
            Err(TriggerExecutorError::ToolError("error".to_string()));
        let result: TriggerResult = failure.into();
        assert!(!result.is_success());
    }

    #[test]
    fn test_trigger_handler_with_executor() {
        let executor = Arc::new(TriggerExecutor::new());

        let handler = TriggerHandler::with_executor(
            "test_handler".to_string(),
            TriggerPattern::All,
            TriggerAction::Log {
                level: log::Level::Debug,
                message: "Handler test".to_string(),
            },
            executor,
        );

        assert!(handler.enabled);
        assert_eq!(handler.id(), "test_handler");
    }

    #[test]
    fn test_trigger_handler_handle_trigger_disabled() {
        let mut handler = TriggerHandler::new(
            "test".to_string(),
            TriggerPattern::All,
            TriggerAction::Log {
                level: log::Level::Info,
                message: "test".to_string(),
            },
        );
        handler.enabled = false;

        let runtime = tokio::runtime::Runtime::new().unwrap();
        let event = Event::Agent(AgentEvent::Created {
            id: AgentUlid::new_root(&SessionUlid::new()),
            parent_ulid: None,
        });
        let context = TriggerContext::new();

        let result = runtime.block_on(handler.handle_trigger(&event, &context));

        assert!(matches!(result, TriggerResult::Skipped));
    }

    #[test]
    fn test_trigger_handler_handle_trigger_no_match() {
        let handler = TriggerHandler::new(
            "test".to_string(),
            TriggerPattern::AgentTerminated,
            TriggerAction::Log {
                level: log::Level::Info,
                message: "test".to_string(),
            },
        );

        let runtime = tokio::runtime::Runtime::new().unwrap();
        let event = Event::Agent(AgentEvent::Created {
            id: AgentUlid::new_root(&SessionUlid::new()),
            parent_ulid: None,
        });
        let context = TriggerContext::new();

        let result = runtime.block_on(handler.handle_trigger(&event, &context));

        assert!(matches!(result, TriggerResult::Skipped));
    }
}
