//! Tool error types.

// Re-export ToolError from core for compatibility
pub use neoco_core::errors::ToolError;

/// Tool error kinds for pattern matching.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolErrorKind {
    /// Invalid arguments.
    InvalidArgs,
    /// Execution failed.
    Execution,
    /// Timeout.
    Timeout,
    /// Permission denied.
    PermissionDenied,
    /// Resource not found.
    NotFound,
    /// Tool not found.
    NotFoundTool,
    /// Confirmation required.
    ConfirmationRequired,
    /// Serialization error.
    Serialization,
    /// Path validation failed.
    PathValidation,
    /// Security violation.
    SecurityViolation,
    /// Execution failed (non-IO).
    ExecutionFailed,
    /// Invalid parameters.
    InvalidParameters,
}

/// 将 `ToolError` 引用转换为 `ToolErrorKind`。
///
/// 此转换会丢弃错误的具体信息，只保留错误类型，用于模式匹配。
impl From<&ToolError> for ToolErrorKind {
    /// 根据工具错误的具体类型返回对应的错误种类。
    fn from(err: &ToolError) -> Self {
        match err {
            ToolError::InvalidArgs(_) => Self::InvalidArgs,
            ToolError::Execution(_) => Self::Execution,
            ToolError::Timeout => Self::Timeout,
            ToolError::PermissionDenied => Self::PermissionDenied,
            ToolError::NotFound => Self::NotFound,
            ToolError::NotFoundTool(_) => Self::NotFoundTool,
            ToolError::ConfirmationRequired => Self::ConfirmationRequired,
            ToolError::Serialization(_) => Self::Serialization,
            ToolError::PathValidation(_) => Self::PathValidation,
            ToolError::SecurityViolation(_) => Self::SecurityViolation,
            ToolError::ExecutionFailed(_) => Self::ExecutionFailed,
            ToolError::InvalidParameters(_) => Self::InvalidParameters,
        }
    }
}
