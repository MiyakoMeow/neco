//! `NeoCo` Configuration Management Module
//!
//! This module provides type-safe configuration structures and loading mechanisms
//! for the `NeoCo` project.

use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use tempfile as _;
use thiserror::Error;
use url::Url;
use zeroize::Zeroize;

pub mod loader;
pub mod merger;
pub mod paths;
pub mod source;
pub mod validator;

pub use chrono::Duration;
pub use loader::ConfigLoader;
pub use merger::ConfigMerger;
pub use paths::Paths;
pub use source::ConfigSource;
pub use validator::ConfigValidator;

/// 支持的配置文件名称列表
///
/// 按优先级顺序排列，配置加载时会依次查找这些文件名。
pub const CONFIG_FILE_NAMES: &[&str] = &["neoco.yaml", "neoco.toml"];

/// 配置目录及其描述
///
/// 元组格式为 `(目录路径, 描述)`，按优先级从高到低排列。
pub const CONFIG_DIRS: &[(&str, &str)] = &[
    (".neoco", "Project-specific configuration"),
    (".agents", "Project-level common configuration"),
    ("~/.config/neoco", "User main configuration"),
    ("~/.agents", "User common configuration (lowest priority)"),
];

/// 配置错误类型
///
/// 表示配置加载、解析或验证过程中可能出现的各种错误。
#[derive(Debug, Error)]
pub enum ConfigError {
    /// 配置文件未找到
    #[error("配置文件未找到: {0}")]
    FileNotFound(PathBuf),

    /// 配置解析错误
    #[error("解析错误: {0}")]
    ParseError(String),

    /// 配置验证失败
    #[error("验证失败: {0}")]
    ValidationError(String),

    /// 环境变量未找到
    #[error("环境变量未找到: {0}")]
    EnvVarNotFound(String),

    /// 没有可用的环境变量
    #[error("没有可用的环境变量")]
    NoEnvVarFound,

    /// 热重载失败
    #[error("热重载失败: {0}")]
    HotReloadFailed(String),

    /// IO 错误
    #[error("IO错误: {0}")]
    IoError(#[from] std::io::Error),

    /// 配置目录未找到
    #[error("配置目录未找到")]
    ConfigDirNotFound,

    /// 无效的 URL
    #[error("无效的URL: {0}")]
    InvalidUrl(String),
}

/// `NeoCo` 配置结构
///
/// 包含 `NeoCo` 系统的所有配置项，包括模型组、模型提供者、MCP 服务器和系统配置。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    /// 当前使用的模型引用（优先级高于 `model_group`）
    #[serde(default)]
    pub model: Option<String>,
    /// 当前使用的模型组名称（优先级低于 model）
    #[serde(default)]
    pub model_group: Option<String>,
    /// 模型组配置
    #[serde(default)]
    pub model_groups: ModelGroups,
    /// 模型提供者配置
    #[serde(default)]
    pub model_providers: ModelProviders,
    /// MCP 服务器配置
    #[serde(default)]
    pub mcp_servers: McpServers,
    /// 系统配置
    #[serde(default)]
    pub system: SystemConfig,
}

/// 模型组集合
///
/// 使用 `IndexMap` 保持插入顺序，提供类似 `HashMap` 的接口。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModelGroups(IndexMap<SmolStr, ModelGroup>);

impl ModelGroups {
    /// 创建一个新的空的模型组集合
    #[must_use]
    pub fn new() -> Self {
        Self(IndexMap::new())
    }

    /// 获取指定键的模型组
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&ModelGroup> {
        self.0.get(key)
    }

    /// 插入一个模型组
    ///
    /// 如果键已存在，返回旧值；否则返回 None。
    pub fn insert(&mut self, key: impl Into<SmolStr>, value: ModelGroup) -> Option<ModelGroup> {
        self.0.insert(key.into(), value)
    }

