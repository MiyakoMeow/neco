//! NeoCo 安全模块（最小化实现）
//!
//! 本模块仅保留工具执行所需的最小安全功能。
//! 完整的安全框架已被移除。

use serde::{Deserialize, Serialize};

/// 安全上下文（最小化版本）
///
/// 用于在工具执行时传递安全相关信息。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SecurityContext {
    /// 工具名称
    pub tool_name: Option<String>,
    /// 输入数据
    pub input: Option<String>,
    /// 会话标识符
    pub session_id: Option<String>,
    /// 用户标识符
    pub user_id: Option<String>,
    /// 动作名称
    pub action: Option<String>,
}

impl SecurityContext {
    /// 创建新的安全上下文
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置工具名称
    pub fn with_tool_name(mut self, tool_name: impl Into<String>) -> Self {
        self.tool_name = Some(tool_name.into());
        self
    }

    /// 设置输入数据
    pub fn with_input(mut self, input: impl Into<String>) -> Self {
        self.input = Some(input.into());
        self
    }

    /// 设置会话标识符
    pub fn with_session_id(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }

    /// 设置用户标识符
    pub fn with_user_id(mut self, user_id: impl Into<String>) -> Self {
        self.user_id = Some(user_id.into());
        self
    }

    /// 设置动作名称
    pub fn with_action(mut self, action: impl Into<String>) -> Self {
        self.action = Some(action.into());
        self
    }
}

/// 安全检查结果（简化版）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecurityResult {
    /// 允许访问
    Allowed,
    /// 拒绝访问
    Denied,
}

impl SecurityResult {
    /// 检查是否允许
    pub fn is_allowed(&self) -> bool {
        matches!(self, Self::Allowed)
    }

    /// 检查是否拒绝
    pub fn is_denied(&self) -> bool {
        matches!(self, Self::Denied)
    }
}

/// 安全管理器（最小化版本）
///
/// 不执行实际的安全检查，仅作为接口存在。
/// 实际的安全检查由工具执行层的PathValidator完成。
#[derive(Default)]
pub struct SecurityManager;

impl SecurityManager {
    /// 创建新的安全管理器
    pub fn new() -> Self {
        Self
    }

    /// 检查安全上下文
    ///
    /// 当前实现总是返回Allowed。
    /// 实际的安全检查由工具执行层的PathValidator完成。
    pub fn check(&self, _context: &SecurityContext) -> SecurityResult {
        SecurityResult::Allowed
    }
}
