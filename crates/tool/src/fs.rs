//! File system tools.

use std::path::PathBuf;
use std::sync::LazyLock;
use std::time::Duration;

use async_trait::async_trait;
use neoco_core::ids::ToolId;
use neoco_core::{
    ToolCapabilities, ToolCategory, ToolContext, ToolDefinition, ToolError, ToolExecutor,
    ToolOutput, ToolResult,
};
use serde_json::{Map, Value};
use tokio::fs;

use crate::security::{PathValidator, VerifyResult, verify_line_content};

/// 将错误转换为工具执行错误。
///
/// # 参数
///
/// * `e` - 要转换的错误
///
/// # 返回
///
/// 返回 `ToolError::ExecutionFailed` 错误。
fn convert_err(e: impl std::fmt::Display) -> ToolError {
    ToolError::ExecutionFailed(e.to_string())
}

/// 创建工具参数的 JSON Schema。
///
/// # 参数
///
/// * `props` - 属性数组，每个元素为 (名称, 描述, 是否必需) 的三元组
///
/// # 返回
///
/// 返回符合 JSON Schema 规范的值对象。
fn make_schema(props: &[(&str, &str, bool)]) -> Value {
    let mut map = Map::new();
    map.insert(
        "$schema".to_string(),
        Value::String("https://json-schema.org/draft/2020-12/schema".to_string()),
    );
    map.insert("type".to_string(), Value::String("object".to_string()));
    map.insert("title".to_string(), Value::String("工具参数".to_string()));
    map.insert("examples".to_string(), Value::Array(vec![]));
    let mut props_map = Map::new();
    let mut required = Vec::new();
    for (name, desc, req) in props {
        let mut p = Map::new();
        p.insert("type".to_string(), Value::String("string".to_string()));
        p.insert("description".to_string(), Value::String(desc.to_string()));
        if *req {
            required.push(Value::String(name.to_string()));
        }
        props_map.insert(name.to_string(), Value::Object(p));
    }
    map.insert("properties".to_string(), Value::Object(props_map));
    if !required.is_empty() {
        map.insert("required".to_string(), Value::Array(required));
    }
    Value::Object(map)
}

/// 文件读取参数的 JSON Schema。
static READ_SCHEMA: LazyLock<Value> = LazyLock::new(|| make_schema(&[("path", "文件路径", true)]));

/// 文件读取工具定义。
static DEFINITION: LazyLock<ToolDefinition> = LazyLock::new(|| ToolDefinition {
    id: ToolId::from_string("fs::read").unwrap(),
    description: "读取文件内容".to_string(),
    schema: READ_SCHEMA.clone(),
    capabilities: ToolCapabilities::default(),
    timeout: Duration::from_secs(5),
    category: ToolCategory::Common,
    prompt_component: Some("tool::fs::read".to_string()),
});

/// 文件读取工具。
///
/// 此工具用于读取指定文件的内容，支持分段读取。
pub struct FileReadTool;

#[async_trait]
impl ToolExecutor for FileReadTool {
    fn definition(&self) -> &ToolDefinition {
        &DEFINITION
    }

    async fn execute(&self, ctx: &ToolContext, args: Value) -> Result<ToolResult, ToolError> {
        let path: String = args
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParameters("path is required".to_string()))?
            .to_string();

        let offset = args
            .get("offset")
            .and_then(serde_json::Value::as_u64)
            .map_or(1, |v| usize::try_from(v).unwrap_or(1));

        let limit = args
            .get("limit")
            .and_then(serde_json::Value::as_u64)
            .map(|v| usize::try_from(v).unwrap_or(usize::MAX));

        let validator = PathValidator::new(ctx.working_dir.clone());
        let validated_path = validator
            .validate(PathBuf::from(&path).as_path())
            .map_err(convert_err)?;

        let file_content = fs::read_to_string(&validated_path)
            .await
            .map_err(convert_err)?;

        let lines: Vec<&str> = file_content.lines().collect();
        let start = offset.saturating_sub(1).min(lines.len());
        let end = limit.map_or(lines.len(), |l| (start + l).min(lines.len()));
        let selected: String = lines
            .get(start..end)
            .map(|s: &[&str]| s.join("\n"))
            .unwrap_or_default();

        Ok(ToolResult {
            output: ToolOutput::Text(selected),
            is_error: false,
            prompt_component: None,
        })
    }
}