    /// 返回迭代器，遍历所有模型组
    pub fn iter(&self) -> impl Iterator<Item = (&SmolStr, &ModelGroup)> {
        self.0.iter()
    }

    /// 返回模型组数量
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// 检查模型组集合是否为空
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl std::ops::Deref for ModelGroups {
    type Target = IndexMap<SmolStr, ModelGroup>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<IndexMap<SmolStr, ModelGroup>> for ModelGroups {
    fn from(map: IndexMap<SmolStr, ModelGroup>) -> Self {
        Self(map)
    }
}

impl From<HashMap<String, ModelGroup>> for ModelGroups {
    fn from(map: HashMap<String, ModelGroup>) -> Self {
        Self(
            map.into_iter()
                .map(|(k, v)| (SmolStr::from(k), v))
                .collect(),
        )
    }
}

impl<'a> IntoIterator for &'a ModelGroups {
    type Item = (&'a SmolStr, &'a ModelGroup);
    type IntoIter = indexmap::map::Iter<'a, SmolStr, ModelGroup>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl IntoIterator for ModelGroups {
    type Item = (SmolStr, ModelGroup);
    type IntoIter = indexmap::map::IntoIter<SmolStr, ModelGroup>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

/// 模型组配置
///
/// 表示一组模型引用，可用于组织和管理相关的模型。
#[derive(Debug, Clone, Default, Serialize)]
pub struct ModelGroup {
    /// 模型引用列表
    #[serde(default)]
    pub models: Vec<ModelRef>,
    /// 追加的模型引用列表（不序列化）
    #[serde(default, skip_serializing)]
    pub append_models: Vec<ModelRef>,
}

impl<'de> Deserialize<'de> for ModelGroup {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize, Default)]
        struct ModelGroupIntermediate {
            #[serde(default)]
            models: Vec<ModelRefIntermediateValue>,
        }

        #[derive(Deserialize)]
        #[serde(untagged)]
        enum ModelRefIntermediateValue {
            String(String),
            Object(ModelRef),
        }

        let intermediate = ModelGroupIntermediate::deserialize(deserializer)?;

        let mut models = Vec::new();
        for item in intermediate.models {
            let model_ref = match item {
                ModelRefIntermediateValue::String(s) => {
                    let (provider, name_with_params) = s
                        .split_once('/')
                        .ok_or_else(|| serde::de::Error::custom("invalid model ref format: expected 'provider/name' or 'provider/name?params'"))?;

                    let (name, params) = if let Some((name, query)) =
                        name_with_params.split_once('?')
                    {
                        let params = parse_query_params(query).map_err(serde::de::Error::custom)?;
                        (SmolStr::from(name), params)
                    } else {
                        (SmolStr::from(name_with_params), IndexMap::new())
                    };

                    ModelRef {
                        provider: SmolStr::from(provider),
                        name,
                        params,
                    }
                },
                ModelRefIntermediateValue::Object(m) => m,
            };
            models.push(model_ref);
        }

        Ok(Self {
            models,
            append_models: Vec::new(),
        })
    }
}

/// 模型引用
///
/// 表示对特定提供者的模型的引用，包含提供者名称、模型名称和参数。
#[derive(Debug, Clone, Serialize)]
pub struct ModelRef {
    /// 模型提供者名称
    pub provider: SmolStr,
    /// 模型名称
    pub name: SmolStr,
    /// 模型参数
    #[serde(default)]
    pub params: IndexMap<SmolStr, serde_json::Value>,
}

impl<'de> Deserialize<'de> for ModelRef {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum ModelRefIntermediate {
            String(String),
            Object {
                provider: String,
                name: String,
                #[serde(default)]
                params: HashMap<String, serde_json::Value>,
            },
        }

        let intermediate = ModelRefIntermediate::deserialize(deserializer)?;

