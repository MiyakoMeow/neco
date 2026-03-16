//! Agent 错误类型
//!
//! 定义了 Agent 引擎相关的所有错误类型。

use thiserror::Error;

/// Agent 错误
///
/// 表示 Agent 操作过程中可能发生的各种错误。
#[derive(Debug, Error)]
pub enum AgentError {
    /// Agent 不存在
    #[error("Agent不存在: {0}")]
    NotFound(String),

    /// 父 Agent 不存在
    #[error("父Agent不存在")]
    ParentNotFound,

    /// Agent 定义未找到
    #[error("Agent定义未找到: {0}")]
    DefinitionNotFound(String),

    /// 提示词未找到
    #[error("提示词未找到: {0}")]
    PromptNotFound(String),

    /// 不能创建下级 Agent
    #[error("不能创建下级Agent")]
    CannotSpawnChildren,

    /// 已达到最大下级 Agent 数量
    #[error("已达到最大下级Agent数量")]
    MaxChildrenReached,

    /// 通信权限不足
    #[error("通信权限不足")]
    PermissionDenied,

    /// 没有上级 Agent
    #[error("没有上级Agent")]
    NoParentAgent,

    /// 模型调用错误
    #[error("模型调用错误: {0}")]
    Model(String),

    /// 工具错误
    #[error("工具错误: {0}")]
    Tool(String),

    /// 配置错误
    #[error("配置错误: {0}")]
    Config(String),

    /// 超时
    #[error("超时")]
    Timeout,

    /// 通道已关闭
    #[error("通道已关闭")]
    ChannelClosed,

    /// Session 不存在
    #[error("Session不存在: {0}")]
    SessionNotFound(String),

    /// 无效的状态转换
    #[error("无效的状态转换: {0}")]
    InvalidStateTransition(String),

    /// 无效的参数
    #[error("无效的参数: {0}")]
    InvalidArgument(String),
}

impl AgentError {
    /// 判断错误是否可恢复
    ///
    /// # Returns
    ///
    /// 如果错误是可恢复的（如超时、通道关闭等），返回 `true`。
    #[must_use]
    pub fn is_recoverable(&self) -> bool {
        matches!(
            self,
            Self::Timeout | Self::ChannelClosed | Self::Model(_) | Self::Tool(_)
        )
    }
}

impl From<neoco_core::ModelError> for AgentError {
    fn from(e: neoco_core::ModelError) -> Self {
        Self::Model(e.to_string())
    }
}

impl From<neoco_core::ToolError> for AgentError {
    fn from(e: neoco_core::ToolError) -> Self {
        Self::Tool(e.to_string())
    }
}
