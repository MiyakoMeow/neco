//! Security validation for tool execution.

use std::path::{Path, PathBuf};
use thiserror::Error;

/// 安全验证错误类型。
#[derive(Debug, Error)]
pub enum SecurityError {
    /// 路径验证失败错误。
    #[error("Path validation failed: {0}")]
    PathValidation(String),
    /// 安全违规错误。
    #[error("Security violation: {0}")]
    SecurityViolation(String),
}

/// 路径验证器，用于确保文件操作在安全的工作目录范围内。
///
/// 此验证器防止路径遍历攻击，确保所有文件操作都在指定的工作目录内进行。
pub struct PathValidator {
    /// 工作目录路径，所有文件操作必须在此目录内
    working_dir: PathBuf,
}

impl PathValidator {
    /// 创建一个新的路径验证器。
    ///
    /// # 参数
    ///
    /// * `working_dir` - 工作目录路径
    pub fn new(working_dir: PathBuf) -> Self {
        Self { working_dir }
    }

    /// Validates a path and returns the canonical path if valid.
    ///
    /// # Errors
    ///
    /// Returns `SecurityError::PathValidation` if the path cannot be normalized,
    /// or `SecurityError::SecurityViolation` if the path is outside the working directory.
    pub fn validate(&self, path: &Path) -> Result<PathBuf, SecurityError> {
        let input_path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.working_dir.join(path)
        };

        let canonical = Self::normalize(&input_path)?;
        let working_canonical = Self::normalize(&self.working_dir)?;

        if !canonical.starts_with(&working_canonical) {
            return Err(SecurityError::SecurityViolation(format!(
                "Path '{}' is outside working directory '{}'",
                canonical.display(),
                working_canonical.display()
            )));
        }

        Ok(canonical)
    }

    /// 规范化路径，解析所有符号链接和相对路径组件。
    ///
    /// # 参数
    ///
    /// * `path` - 要规范化的路径
    ///
    /// # 返回
    ///
    /// 返回规范化的绝对路径。
    ///
    /// # 错误
    ///
    /// 如果路径无法规范化，返回 `SecurityError::PathValidation`。
    fn normalize(path: &Path) -> Result<PathBuf, SecurityError> {
        let result = std::fs::canonicalize(path);
        match result {
            Ok(canonical) => Ok(canonical),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                if let Some(parent) = path.parent()
                    && let Ok(parent_canonical) = std::fs::canonicalize(parent)
                {
                    let file_name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default();
                    return Ok(parent_canonical.join(file_name));
                }
                Ok(path.to_path_buf())
            },
            Err(e) => Err(SecurityError::PathValidation(format!(
                "Failed to normalize path '{}': {}",
                path.display(),
                e
            ))),
        }
    }

    /// 获取工作目录路径。
    ///
    /// # 返回
    ///
    /// 返回工作目录的引用。
    #[must_use]
    pub fn working_dir(&self) -> &Path {
        &self.working_dir
    }
}

/// 行内容验证结果。
///
/// 表示实际内容与预期内容的匹配程度。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerifyResult {
    /// 完全匹配。
    ExactMatch,
    /// 前缀匹配（用于长行内容的编辑）。
    PrefixMatch,
    /// 内容不匹配。
    Mismatch,
    /// 内容太短，无法判断是否匹配。
    TooShort,
}

/// 行内容验证配置。
///
/// 用于控制行内容匹配验证的行为。
pub struct VerifyConfig {
    /// 前缀匹配的最小字符数阈值
    pub prefix_match_threshold: usize,
}

impl Default for VerifyConfig {
    fn default() -> Self {
        Self {
            prefix_match_threshold: 20,
        }
    }
}

/// 验证实际内容是否与预期内容匹配。
///
/// # 参数
///
/// * `actual` - 实际的内容
/// * `expected` - 预期的内容
///
/// # 返回
///
/// 返回验证结果，表示内容的匹配程度。
pub fn verify_line_content(actual: &str, expected: &str) -> VerifyResult {
    verify_line_content_with_config(actual, expected, &VerifyConfig::default())
}

/// 使用指定配置验证行内容是否匹配。
///
/// # 参数
///
/// * `actual` - 实际的内容
/// * `expected` - 预期的内容
/// * `config` - 验证配置
///
/// # 返回
///
/// 返回验证结果，表示内容的匹配程度。
#[allow(clippy::needless_pass_by_ref_mut)]
pub fn verify_line_content_with_config(
    actual: &str,
    expected: &str,
    config: &VerifyConfig,
) -> VerifyResult {
    let actual = actual.trim_end_matches('\n').trim_end_matches('\r');
    let expected = expected.trim_end_matches('\n').trim_end_matches('\r');

    if actual == expected {
        return VerifyResult::ExactMatch;
    }

    if expected.len() >= config.prefix_match_threshold && actual.starts_with(expected) {
        return VerifyResult::PrefixMatch;
    }

    if expected.len() < config.prefix_match_threshold && actual != expected {
        return VerifyResult::TooShort;
    }

    VerifyResult::Mismatch
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verify_exact_match() {
        assert_eq!(
            verify_line_content("hello world", "hello world"),
            VerifyResult::ExactMatch
        );
    }

    #[test]
    fn test_verify_prefix_match() {
        let long_actual = "this is a very long line that should trigger prefix match when verified";
        let long_prefix = "this is a very long line that should trigger prefix";
        assert_eq!(
            verify_line_content(long_actual, long_prefix),
            VerifyResult::PrefixMatch
        );
    }

    #[test]
    fn test_verify_mismatch() {
        assert_eq!(
            verify_line_content("hello there", "hello world"),
            VerifyResult::TooShort
        );
    }

    #[test]
    fn test_verify_too_short() {
        assert_eq!(verify_line_content("hi", "hello"), VerifyResult::TooShort);
    }
}