        match intermediate {
            ModelRefIntermediate::String(s) => {
                let (provider, name_with_params) = s
                    .split_once('/')
                    .ok_or_else(|| serde::de::Error::custom("invalid model ref format: expected 'provider/name' or 'provider/name?params'"))?;

                let (name, params) = if let Some((name, query)) = name_with_params.split_once('?') {
                    let params = parse_query_params(query).map_err(serde::de::Error::custom)?;
                    (SmolStr::from(name), params)
                } else {
                    (SmolStr::from(name_with_params), IndexMap::new())
                };

                Ok(Self {
                    provider: SmolStr::from(provider),
                    name,
                    params,
                })
            },
            ModelRefIntermediate::Object {
                provider,
                name,
                params,
            } => Ok(Self {
                provider: SmolStr::from(provider),
                name: SmolStr::from(name),
                params: params
                    .into_iter()
                    .map(|(k, v)| (SmolStr::from(k), v))
                    .collect(),
            }),
        }
    }
}

/// Parse query parameters from a URL query string.
fn parse_query_params(query: &str) -> Result<IndexMap<SmolStr, serde_json::Value>, String> {
    let mut params = IndexMap::new();
    for pair in query.split('&') {
        let (key, value) = pair
            .split_once('=')
            .ok_or_else(|| format!("invalid query param: {pair}"))?;
        params.insert(
            SmolStr::from(key),
            serde_json::Value::String(value.to_string()),
        );
    }
    Ok(params)
}

/// 模型提供者集合
///
/// 使用 `IndexMap` 保持插入顺序，提供类似 `HashMap` 的接口。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModelProviders(IndexMap<SmolStr, ModelProvider>);

impl ModelProviders {
    /// 创建一个新的空的模型提供者集合
    #[must_use]
    pub fn new() -> Self {
        Self(IndexMap::new())
    }

    /// 获取指定键的模型提供者
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&ModelProvider> {
        self.0.get(key)
    }

    /// 插入一个模型提供者
    ///
    /// 如果键已存在，返回旧值；否则返回 None。
    pub fn insert(
        &mut self,
        key: impl Into<SmolStr>,
        value: ModelProvider,
    ) -> Option<ModelProvider> {
        self.0.insert(key.into(), value)
    }

    /// 返回迭代器，遍历所有模型提供者
    pub fn iter(&self) -> impl Iterator<Item = (&SmolStr, &ModelProvider)> {
        self.0.iter()
    }

    /// 返回模型提供者数量
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// 检查模型提供者集合是否为空
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl std::ops::Deref for ModelProviders {
    type Target = IndexMap<SmolStr, ModelProvider>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<IndexMap<SmolStr, ModelProvider>> for ModelProviders {
    fn from(map: IndexMap<SmolStr, ModelProvider>) -> Self {
        Self(map)
    }
}

impl From<HashMap<String, ModelProvider>> for ModelProviders {
    fn from(map: HashMap<String, ModelProvider>) -> Self {
        Self(
            map.into_iter()
                .map(|(k, v)| (SmolStr::from(k), v))
                .collect(),
        )
    }
}

impl<'a> IntoIterator for &'a ModelProviders {
    type Item = (&'a SmolStr, &'a ModelProvider);
    type IntoIter = indexmap::map::Iter<'a, SmolStr, ModelProvider>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl IntoIterator for ModelProviders {
    type Item = (SmolStr, ModelProvider);
    type IntoIter = indexmap::map::IntoIter<SmolStr, ModelProvider>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

/// 模型提供者配置
///
/// 表示一个模型提供者的完整配置，包括认证、重试策略和超时设置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelProvider {
    /// 提供者类型（默认 openai）
    #[serde(rename = "type", default)]
    pub provider_type: ProviderType,
    /// 提供者名称（可选，默认使用 provider key）
    #[serde(default)]
    pub name: Option<SmolStr>,
    /// 基础 URL（必填）
    pub base_url: Url,
    /// API 密钥环境变量名称（单个）
    #[serde(default)]
    pub api_key_env: Option<SmolStr>,
    /// API 密钥环境变量名称列表（多个，按优先级）
    #[serde(default)]
    pub api_key_envs: Option<Vec<SmolStr>>,
    /// API 密钥
    #[serde(default)]
    pub api_key: Option<SecretString>,
    /// 默认参数
    #[serde(default)]
    pub default_params: IndexMap<SmolStr, serde_json::Value>,
    /// 重试配置
    #[serde(default)]
    pub retry: RetryConfig,
    /// 超时时间（默认 60 秒）
    #[serde(default, deserialize_with = "deserialize_duration")]
    pub timeout: Duration,
}

impl ModelProvider {
    /// 获取 provider 名称，如果没有配置则返回 key
    #[must_use]
    pub fn name_or<'a>(&'a self, key: &'a str) -> Cow<'a, str> {
        self.name
            .as_ref()
            .map_or(Cow::Borrowed(key), |n| Cow::Borrowed(n.as_str()))
    }
}

/// Deserialize duration from seconds.
fn deserialize_duration<'de, D>(deserializer: D) -> Result<Duration, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let seconds = i64::deserialize(deserializer)?;
    Ok(Duration::seconds(seconds))
}

/// 模型提供者类型
///
/// 支持的模型提供者类型枚举。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ProviderType {
    /// `OpenAI` 兼容 API
    #[serde(rename = "openai")]
    #[default]
    OpenAI,
    /// Anthropic API
    #[serde(rename = "anthropic")]
    Anthropic,
    /// `OpenRouter` API
    #[serde(rename = "openrouter")]
    OpenRouter,
}