/// 文件写入参数的 JSON Schema。
static WRITE_SCHEMA: LazyLock<Value> = LazyLock::new(|| {
    let mut map = Map::new();
    map.insert("type".to_string(), Value::String("object".to_string()));
    let mut props = Map::new();
    props.insert(
        "path".to_string(),
        Value::Object({
            let mut p = Map::new();
            p.insert("type".to_string(), Value::String("string".to_string()));
            p
        }),
    );
    props.insert(
        "content".to_string(),
        Value::Object({
            let mut p = Map::new();
            p.insert("type".to_string(), Value::String("string".to_string()));
            p
        }),
    );
    props.insert(
        "verify".to_string(),
        Value::Object({
            let mut p = Map::new();
            p.insert("type".to_string(), Value::String("string".to_string()));
            p
        }),
    );
    map.insert("properties".to_string(), Value::Object(props));
    Value::Object(map)
});

/// 文件写入工具定义。
static WRITE_DEFINITION: LazyLock<ToolDefinition> = LazyLock::new(|| ToolDefinition {
    id: ToolId::from_string("fs::write").unwrap(),
    description: "写入文件内容（完全覆盖），支持verify参数验证".to_string(),
    schema: WRITE_SCHEMA.clone(),
    capabilities: ToolCapabilities::default(),
    timeout: Duration::from_secs(10),
    category: ToolCategory::Common,
    prompt_component: Some("tool::fs::write".to_string()),
});

/// 文件写入工具。
///
/// 此工具用于向文件写入内容，会完全覆盖原有内容。
/// 使用临时文件确保原子性写入。
pub struct FileWriteTool;

#[async_trait]
impl ToolExecutor for FileWriteTool {
    fn definition(&self) -> &ToolDefinition {
        &WRITE_DEFINITION
    }

    async fn execute(&self, ctx: &ToolContext, args: Value) -> Result<ToolResult, ToolError> {
        let path: String = args
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParameters("path is required".to_string()))?
            .to_string();

        let file_content: String = args
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParameters("content is required".to_string()))?
            .to_string();

        let verify: Option<String> = args
            .get("verify")
            .and_then(|v| v.as_str())
            .map(String::from);

        let validator = PathValidator::new(ctx.working_dir.clone());
        let validated_path = validator
            .validate(PathBuf::from(&path).as_path())
            .map_err(convert_err)?;

        if let Some(expected_content) = verify
            && validated_path.exists()
        {
            let actual_content = fs::read_to_string(&validated_path)
                .await
                .map_err(convert_err)?;
            let verify_result = verify_line_content(&actual_content, &expected_content);
            if verify_result == VerifyResult::ExactMatch {
                return Ok(ToolResult {
                    output: ToolOutput::Text(format!(
                        "内容一致，无需写入: {}",
                        validated_path.display()
                    )),
                    is_error: false,
                    prompt_component: None,
                });
            }
            if verify_result == VerifyResult::Mismatch || verify_result == VerifyResult::TooShort {
                return Err(ToolError::InvalidParameters(
                    "Verify failed: expected content differs from existing file content"
                        .to_string(),
                ));
            }
        }

        if let Some(parent) = validated_path.parent() {
            fs::create_dir_all(parent).await.map_err(convert_err)?;
        }

        let temp_path = validated_path.with_extension("tmp");
        fs::write(&temp_path, &file_content)
            .await
            .map_err(convert_err)?;
        fs::rename(&temp_path, &validated_path)
            .await
            .map_err(convert_err)?;

        Ok(ToolResult {
            output: ToolOutput::Text(format!("写入成功: {}", validated_path.display())),
            is_error: false,
            prompt_component: None,
        })
    }
}

/// 文件编辑参数的 JSON Schema。
static EDIT_SCHEMA: LazyLock<Value> = LazyLock::new(|| {
    let mut map = Map::new();
    map.insert("type".to_string(), Value::String("object".to_string()));
    let mut props = Map::new();
    let mut verify_props = Map::new();
    verify_props.insert(
        "line".to_string(),
        Value::Object({
            let mut p = Map::new();
            p.insert("type".to_string(), Value::String("integer".to_string()));
            p
        }),
    );
    verify_props.insert(
        "content".to_string(),
        Value::Object({
            let mut p = Map::new();
            p.insert("type".to_string(), Value::String("string".to_string()));
            p
        }),
    );
    props.insert(
        "path".to_string(),
        Value::Object({
            let mut p = Map::new();
            p.insert("type".to_string(), Value::String("string".to_string()));
            p
        }),
    );
    props.insert(
        "verify".to_string(),
        Value::Object({
            let mut p = Map::new();
            p.insert("type".to_string(), Value::String("object".to_string()));
            p.insert("properties".to_string(), Value::Object(verify_props));
            p
        }),
    );
    props.insert(
        "new_content".to_string(),
        Value::Object({
            let mut p = Map::new();
            p.insert("type".to_string(), Value::String("string".to_string()));
            p
        }),
    );
    map.insert("properties".to_string(), Value::Object(props));
    Value::Object(map)
});

