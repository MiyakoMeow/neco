//! Agent 间通信类型
//!
//! 定义了 Agent 之间通信的消息结构和类型。

use chrono::{DateTime, Utc};
use neoco_core::ids::AgentUlid;
use serde::{Deserialize, Serialize};

/// Agent 间消息
///
/// 表示 Agent 之间传递的消息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterAgentMessage {
    /// 消息 ID
    pub id: String,
    /// 发送方 Agent ID
    pub from: AgentUlid,
    /// 接收方 Agent ID
    pub to: AgentUlid,
    /// 消息类型
    pub message_type: MessageType,
    /// 消息内容
    pub content: String,
    /// 时间戳
    pub timestamp: DateTime<Utc>,
    /// 是否需要响应
    pub requires_response: bool,
}

impl InterAgentMessage {
    /// 创建新的 Agent 间消息
    #[must_use]
    pub fn new(from: AgentUlid, to: AgentUlid, message_type: MessageType, content: String) -> Self {
        Self {
            id: ulid::Ulid::new().to_string(),
            from,
            to,
            message_type,
            content,
            timestamp: Utc::now(),
            requires_response: false,
        }
    }

    /// 设置是否需要响应
    #[must_use]
    pub fn with_response(mut self, requires_response: bool) -> Self {
        self.requires_response = requires_response;
        self
    }
}

/// 消息类型
///
/// 表示 Agent 间通信的不同消息类型。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum MessageType {
    /// 任务分配
    TaskAssignment {
        /// 任务 ID
        task_id: String,
        /// 任务优先级
        priority: TaskPriority,
        /// 截止时间（可选）
        deadline: Option<DateTime<Utc>>,
    },
    /// 进度汇报
    ProgressReport {
        /// 任务 ID
        task_id: String,
        /// 进度（0.0-1.0）
        progress: f64,
        /// 任务状态
        status: TaskStatus,
    },
    /// 结果汇报
    ResultReport {
        /// 任务 ID
        task_id: String,
        /// 结果
        result: String,
        /// 是否成功
        success: bool,
    },
    /// 澄清请求
    ClarificationRequest {
        /// 问题
        question: String,
        /// 上下文
        context: String,
    },
    /// 一般消息
    General,
}

/// 任务优先级
///
/// 表示任务的优先级级别。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Serialize, Deserialize)]
pub enum TaskPriority {
    /// 低优先级
    Low,
    /// 普通优先级
    Normal,
    /// 高优先级
    High,
    /// 紧急优先级
    Critical,
}

/// 任务状态
///
/// 表示任务的当前状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskStatus {
    /// 待处理
    Pending,
    /// 进行中
    InProgress,
    /// 已阻塞
    Blocked,
    /// 已完成
    Completed,
    /// 已失败
    Failed,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inter_agent_message_creation() {
        let session = neoco_core::ids::SessionUlid::new();
        let from = AgentUlid::new_root(&session);
        let to = AgentUlid::new_child(&from);

        let msg = InterAgentMessage::new(from, to, MessageType::General, "Hello".to_string());

        assert_eq!(msg.from, from);
        assert_eq!(msg.to, to);
        assert!(matches!(msg.message_type, MessageType::General));
        assert_eq!(msg.content, "Hello");
    }

    #[test]
    fn test_inter_agent_message_with_response() {
        let session = neoco_core::ids::SessionUlid::new();
        let from = AgentUlid::new_root(&session);
        let to = AgentUlid::new_child(&from);

        let msg = InterAgentMessage::new(from, to, MessageType::General, "Hello".to_string())
            .with_response(true);

        assert!(msg.requires_response);
    }

    #[test]
    fn test_task_priority_ordering() {
        assert!(TaskPriority::Low < TaskPriority::Normal);
        assert!(TaskPriority::Normal < TaskPriority::High);
        assert!(TaskPriority::High < TaskPriority::Critical);
    }

    #[test]
    fn test_task_status() {
        let statuses = [
            TaskStatus::Pending,
            TaskStatus::InProgress,
            TaskStatus::Blocked,
            TaskStatus::Completed,
            TaskStatus::Failed,
        ];

        for status in statuses {
            let _ = format!("{status:?}");
        }
    }
}