/// 重试配置
///
/// 定义请求失败时的重试策略，包括最大重试次数、退避时间和退避倍数。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    /// 最大重试次数
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    /// 初始退避时间
    #[serde(default)]
    pub initial_backoff: Duration,
    /// 退避时间倍数
    #[serde(default = "default_backoff_multiplier")]
    pub backoff_multiplier: f64,
    /// 最大退避时间
    #[serde(default)]
    pub max_backoff: Duration,
}

/// Default value for max retries (3).
fn default_max_retries() -> u32 {
    3
}

/// Default value for backoff multiplier (2.0).
fn default_backoff_multiplier() -> f64 {
    2.0
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_backoff: Duration::seconds(1),
            backoff_multiplier: 2.0,
            max_backoff: Duration::seconds(4),
        }
    }
}

/// MCP 服务器集合
///
/// 使用 `IndexMap` 保持插入顺序，提供类似 `HashMap` 的接口。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct McpServers(IndexMap<SmolStr, McpServerConfig>);

impl McpServers {
    /// 创建一个新的空的 MCP 服务器集合
    #[must_use]
    pub fn new() -> Self {
        Self(IndexMap::new())
    }

    /// 获取指定键的 MCP 服务器配置
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&McpServerConfig> {
        self.0.get(key)
    }

    /// 插入一个 MCP 服务器配置
    ///
    /// 如果键已存在，返回旧值；否则返回 None。
    pub fn insert(
        &mut self,
        key: impl Into<SmolStr>,
        value: McpServerConfig,
    ) -> Option<McpServerConfig> {
        self.0.insert(key.into(), value)
    }

    /// 返回迭代器，遍历所有 MCP 服务器配置
    pub fn iter(&self) -> impl Iterator<Item = (&SmolStr, &McpServerConfig)> {
        self.0.iter()
    }