/// 文件编辑工具定义。
static EDIT_DEFINITION: LazyLock<ToolDefinition> = LazyLock::new(|| ToolDefinition {
    id: ToolId::from_string("fs::edit").unwrap(),
    description: "基于verify编辑文件内容".to_string(),
    schema: EDIT_SCHEMA.clone(),
    capabilities: ToolCapabilities::default(),
    timeout: Duration::from_secs(10),
    category: ToolCategory::Common,
    prompt_component: Some("tool::fs::edit".to_string()),
});

/// 文件编辑工具。
///
/// 此工具基于验证机制编辑文件内容，确保在修改前内容符合预期。
/// 支持基于行号的精确编辑。
pub struct FileEditTool;

#[async_trait]
impl ToolExecutor for FileEditTool {
    fn definition(&self) -> &ToolDefinition {
        &EDIT_DEFINITION
    }

    async fn execute(&self, ctx: &ToolContext, args: Value) -> Result<ToolResult, ToolError> {
        let path: String = args
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParameters("path is required".to_string()))?
            .to_string();

        let verify = args
            .get("verify")
            .ok_or_else(|| ToolError::InvalidParameters("verify is required".to_string()))?;

        let line: usize = usize::try_from(
            verify
                .get("line")
                .and_then(serde_json::Value::as_u64)
                .ok_or_else(|| {
                    ToolError::InvalidParameters("verify.line is required".to_string())
                })?,
        )
        .map_err(|_| ToolError::InvalidParameters("line number too large".to_string()))?;

        let expected_content: String = verify
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParameters("verify.content is required".to_string()))?
            .to_string();

        let new_content: String = args
            .get("new_content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParameters("new_content is required".to_string()))?
            .to_string();

        let validator = PathValidator::new(ctx.working_dir.clone());
        let validated_path = validator
            .validate(PathBuf::from(&path).as_path())
            .map_err(convert_err)?;

        let file_content = fs::read_to_string(&validated_path)
            .await
            .map_err(convert_err)?;

        let mut lines: Vec<String> = file_content.lines().map(String::from).collect();

        if line == 0 || line > lines.len() {
            return Err(ToolError::InvalidParameters(format!(
                "Line {line} is out of range (1-{})",
                lines.len()
            )));
        }

        let actual_content = lines
            .get(line - 1)
            .ok_or_else(|| ToolError::InvalidParameters("Line index out of bounds".to_string()))?;
        let verify_result = verify_line_content(actual_content, &expected_content);

        if verify_result == VerifyResult::Mismatch || verify_result == VerifyResult::TooShort {
            return Err(ToolError::InvalidParameters(format!(
                "Verify failed: expected '{expected_content}', got '{actual_content}'"
            )));
        }

        if let Some(line_mut) = lines.get_mut(line - 1) {
            *line_mut = new_content;
        }
        let new_file_content = lines.join("\n");

        let temp_path = validated_path.with_extension("tmp");
        fs::write(&temp_path, &new_file_content)
            .await
            .map_err(convert_err)?;
        fs::rename(&temp_path, &validated_path)
            .await
            .map_err(convert_err)?;

        Ok(ToolResult {
            output: ToolOutput::Text(format!("编辑成功: {}", validated_path.display())),
            is_error: false,
            prompt_component: None,
        })
    }
}

/// 文件删除参数的 JSON Schema。
static DELETE_SCHEMA: LazyLock<Value> =
    LazyLock::new(|| make_schema(&[("path", "要删除的文件路径", true)]));

