//! # UI 特征模块
//!
//! 本模块定义用户界面的抽象特征和数据类型。
//!
//! ## 主要类型
//!
//! - [`UserInterface`]：用户界面抽象特征
//! - [`UserInput`]：用户输入类型
//! - [`AgentOutput`]：Agent 输出类型
//! - [`OutputType`]：输出类型枚举

use crate::error::UiError;
use async_trait::async_trait;

/// 用户界面抽象特征
///
/// 定义所有用户界面实现必须遵循的接口。
#[async_trait]
pub trait UserInterface: Send + Sync {
    /// 初始化用户界面
    async fn init(&mut self) -> Result<(), UiError>;

    /// 获取用户输入
    async fn get_input(&mut self) -> Result<UserInput, UiError>;

    /// 渲染 Agent 输出
    async fn render(&mut self, output: &AgentOutput) -> Result<(), UiError>;

    /// 向用户提问并获取回答
    async fn ask(
        &mut self,
        question: &str,
        options: Option<Vec<String>>,
    ) -> Result<String, UiError>;

    /// 关闭用户界面
    async fn shutdown(&mut self) -> Result<(), UiError>;
}

/// 用户输入
///
/// 表示用户可能输入的各种类型。
#[derive(Debug, Clone)]
pub enum UserInput {
    /// 消息
    Message(String),
    /// 命令
    Command {
        /// 命令名称
        name: String,
        /// 命令参数
        args: Vec<String>,
    },
    /// 退出
    Exit,
    /// 中断
    Interrupt,
}

/// Agent 输出
///
/// 表示 Agent 返回的输出内容和类型。
#[derive(Debug, Clone)]
pub struct AgentOutput {
    /// 输出内容
    pub content: String,
    /// 输出类型
    pub output_type: OutputType,
}

/// 输出类型
///
/// 定义各种可能的输出类型。
#[derive(Debug, Clone)]
pub enum OutputType {
    /// 纯文本
    Text,
    /// Markdown 格式
    Markdown,
    /// 代码块
    Code {
        /// 编程语言名称
        language: String,
    },
    /// 工具执行结果
    ToolResult {
        /// 工具名称
        tool_name: String,
    },
    /// 错误
    Error,
}