    /// 返回 MCP 服务器数量
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// 检查 MCP 服务器集合是否为空
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl std::ops::Deref for McpServers {
    type Target = IndexMap<SmolStr, McpServerConfig>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<IndexMap<SmolStr, McpServerConfig>> for McpServers {
    fn from(map: IndexMap<SmolStr, McpServerConfig>) -> Self {
        Self(map)
    }
}

impl From<HashMap<String, McpServerConfig>> for McpServers {
    fn from(map: HashMap<String, McpServerConfig>) -> Self {
        Self(
            map.into_iter()
                .map(|(k, v)| (SmolStr::from(k), v))
                .collect(),
        )
    }
}

impl<'a> IntoIterator for &'a McpServers {
    type Item = (&'a SmolStr, &'a McpServerConfig);
    type IntoIter = indexmap::map::Iter<'a, SmolStr, McpServerConfig>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl IntoIterator for McpServers {
    type Item = (SmolStr, McpServerConfig);
    type IntoIter = indexmap::map::IntoIter<SmolStr, McpServerConfig>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

/// MCP 服务器配置
///
/// 表示一个 MCP 服务器的完整配置，包括命令、参数、URL 和认证信息。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct McpServerConfig {
    /// 服务器命令（可选）
    #[serde(default)]
    pub command: Option<SmolStr>,
    /// 命令参数（可选）
    #[serde(default)]
    pub args: Option<Vec<SmolStr>>,
    /// 服务器 URL（可选）
    #[serde(default)]
    pub url: Option<Url>,
    /// Bearer Token 环境变量名称（可选）
    #[serde(default)]
    pub bearer_token_env: Option<SmolStr>,
    /// HTTP 请求头
    #[serde(default)]
    pub http_headers: IndexMap<SmolStr, SmolStr>,
    /// 环境变量
    #[serde(default)]
    pub env: IndexMap<SmolStr, SmolStr>,
    /// 传输层配置（可选）
    #[serde(default)]
    pub transport: Option<McpTransport>,
}

/// MCP 传输层配置
///
/// 定义 MCP 服务器的传输层细节，包括传输类型、命令、URL 和认证信息。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct McpTransport {
    /// 传输类型
    #[serde(rename = "type", default)]
    pub transport_type: McpTransportType,
    /// 命令（可选）
    #[serde(default)]
    pub command: Option<SmolStr>,
    /// 命令参数（可选）
    #[serde(default)]
    pub args: Option<Vec<SmolStr>>,
    /// URL（可选）
    #[serde(default)]
    pub url: Option<Url>,
    /// HTTP 请求头
    #[serde(default)]
    pub headers: IndexMap<SmolStr, SmolStr>,
    /// Bearer Token（可选）
    #[serde(default)]
    pub bearer_token: Option<SecretString>,
}

/// MCP 传输类型
///
/// 支持的 MCP 传输层类型。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum McpTransportType {
    /// 标准输入输出传输
    #[default]
    Stdio,
    /// HTTP 传输
    Http,
}

/// 系统配置
///
/// 包含 `NeoCo` 系统的核心配置，包括存储、上下文、工具和 UI 配置。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SystemConfig {
    /// 存储配置
    pub storage: StorageConfig,
    /// 上下文配置
    pub context: ContextConfig,
    /// 工具配置
    pub tools: ToolsConfig,
    /// UI 配置
    pub ui: UiConfig,
}

/// 存储配置
///
/// 定义会话存储的相关设置，包括会话目录和压缩选项。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// 会话目录路径
    #[serde(default = "default_session_dir")]
    pub session_dir: PathBuf,
    /// 是否启用压缩
    #[serde(default = "default_compression")]
    pub compression: bool,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            session_dir: default_session_dir(),
            compression: default_compression(),
        }
    }
}

/// Default session directory path.
///
/// Uses `XDG_DATA_HOME` standard: `~/.local/share/neoco` (Linux) / `%APPDATA%/neoco` (Windows)
fn default_session_dir() -> PathBuf {
    Paths::new().session_dir
}

/// Default compression enabled (true).
fn default_compression() -> bool {
    true
}

/// 上下文配置
///
/// 定义上下文管理的相关设置，包括自动压缩阈值和保留消息数量。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextConfig {
    /// 自动压缩阈值（0-1 之间的值）
    #[serde(default = "default_compact_threshold")]
    pub auto_compact_threshold: f64,
    /// 是否启用自动压缩
    #[serde(default = "default_true")]
    pub auto_compact_enabled: bool,
    /// 用于压缩的模型组名称
    #[serde(default = "default_compact_model_group")]
    pub compact_model_group: String,
    /// 保留的最近消息数量
    #[serde(default = "default_keep_recent_messages")]
    pub keep_recent_messages: usize,
}

