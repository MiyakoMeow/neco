//! # UI 错误类型
//!
//! 本模块定义 UI 层使用的所有错误类型。
//!
//! ## 主要类型
//!
//! - [`UiError`]：通用 UI 错误
//! - [`ApiError`]：API 相关错误

use neoco_config::ConfigError;
use neoco_core::AppError;
use neoco_session::SessionError;
use thiserror::Error;

/// UI 错误
///
/// 表示 UI 层可能遇到的所有错误类型。
#[derive(Debug, Error)]
pub enum UiError {
    /// IO 错误
    #[error("IO错误: {0}")]
    Io(#[source] std::io::Error),

    /// 终端错误
    #[error("终端错误: {0}")]
    Terminal(#[source] std::io::Error),

    /// 配置错误
    #[error("配置错误: {0}")]
    Config(#[source] ConfigError),

    /// 会话错误
    #[error("会话错误: {0}")]
    Session(#[source] SessionError),

    /// API 错误
    #[error("API错误: {0}")]
    Api(#[source] ApiError),

    /// 请求错误
    #[error("请求错误: {0}")]
    BadRequest(String),

    /// 内部错误
    #[error("内部错误: {0}")]
    Internal(String),
}

/// API 错误
///
/// 表示 HTTP API 可能返回的所有错误类型。
#[derive(Debug, Error)]
pub enum ApiError {
    /// Session 未找到
    #[error("Session未找到")]
    SessionNotFound,

    /// 未授权访问
    #[error("未授权访问: {0}")]
    Unauthorized(String),

    /// 无效请求
    #[error("无效请求: {0}")]
    BadRequest(String),

    /// 冲突
    #[error("冲突: {0}")]
    Conflict(String),

    /// 资源不存在
    #[error("资源不存在: {0}")]
    NotFound(String),

    /// 请求超时
    #[error("请求超时")]
    RequestTimeout,

    /// 内部错误
    #[error("内部错误: {0}")]
    Internal(String),

    /// 服务不可用
    #[error("服务不可用: {0}")]
    ServiceUnavailable(String),

    /// 网关错误
    #[error("网关错误: {0}")]
    BadGateway(String),
}

impl ApiError {
    /// 获取 HTTP 状态码
    ///
    /// 返回与此错误对应的 HTTP 状态码。
    #[must_use]
    pub fn status_code(&self) -> u16 {
        match self {
            ApiError::Unauthorized(_) => 401,
            ApiError::BadRequest(_) => 400,
            ApiError::Conflict(_) => 409,
            ApiError::SessionNotFound | ApiError::NotFound(_) => 404,
            ApiError::RequestTimeout => 408,
            ApiError::Internal(_) => 500,
            ApiError::ServiceUnavailable(_) => 503,
            ApiError::BadGateway(_) => 502,
        }
    }

    /// 获取错误代码
    ///
    /// 返回此错误的机器可读错误代码。
    #[must_use]
    pub fn error_code(&self) -> &'static str {
        match self {
            ApiError::SessionNotFound => "SESSION_NOT_FOUND",
            ApiError::Unauthorized(_) => "UNAUTHORIZED",
            ApiError::BadRequest(_) => "BAD_REQUEST",
            ApiError::Conflict(_) => "CONFLICT",
            ApiError::NotFound(_) => "NOT_FOUND",
            ApiError::RequestTimeout => "REQUEST_TIMEOUT",
            ApiError::Internal(_) => "INTERNAL_ERROR",
            ApiError::ServiceUnavailable(_) => "SERVICE_UNAVAILABLE",
            ApiError::BadGateway(_) => "BAD_GATEWAY",
        }
    }
}

impl From<UiError> for AppError {
    fn from(err: UiError) -> Self {
        AppError::Ui(err.to_string())
    }
}