/// 文件删除工具定义。
static DELETE_DEFINITION: LazyLock<ToolDefinition> = LazyLock::new(|| ToolDefinition {
    id: ToolId::from_string("fs::delete").unwrap(),
    description: "删除文件或目录".to_string(),
    schema: DELETE_SCHEMA.clone(),
    capabilities: ToolCapabilities::default(),
    timeout: Duration::from_secs(5),
    category: ToolCategory::Common,
    prompt_component: Some("tool::fs::delete".to_string()),
});

/// 文件或目录删除工具。
///
/// 此工具用于删除指定的文件或目录（会递归删除目录及其内容）。
pub struct FileDeleteTool;

#[async_trait]
impl ToolExecutor for FileDeleteTool {
    fn definition(&self) -> &ToolDefinition {
        &DELETE_DEFINITION
    }

    async fn execute(&self, context: &ToolContext, args: Value) -> Result<ToolResult, ToolError> {
        let path: String = args
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParameters("path is required".to_string()))?
            .to_string();

        let validator = PathValidator::new(context.working_dir.clone());
        let validated_path = validator
            .validate(PathBuf::from(&path).as_path())
            .map_err(convert_err)?;

        if !validated_path.exists() {
            return Err(ToolError::NotFound);
        }

        if validated_path.is_dir() {
            let mut entries = fs::read_dir(&validated_path).await.map_err(convert_err)?;
            if entries.next_entry().await.map_err(convert_err)?.is_some() {
                return Err(ToolError::InvalidParameters(
                    "目录不为空，无法删除".to_string(),
                ));
            }
            fs::remove_dir(&validated_path).await.map_err(convert_err)?;
        } else {
            fs::remove_file(&validated_path)
                .await
                .map_err(convert_err)?;
        }

        Ok(ToolResult {
            output: ToolOutput::Text(format!("删除成功: {}", validated_path.display())),
            is_error: false,
            prompt_component: None,
        })
    }
}

/// 目录列表参数的 JSON Schema。
static LIST_SCHEMA: LazyLock<Value> = LazyLock::new(|| {
    let mut map = Map::new();
    map.insert("type".to_string(), Value::String("object".to_string()));
    let mut props = Map::new();
    props.insert(
        "path".to_string(),
        Value::Object({
            let mut p = Map::new();
            p.insert("type".to_string(), Value::String("string".to_string()));
            p
        }),
    );
    props.insert(
        "include_hidden".to_string(),
        Value::Object({
            let mut p = Map::new();
            p.insert("type".to_string(), Value::String("boolean".to_string()));
            p
        }),
    );
    map.insert("properties".to_string(), Value::Object(props));
    Value::Object(map)
});

/// 目录列表工具定义。
static LIST_DEFINITION: LazyLock<ToolDefinition> = LazyLock::new(|| ToolDefinition {
    id: ToolId::from_string("fs::list").unwrap(),
    description: "列出目录内容".to_string(),
    schema: LIST_SCHEMA.clone(),
    capabilities: ToolCapabilities::default(),
    timeout: Duration::from_secs(5),
    category: ToolCategory::Common,
    prompt_component: Some("tool::fs::list".to_string()),
});

/// 目录列表工具。
///
/// 此工具用于列出指定目录下的文件和子目录。
pub struct FileListTool;

#[async_trait]
impl ToolExecutor for FileListTool {
    fn definition(&self) -> &ToolDefinition {
        &LIST_DEFINITION
    }

    async fn execute(&self, ctx: &ToolContext, args: Value) -> Result<ToolResult, ToolError> {
        let path = args
            .get("path")
            .and_then(serde_json::Value::as_str)
            .map_or_else(|| ctx.working_dir.clone(), PathBuf::from);

        let include_hidden = args
            .get("include_hidden")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);

        let validator = PathValidator::new(ctx.working_dir.clone());
        let validated_path = validator.validate(&path).map_err(convert_err)?;

        if !validated_path.is_dir() {
            return Err(ToolError::InvalidParameters(
                "path is not a directory".to_string(),
            ));
        }

        let mut entries = Vec::new();
        let mut dir = fs::read_dir(&validated_path).await.map_err(convert_err)?;

        while let Some(entry) = dir.next_entry().await.map_err(convert_err)? {
            let file_name = entry.file_name();
            let name = file_name.to_string_lossy().to_string();

            if !include_hidden && name.starts_with('.') {
                continue;
            }

            let file_type = entry.file_type().await.map_err(convert_err)?;
            let is_dir = file_type.is_dir();

            entries.push(format!("{} {}", if is_dir { "d" } else { "-" }, name));
        }