impl Default for ContextConfig {
    fn default() -> Self {
        Self {
            auto_compact_threshold: default_compact_threshold(),
            auto_compact_enabled: default_true(),
            compact_model_group: default_compact_model_group(),
            keep_recent_messages: default_keep_recent_messages(),
        }
    }
}

/// Default compact threshold (0.9).
fn default_compact_threshold() -> f64 {
    0.9
}

/// Default value for boolean (true).
fn default_true() -> bool {
    true
}

/// Default compact model group ("fast").
fn default_compact_model_group() -> String {
    "fast".to_string()
}

/// Default keep recent messages count (10).
fn default_keep_recent_messages() -> usize {
    10
}

/// 工具配置
///
/// 定义工具执行的相关设置，包括超时时间。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolsConfig {
    /// 各个工具的超时时间映射
    #[serde(default)]
    pub timeouts: IndexMap<SmolStr, Duration>,
    /// 默认超时时间
    #[serde(
        default = "default_tool_timeout",
        deserialize_with = "deserialize_duration"
    )]
    pub default_timeout: Duration,
}

impl Default for ToolsConfig {
    fn default() -> Self {
        Self {
            timeouts: IndexMap::new(),
            default_timeout: default_tool_timeout(),
        }
    }
}

/// Default tool timeout (30 seconds).
fn default_tool_timeout() -> Duration {
    Duration::seconds(30)
}

/// UI 配置
///
/// 定义用户界面的相关设置，包括默认运行模式。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UiConfig {
    /// 默认运行模式
    #[serde(default)]
    pub default_mode: RunMode,
}

/// 运行模式
///
/// 定义 `NeoCo` 的运行模式。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RunMode {
    /// 终端用户界面模式（默认）
    #[default]
    Tui,
    /// 命令行界面模式
    Cli,
}

/// 秘密字符串
///
/// 用于存储敏感信息（如 API 密钥），在序列化和调试时会自动隐藏内容。
#[derive(Clone, Zeroize, PartialEq, Eq)]
pub struct SecretString(String);

impl SecretString {
    /// 创建一个新的秘密字符串
    #[must_use]
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    /// 获取字符串的引用
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Load secret from environment variable.
    ///
    /// # Errors
    ///
    /// Returns an error if the environment variable is not found.
    pub fn from_env(name: &str) -> Result<Self, ConfigError> {
        std::env::var(name)
            .map(SecretString::new)
            .map_err(|_| ConfigError::EnvVarNotFound(name.to_string()))
    }

    /// Resolve the API key value.
    ///
    /// Returns the underlying string value.
    #[must_use]
    pub fn resolve(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SecretString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SecretString([REDACTED])")
    }
}

impl Serialize for SecretString {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str("[REDACTED]")
    }
}

impl<'de> Deserialize<'de> for SecretString {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct SecretStringInner {
            source: String,
            name: Option<String>,
        }

        let inner = SecretStringInner::deserialize(deserializer)?;

        match inner.source.as_str() {
            "env" => {
                let env_name = inner
                    .name
                    .ok_or_else(|| serde::de::Error::custom("env source requires 'name' field"))?;
                std::env::var(&env_name)
                    .map(SecretString::new)
                    .map_err(|_| serde::de::Error::custom(format!("env var not found: {env_name}")))
            },
            "env_list" => {
                let names = inner.name.ok_or_else(|| {
                    serde::de::Error::custom("env_list source requires 'name' field")
                })?;
                let names: Vec<String> = names.split(',').map(|s| s.trim().to_string()).collect();

                for name in &names {
                    if let Ok(value) = std::env::var(name) {
                        return Ok(SecretString::new(value));
                    }
                }
                Err(serde::de::Error::custom(format!(
                    "none of the env vars found: {names:?}"
                )))
            },
            "literal" => {
                let value = inner.name.ok_or_else(|| {
                    serde::de::Error::custom("literal source requires 'name' field")
                })?;
                Ok(SecretString::new(value))
            },
            _ => Err(serde::de::Error::custom(format!(
                "unknown secret source: {}",
                inner.source
            ))),
        }
    }
}

