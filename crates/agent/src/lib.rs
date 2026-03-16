//! # `NeoCo` Agent 引擎
//!
//! 本 crate 提供了多 Agent 协作的引擎实现，采用分层结构和通信隔离的 `SubAgent` 模式。
//!
//! ## 核心组件
//!
//! - **`AgentEngine`**：Agent 执行引擎，负责运行 Agent、管理子 Agent 和处理工具调用
//! - **`AgentError`**：Agent 相关的错误类型定义
//! - **事件系统**：Agent 生命周期事件和触发器机制
//! - **Agent 间通信**：支持 Agent 之间的消息传递和进度汇报
//!
//! ## 架构特点
//!
//! - 分层结构：支持父子 Agent 关系，实现任务的分解和协作
//! - 通信隔离：Agent 之间的通信受权限控制，确保安全性
//! - 事件驱动：通过事件系统跟踪和响应 Agent 状态变化
//! - 工具集成：支持动态加载和使用工具来扩展 Agent 能力

#![allow(unused_crate_dependencies)]

pub mod engine;
pub mod error;
pub mod events;
pub mod inter_agent;
pub mod tools;

pub use engine::{
    AgentEngine, AgentEngineBuilder, AgentResult, SkillContent, SkillLoadState, UsageContext,
};
pub use error::AgentError;
pub use events::{
    AgentEvent, Condition, ErrorKind, Event as DomainEvent, EventTrigger, TriggerAction,
    TriggerActionExecutor, TriggerCallback, TriggerContext, TriggerExecutor, TriggerExecutorError,
    TriggerHandler, TriggerHandlerTrait, TriggerPattern, TriggerResult,
};
pub use inter_agent::{InterAgentMessage, MessageType, TaskPriority, TaskStatus};