        entries.sort();

        Ok(ToolResult {
            output: ToolOutput::Text(entries.join("\n")),
            is_error: false,
            prompt_component: None,
        })
    }
}

/// 目录 ls 工具定义。
static LS_DEFINITION: LazyLock<ToolDefinition> = LazyLock::new(|| ToolDefinition {
    id: ToolId::from_string("fs::ls").unwrap(),
    description: "列出目录内容（fs::list 的别名）".to_string(),
    schema: LIST_SCHEMA.clone(),
    capabilities: ToolCapabilities::default(),
    timeout: Duration::from_secs(5),
    category: ToolCategory::Common,
    prompt_component: Some("tool::fs::ls".to_string()),
});

/// 文件 ls 工具（`FileListTool` 的别名）。
///
/// 此工具是 `FileListTool` 的别名，功能完全相同。
pub struct FileLsTool(FileListTool);

impl Default for FileLsTool {
    fn default() -> Self {
        Self::new()
    }
}

impl FileLsTool {
    /// 创建一个新的 `FileLsTool` 实例。
    ///
    /// # 返回
    ///
    /// 返回一个新的 `FileLsTool` 实例。
    pub fn new() -> Self {
        Self(FileListTool)
    }
}

#[async_trait]
impl ToolExecutor for FileLsTool {
    fn definition(&self) -> &ToolDefinition {
        &LS_DEFINITION
    }

    async fn execute(&self, ctx: &ToolContext, args: Value) -> Result<ToolResult, ToolError> {
        self.0.execute(ctx, args).await
    }
}

/// 文件 rm 工具定义。
static RM_DEFINITION: LazyLock<ToolDefinition> = LazyLock::new(|| ToolDefinition {
    id: ToolId::from_string("fs::rm").unwrap(),
    description: "删除文件或目录（fs::delete 的别名）".to_string(),
    schema: DELETE_SCHEMA.clone(),
    capabilities: ToolCapabilities::default(),
    timeout: Duration::from_secs(5),
    category: ToolCategory::Common,
    prompt_component: Some("tool::fs::rm".to_string()),
});

/// 文件 rm 工具（`FileDeleteTool` 的别名）。
///
/// 此工具是 `FileDeleteTool` 的别名，功能完全相同。
pub struct FileRmTool(FileDeleteTool);

impl Default for FileRmTool {
    fn default() -> Self {
        Self::new()
    }
}

impl FileRmTool {
    /// 创建一个新的 `FileRmTool` 实例。
    ///
    /// # 返回
    ///
    /// 返回一个新的 `FileRmTool` 实例。
    pub fn new() -> Self {
        Self(FileDeleteTool)
    }
}

#[async_trait]
impl ToolExecutor for FileRmTool {
    fn definition(&self) -> &ToolDefinition {
        &RM_DEFINITION
    }

    async fn execute(&self, ctx: &ToolContext, args: Value) -> Result<ToolResult, ToolError> {
        self.0.execute(ctx, args).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use neoco_core::{AgentUlid, SessionUlid};

    fn create_test_context() -> ToolContext {
        let session = SessionUlid::new();
        ToolContext {
            session_ulid: session,
            agent_ulid: AgentUlid::new_root(&session),
            working_dir: std::env::temp_dir(),
            user_interaction_tx: None,
        }
    }

    #[tokio::test]
    async fn test_file_read() {
        let ctx = create_test_context();
        let tool = FileReadTool;

        let path = ctx.working_dir.join("test_read.txt");
        tokio::fs::write(&path, "hello\nworld").await.unwrap();

        let result = tool
            .execute(&ctx, serde_json::json!({ "path": path.to_string_lossy() }))
            .await;
        result.unwrap();

        tokio::fs::remove_file(&path).await.ok();
    }

    #[tokio::test]
    async fn test_file_write() {
        let ctx = create_test_context();
        let tool = FileWriteTool;

        let path = ctx.working_dir.join("test_write.txt");

        let result = tool
            .execute(
                &ctx,
                serde_json::json!({
                    "path": path.to_string_lossy(),
                    "content": "test content"
                }),
            )
            .await;
        result.unwrap();

        let content = tokio::fs::read_to_string(&path).await.unwrap();
        assert_eq!(content, "test content");

        tokio::fs::remove_file(&path).await.ok();
    }
}
