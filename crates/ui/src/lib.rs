#![allow(unused_crate_dependencies)]
//! # `NeoCo` 用户界面模块
//!
//! 本模块为 `NeoCo` 提供用户界面实现，包括：
//!
//! - **CLI 模式**：命令行界面，支持直接发送消息
//! - **TUI 模式**：终端用户界面，提供交互式终端体验
//! - **服务模块**：统一的服务初始化和依赖注入
//!
//! ## 模块结构
//!
//! - [`cli`]：命令行界面实现
//! - [`tui`]：终端用户界面实现
//! - [`service`]：服务初始化和依赖注入
//! - [`error`]：UI 相关的错误类型
//! - [`traits`]：用户界面抽象特征

pub mod cli;
pub mod error;
pub mod service;
pub mod traits;
pub mod tui;

pub use cli::{CliArgs, CliInterface};
pub use error::UiError;
pub use service::{ServiceContext, ServiceError, ServiceInitializer, StorageAdapter};
pub use traits::{AgentOutput, OutputType, UserInput, UserInterface};