/// 内置模型提供者配置
///
/// 从 assets/providers.toml 加载内置的模型提供者配置
pub fn builtin_providers() -> ModelProviders {
    static PROVIDERS_ONCE: std::sync::OnceLock<ModelProviders> = std::sync::OnceLock::new();

    PROVIDERS_ONCE
        .get_or_init(|| {
            let content = include_str!("../../../assets/providers.toml");
            toml::from_str(content).unwrap_or_else(|e| {
                eprintln!("[WARN] Failed to parse builtin providers: {e}");
                ModelProviders::new()
            })
        })
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtin_providers() {
        let providers = builtin_providers();
        assert!(providers.contains_key("minimax-cn"));
        assert!(providers.contains_key("deepseek"));
        assert!(providers.contains_key("zhipuai-coding-plan"));

        let minimax = providers.get("minimax-cn").unwrap();
        assert_eq!(minimax.name.as_deref(), Some("MiniMax"));
        assert_eq!(minimax.api_key_env, Some("MINIMAX_API_KEY".into()));
    }

    #[test]
    fn test_secret_string_debug() {
        let secret = SecretString::new("my-api-key");
        let debug_str = format!("{secret:?}");
        assert_eq!(debug_str, "SecretString([REDACTED])");
    }

    #[test]
    fn test_secret_string_serialize() {
        let secret = SecretString::new("my-api-key");
        let serialized = serde_json::to_string(&secret).unwrap();
        assert_eq!(serialized, "\"[REDACTED]\"");
    }

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert!(config.model_groups.is_empty());
        assert!(config.model_providers.is_empty());
        assert!(config.mcp_servers.is_empty());
    }

    #[test]
    fn test_model_groups_deref() {
        let mut groups = ModelGroups::new();
        groups.insert(
            SmolStr::from("test"),
            ModelGroup {
                models: vec![ModelRef {
                    provider: SmolStr::from("openai"),
                    name: SmolStr::from("gpt-4"),
                    params: IndexMap::new(),
                }],
                ..Default::default()
            },
        );

        assert!(groups.contains_key("test"));
        assert_eq!(groups.len(), 1);
    }

    #[test]
    fn test_retry_config_defaults() {
        let retry = RetryConfig::default();
        assert_eq!(retry.max_retries, 3);
        assert!((retry.backoff_multiplier - 2.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_system_config_defaults() {
        let system = SystemConfig::default();
        assert!(system.storage.compression);
        assert_eq!(system.context.compact_model_group, "fast");
        assert_eq!(system.tools.default_timeout, Duration::seconds(30));
        assert_eq!(system.ui.default_mode, RunMode::Tui);
    }

    #[test]
    fn test_run_mode_serialization() {
        assert_eq!(serde_json::to_string(&RunMode::Tui).unwrap(), "\"tui\"");
        assert_eq!(serde_json::to_string(&RunMode::Cli).unwrap(), "\"cli\"");
    }

    #[test]
    fn test_config_file_names() {
        assert!(CONFIG_FILE_NAMES.contains(&"neoco.yaml"));
        assert!(CONFIG_FILE_NAMES.contains(&"neoco.toml"));
    }

    #[test]
    fn test_provider_type() {
        assert_eq!(ProviderType::OpenAI, ProviderType::default());
    }

    #[test]
    fn test_parse_neoco_toml_format() {
        let toml_content = r#"
model = "minimax-cn/MiniMax-M2.5?temperature=0.1"
model_group = "balanced"

[model_groups.smart]
models = ["deepseek/deepseek-chat"]

[model_groups.fast]
models = ["zhipuai-coding-plan/glm-4.7-flashx?temperature=0.1"]

[model_groups.balanced]
models = ["minimax-cn/MiniMax-M2.5"]

[model_providers.zhipuai-coding-plan]
type = "openai"
name = "ZhipuAI Coding Plan"
base_url = "https://open.bigmodel.cn/api/coding/paas/v4"
api_key_env = "ZHIPU_API_KEY"

[model_providers.minimax-cn]
type = "openai"
name = "MiniMax"
base_url = "https://api.minimaxi.com/v1"
api_key_env = "MINIMAX_API_KEY"

[model_providers.deepseek]
type = "openai"
name = "DeepSeek"
base_url = "https://api.deepseek.com/v1"
api_key_env = "DEEPSEEK_API_KEY"
"#;

        let config: Config = toml::from_str(toml_content).expect("failed to parse config");

        assert_eq!(
            config.model,
            Some("minimax-cn/MiniMax-M2.5?temperature=0.1".to_string())
        );
        assert_eq!(config.model_group, Some("balanced".to_string()));

        let smart = config
            .model_groups
            .get("smart")
            .expect("smart group not found");
        assert_eq!(smart.models.len(), 1);
        let smart_model = smart.models.first().unwrap();
        assert_eq!(smart_model.provider, "deepseek");
        assert_eq!(smart_model.name, "deepseek-chat");
        assert!(smart_model.params.is_empty());

        let fast = config
            .model_groups
            .get("fast")
            .expect("fast group not found");
        assert_eq!(fast.models.len(), 1);
        let fast_model = fast.models.first().unwrap();
        assert_eq!(fast_model.provider, "zhipuai-coding-plan");
        assert_eq!(fast_model.name, "glm-4.7-flashx");
        assert_eq!(
            fast_model
                .params
                .get("temperature")
                .and_then(|v| v.as_str()),
            Some("0.1")
        );

        let balanced = config
            .model_groups
            .get("balanced")
            .expect("balanced group not found");
        assert_eq!(balanced.models.len(), 1);
        let balanced_model = balanced.models.first().unwrap();
        assert_eq!(balanced_model.provider, "minimax-cn");
        assert_eq!(balanced_model.name, "MiniMax-M2.5");

        assert!(config.model_providers.get("deepseek").is_some());
        assert!(config.model_providers.get("minimax-cn").is_some());
        assert!(config.model_providers.get("zhipuai-coding-plan").is_some());
    }

    #[test]
    fn test_model_ref_string_deserialization() {
        let toml_content = r#"
[model_groups.test]
models = [
    "provider1/model-a",
    "provider2/model-b?temperature=0.5&max_tokens=100",
    { provider = "provider3", name = "model-c", params = { top_p = 0.9 } }
]
"#;

        let config: Config = toml::from_str(toml_content).expect("failed to parse config");
        let test_group = config
            .model_groups
            .get("test")
            .expect("test group not found");

        assert_eq!(test_group.models.len(), 3);

        let model_0 = test_group.models.first().unwrap();
        assert_eq!(model_0.provider, "provider1");
        assert_eq!(model_0.name, "model-a");
        assert!(model_0.params.is_empty());

        let model_1 = test_group.models.get(1).unwrap();
        assert_eq!(model_1.provider, "provider2");
        assert_eq!(model_1.name, "model-b");
        assert_eq!(
            model_1.params.get("temperature").and_then(|v| v.as_str()),
            Some("0.5")
        );
        assert_eq!(
            model_1.params.get("max_tokens").and_then(|v| v.as_str()),
            Some("100")
        );

        let model_2 = test_group.models.get(2).unwrap();
        assert_eq!(model_2.provider, "provider3");
        assert_eq!(model_2.name, "model-c");
        assert_eq!(
            model_2
                .params
                .get("top_p")
                .and_then(serde_json::Value::as_f64),
            Some(0.9)
        );
    }
}
