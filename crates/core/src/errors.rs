//! Unified error types for all modules.

use thiserror::Error;

/// Unified application error aggregating all module errors.
#[derive(Debug, Error)]
pub enum AppError {
    /// Session-related error.
    #[error("Session错误: {0}")]
    Session(#[from] SessionError),

    /// Agent-related error.
    #[error("Agent错误: {0}")]
    Agent(#[from] AgentError),

    /// Model-related error.
    #[error("模型错误: {0}")]
    Model(#[from] ModelError),

    /// Tool-related error.
    #[error("工具错误: {0}")]
    Tool(#[from] ToolError),

    /// Configuration error.
    #[error("配置错误: {0}")]
    Config(#[from] ConfigError),

    /// Storage error.
    #[error("存储错误: {0}")]
    Storage(#[from] StorageError),

    /// MCP-related error.
    #[error("MCP错误: {0}")]
    Mcp(#[from] McpError),

    /// Context-related error.
    #[error("上下文错误: {0}")]
    Context(#[from] ContextError),

    /// Skill-related error.
    #[error("Skill错误: {0}")]
    Skill(#[from] SkillError),

    /// Identifier error.
    #[error("ID错误: {0}")]
    Id(#[from] IdError),

    /// UI-related error.
    #[error("UI错误: {0}")]
    Ui(String),
}

impl AppError {
    /// Check if error is retryable.
    pub fn is_retryable(&self) -> bool {
        match self {
            Self::Session(e) => e.is_retryable(),
            Self::Agent(e) => e.is_retryable(),
            Self::Model(e) => e.is_retryable(),
            Self::Tool(e) => e.is_retryable(),
            Self::Config(_) => false,
            Self::Storage(e) => e.is_retryable(),
            Self::Mcp(e) => e.is_retryable(),
            Self::Context(_) | Self::Skill(_) | Self::Id(_) | Self::Ui(_) => false,
        }
    }

    /// Check if error is user-facing.
    pub fn is_user_facing(&self) -> bool {
        matches!(
            self,
            Self::Session(_) | Self::Agent(_) | Self::Config(_) | Self::Id(_) | Self::Ui(_)
        )
    }
}

/// Identifier-related errors.
#[derive(Debug, Error)]
pub enum IdError {
    /// Invalid ID format.
    #[error("ID格式错误: {0}")]
    InvalidFormat(String),

    /// ID type mismatch.
    #[error("ID类型不匹配: 期望 {expected}, 实际 {actual}")]
    TypeMismatch {
        /// Expected type.
        expected: &'static str,
        /// Actual type.
        actual: &'static str,
    },

    /// Failed to parse ID.
    #[error("无法解析ID: {input}, 原因: {reason}")]
    ParseError {
        /// Input string.
        input: String,
        /// Reason for failure.
        reason: String,
    },

    /// ID validation failed.
    #[error("ID验证失败: {0}")]
    ValidationFailed(String),

    /// ID is empty.
    #[error("ID不能为空")]
    Empty,

    /// ID not found.
    #[error("ID不存在: {0}")]
    NotFound(String),

    /// Failed to generate ID.
    #[error("ID生成失败: {0}")]
    GenerationFailed(String),
}

impl IdError {
    /// Check if error is retryable.
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::GenerationFailed(_))
    }
}

/// Session-related errors.
#[derive(Debug, Error)]
pub enum SessionError {
    /// Session does not exist.
    #[error("Session不存在: {0}")]
    NotFound(String),

    /// Session already exists.
    #[error("Session已存在: {0}")]
    AlreadyExists(String),

    /// Invalid session.
    #[error("Session无效: {0}")]
    Invalid(String),

    /// Failed to create session.
    #[error("Session创建失败: {0}")]
    CreationFailed(String),
}

impl SessionError {
    /// Check if error is retryable.
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::CreationFailed(_))
    }
}

/// Agent-related errors.
#[derive(Debug, Error)]
pub enum AgentError {
    /// Agent does not exist.
    #[error("Agent不存在: {0}")]
    NotFound(String),

    /// Agent already exists.
    #[error("Agent已存在: {0}")]
    AlreadyExists(String),

    /// Invalid agent.
    #[error("Agent无效: {0}")]
    Invalid(String),

    /// Invalid agent state.
    #[error("Agent状态错误: {0}")]
    InvalidState(String),
}

impl AgentError {
    /// Check if error is retryable.
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::InvalidState(_))
    }
}

/// Model-related errors.
#[derive(Debug, Error)]
pub enum ModelError {
    /// API error.
    #[error("API错误: {provider} - {message}")]
    Api {
        /// The provider name.
        provider: String,
        /// The error message.
        message: String,
    },

    /// Network error.
    #[error("网络错误: {0}")]
    Network(#[from] reqwest::Error),

    /// Rate limit error.
    #[error("速率限制: {0}")]
    RateLimit(String),

    /// Server error.
    #[error("服务器错误: {status} - {message}")]
    ServerError {
        /// HTTP status code.
        status: u16,
        /// Error message.
        message: String,
    },

    /// Client not found.
    #[error("客户端未找到: {0}")]
    ClientNotFound(String),

    /// All models failed.
    #[error("模型组 {group} 中所有模型都失败")]
    AllModelsFailed {
        /// The group name.
        group: String,
    },

    /// Configuration error.
    #[error("配置错误: {0}")]
    Config(String),

    /// Timeout error.
    #[error("超时")]
    Timeout,

    /// Serialization error.
    #[error("序列化错误: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Stream error.
    #[error("流式错误: {0}")]
    Stream(String),

    /// Model request failed.
    #[error("模型请求失败: {0}")]
    RequestFailed(String),

    /// Invalid model response.
    #[error("模型响应无效: {0}")]
    InvalidResponse(String),

    /// Model authentication failed.
    #[error("模型认证失败: {0}")]
    AuthenticationFailed(String),

    /// Model quota exceeded.
    #[error("模型配额不足: {0}")]
    QuotaExceeded(String),

    /// Unsupported model.
    #[error("不支持的模型: {0}")]
    UnsupportedModel(String),
}

impl ModelError {
    /// Checks if the error is retryable.
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::RateLimit(_)
                | Self::Timeout
                | Self::ServerError { .. }
                | Self::RequestFailed(_)
                | Self::Network(_)
        )
    }

    /// Creates an API error.
    pub fn api(provider: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Api {
            provider: provider.into(),
            message: message.into(),
        }
    }

    /// Creates a server error.
    pub fn server_error(status: u16, message: impl Into<String>) -> Self {
        Self::ServerError {
            status,
            message: message.into(),
        }
    }

    /// Creates a rate limit error.
    pub fn rate_limit(message: impl Into<String>) -> Self {
        Self::RateLimit(message.into())
    }
}

/// Tool-related errors.
#[derive(Debug, Error)]
pub enum ToolError {
    /// Invalid arguments.
    #[error("参数无效: {0}")]
    InvalidArgs(String),

    /// Execution failed.
    #[error("执行失败: {0}")]
    Execution(#[source] std::io::Error),

    /// Timeout.
    #[error("超时")]
    Timeout,

    /// Permission denied.
    #[error("权限不足")]
    PermissionDenied,

    /// Resource not found.
    #[error("资源未找到")]
    NotFound,

    /// Tool not found.
    #[error("工具未找到: {0}")]
    NotFoundTool(String),

    /// Confirmation required.
    #[error("需要确认")]
    ConfirmationRequired,

    /// Serialization error.
    #[error("序列化错误: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Path validation failed.
    #[error("路径验证失败: {0}")]
    PathValidation(String),

    /// Security violation.
    #[error("安全违规: {0}")]
    SecurityViolation(String),

    /// Execution failed with message (non-IO errors).
    #[error("执行失败: {0}")]
    ExecutionFailed(String),

    /// Invalid parameters.
    #[error("工具参数无效: {0}")]
    InvalidParameters(String),
}

impl ToolError {
    /// Check if error is retryable.
    pub fn is_retryable(&self) -> bool {
        match self {
            Self::Timeout => true,
            Self::Execution(e) => e.kind() == std::io::ErrorKind::NotFound,
            Self::ExecutionFailed(_) => false,
            _ => false,
        }
    }
}

/// Configuration-related errors.
#[derive(Debug, Error)]
pub enum ConfigError {
    /// Config file not found.
    #[error("配置文件不存在: {0}")]
    NotFound(String),

    /// Failed to parse config.
    #[error("配置解析失败: {0}")]
    ParseError(String),

    /// Invalid config.
    #[error("配置无效: {0}")]
    Invalid(String),
}

/// Storage-related errors.
#[derive(Debug, Error)]
pub enum StorageError {
    /// Storage operation failed.
    #[error("存储操作失败: {0}")]
    OperationFailed(String),

    /// Storage does not exist.
    #[error("存储不存在: {0}")]
    NotFound(String),

    /// Storage permission denied.
    #[error("存储权限错误: {0}")]
    PermissionDenied(String),
}

impl StorageError {
    /// Check if error is retryable.
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::OperationFailed(_))
    }
}

/// MCP-related errors.
#[derive(Debug, Error)]
pub enum McpError {
    /// MCP connection failed.
    #[error("MCP连接失败: {0}")]
    ConnectionFailed(String),

    /// MCP request failed.
    #[error("MCP请求失败: {0}")]
    RequestFailed(String),

    /// Invalid MCP response.
    #[error("MCP响应无效: {0}")]
    InvalidResponse(String),
}

impl McpError {
    /// Check if error is retryable.
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::ConnectionFailed(_) | Self::RequestFailed(_))
    }
}

/// Context-related errors.
#[derive(Debug, Error)]
pub enum ContextError {
    /// Context exceeds limit.
    #[error("上下文超限: {0}")]
    ExceedsLimit(String),

    /// Failed to compute context.
    #[error("上下文计算失败: {0}")]
    ComputationFailed(String),
}

/// Skill-related errors.
#[derive(Debug, Error)]
pub enum SkillError {
    /// Skill does not exist.
    #[error("Skill不存在: {0}")]
    NotFound(String),

    /// Failed to activate skill.
    #[error("Skill激活失败: {0}")]
    ActivationFailed(String),

    /// Failed to execute skill.
    #[error("Skill执行失败: {0}")]
    ExecutionFailed(String),
}

/// Routing-related errors.
#[derive(Debug, Error)]
pub enum RouteError {
    /// Route not found.
    #[error("路由不存在: {0}")]
    NotFound(String),

    /// Routing failed.
    #[error("路由失败: {0}")]
    Failed(String),
}
