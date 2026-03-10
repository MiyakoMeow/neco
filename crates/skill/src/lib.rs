//! `NeoCo` Skill Management
//!
//! This crate provides skill management functionality for the `NeoCo` project.

#![allow(unused_crate_dependencies)]

use async_trait::async_trait;
use neoco_core::ToolCapabilities;
use neoco_core::ToolCategory;
use neoco_core::ToolDefinition;
use neoco_core::ToolError;
use neoco_core::ToolExecutor;
use neoco_core::ToolOutput;
use neoco_core::ToolRegistry;
use neoco_core::ToolResult;
use neoco_core::ids::SkillUlid;
use neoco_core::ids::ToolId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::fs;
use tokio::process::Command;

pub use neoco_core::SkillError;

/// Skill definition for discovery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillDefinition {
    /// Skill ID.
    pub id: SkillUlid,
    /// Skill name.
    pub name: String,
    /// Skill description.
    pub description: String,
    /// Skill tags.
    pub tags: Vec<String>,
}

/// Index of available skills.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SkillIndex {
    /// List of skills.
    pub skills: Vec<SkillInfo>,
}

impl SkillIndex {
    /// Gets a skill by ID.
    #[must_use]
    pub fn get(&self, id: &SkillUlid) -> Option<&SkillInfo> {
        self.skills.iter().find(|s| &s.id == id)
    }
}

/// Skill information from discovery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillInfo {
    /// Skill ID.
    pub id: SkillUlid,
    /// Skill directory name (stable identifier).
    pub dir_name: String,
    /// Skill name.
    pub name: String,
    /// Skill description.
    pub description: String,
    /// Skill tags.
    pub tags: Vec<String>,
    /// Base path to skill files.
    pub base_path: Option<PathBuf>,
}

/// Loaded skill with content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    /// Skill ID.
    pub id: SkillUlid,
    /// Skill name.
    pub name: String,
    /// Skill description.
    pub description: String,
    /// Skill instruction content.
    pub content: String,
    /// Skill tags.
    pub tags: Vec<String>,
    /// Base path to skill files.
    pub base_path: PathBuf,
}

/// Skill metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillMetadata {
    /// Skill version.
    pub version: String,
    /// Skill author.
    pub author: Option<String>,
    /// Skill tags.
    pub tags: Vec<String>,
    /// Skill dependencies.
    pub dependencies: Vec<String>,
}

impl Default for SkillMetadata {
    fn default() -> Self {
        Self {
            version: "0.1.0".to_string(),
            author: None,
            tags: Vec::new(),
            dependencies: Vec::new(),
        }
    }
}

/// Script language for skills.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScriptLanguage {
    /// Shell script (bash, sh, zsh).
    Shell,
    /// Python script.
    Python,
    /// Rust script.
    Rust,
    /// JavaScript script.
    JavaScript,
}

impl ScriptLanguage {
    /// Gets the script language from a file extension.
    #[must_use]
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_lowercase().as_str() {
            "sh" | "bash" | "zsh" => Some(Self::Shell),
            "py" => Some(Self::Python),
            "rs" => Some(Self::Rust),
            "js" | "mjs" | "cjs" => Some(Self::JavaScript),
            _ => None,
        }
    }
}

/// Script information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScriptInfo {
    /// Path to the script file.
    pub path: PathBuf,
    /// Script language.
    pub language: ScriptLanguage,
    /// Entry point function name.
    pub entry_point: Option<String>,
}

/// Reference file information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReferenceInfo {
    /// Path to the reference file.
    pub path: PathBuf,
    /// Content type of the reference.
    pub content_type: String,
}

/// Asset file information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetInfo {
    /// Path to the asset file.
    pub path: PathBuf,
    /// MIME type of the asset.
    pub mime_type: String,
}

/// Skill resources including scripts, references, and assets.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SkillResources {
    /// Base path for resources.
    pub base_path: PathBuf,
    /// Scripts in the skill.
    pub scripts: HashMap<String, ScriptInfo>,
    /// Reference files in the skill.
    pub references: HashMap<String, ReferenceInfo>,
    /// Asset files in the skill.
    pub assets: HashMap<String, AssetInfo>,
}

/// Activated skill with loaded resources.
#[derive(Debug, Clone)]
pub struct ActivatedSkill {
    /// Skill ID.
    pub id: SkillUlid,
    /// Skill name.
    pub name: String,
    /// Skill instruction content.
    pub instruction: String,
    /// Skill metadata.
    pub metadata: SkillMetadata,
    /// Skill resources.
    pub resources: SkillResources,
    /// Tools provided by the skill.
    pub tools: Vec<ToolDefinition>,
}

/// Skill service errors.
#[derive(Debug, Error)]
pub enum SkillServiceError {
    /// Skill not found.
    #[error("Skill未找到: {0}")]
    NotFound(String),

    /// Parse error.
    #[error("解析失败: {0}")]
    ParseError(String),

    /// I/O error.
    #[error("IO错误: {0}")]
    IoError(#[source] std::io::Error),

    /// Validation error.
    #[error("验证失败: {0}")]
    ValidationError(String),

    /// Load failed.
    #[error("加载失败: {0}")]
    LoadFailed(String),

    /// Activation failed.
    #[error("激活失败: {0}")]
    ActivationFailed(String),
}

impl From<std::io::Error> for SkillServiceError {
    fn from(err: std::io::Error) -> Self {
        Self::IoError(err)
    }
}

impl From<serde_yaml::Error> for SkillServiceError {
    fn from(err: serde_yaml::Error) -> Self {
        Self::ParseError(err.to_string())
    }
}

impl From<SkillServiceError> for neoco_core::SkillError {
    fn from(err: SkillServiceError) -> Self {
        match err {
            SkillServiceError::NotFound(id) => neoco_core::SkillError::NotFound(id),
            SkillServiceError::ActivationFailed(msg) => {
                neoco_core::SkillError::ActivationFailed(msg)
            },
            SkillServiceError::LoadFailed(msg)
            | SkillServiceError::ParseError(msg)
            | SkillServiceError::ValidationError(msg) => {
                neoco_core::SkillError::ExecutionFailed(msg)
            },
            SkillServiceError::IoError(e) => neoco_core::SkillError::ExecutionFailed(e.to_string()),
        }
    }
}

/// Trait for skill management services.
#[async_trait]
pub trait SkillService: Send + Sync {
    /// Discovers available skills.
    async fn discover_skills(&self) -> Result<Vec<SkillDefinition>, SkillServiceError>;
    /// Activates a skill by ID.
    async fn activate(&self, skill_ulid: &SkillUlid) -> Result<ActivatedSkill, SkillServiceError>;
    /// Deactivates a skill by ID.
    async fn deactivate(&self, skill_ulid: &SkillUlid) -> Result<(), SkillServiceError>;
    /// Gets critical reminder for a skill (~100 tokens).
    /// Returns None if the skill doesn't have a critical reminder file.
    async fn get_critical_reminder(
        &self,
        skill_ulid: &SkillUlid,
    ) -> Result<Option<String>, SkillServiceError>;
}

/// Default implementation of the skill service.
pub struct DefaultSkillService {
    /// Skill index.
    index: tokio::sync::RwLock<SkillIndex>,
    /// Base path for skills.
    skills_base_path: PathBuf,
    /// Config directory for storing persistent data.
    config_dir: PathBuf,
    /// Currently activated skills.
    activated_skills: tokio::sync::RwLock<HashMap<SkillUlid, ActivatedSkill>>,
    /// Tool registry for registering/unregistering skill tools.
    tool_registry: Option<Arc<dyn ToolRegistry>>,
}

impl DefaultSkillService {
    /// Creates a new skill service.
    #[must_use]
    pub fn new(
        skills_base_path: PathBuf,
        config_dir: PathBuf,
        tool_registry: Option<Arc<dyn ToolRegistry>>,
    ) -> Self {
        Self {
            index: tokio::sync::RwLock::new(SkillIndex::default()),
            skills_base_path,
            config_dir,
            activated_skills: tokio::sync::RwLock::new(HashMap::new()),
            tool_registry,
        }
    }

    /// Gets the path to the skill index storage file.
    fn skill_index_path(&self) -> PathBuf {
        self.config_dir.join("skill_index.json")
    }

    /// Loads the persisted skill ID mapping (directory name -> `SkillUlid`).
    async fn load_skill_id_map(&self) -> HashMap<String, SkillUlid> {
        let path = self.skill_index_path();
        if !path.exists() {
            return HashMap::new();
        }
        match fs::read_to_string(&path).await {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => HashMap::new(),
        }
    }

    /// Saves the skill ID mapping to persistent storage.
    async fn save_skill_id_map(
        &self,
        map: &HashMap<String, SkillUlid>,
    ) -> Result<(), SkillServiceError> {
        let path = self.skill_index_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }
        let content = serde_json::to_string_pretty(map)
            .map_err(|e| SkillServiceError::IoError(std::io::Error::other(e)))?;
        fs::write(&path, content).await?;
        Ok(())
    }

    /// Loads the skill index.
    ///
    /// # Errors
    ///
    /// Returns an error if reading the skills directory fails.
    pub async fn load_index(&self) -> Result<SkillIndex, SkillServiceError> {
        let mut index = SkillIndex::default();

        if !self.skills_base_path.exists() {
            return Ok(index);
        }

        let id_map = self.load_skill_id_map().await;
        let mut new_id_map = id_map.clone();

        let mut entries = tokio::fs::read_dir(&self.skills_base_path).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.is_dir() {
                let dir_name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_string();

                let id = id_map
                    .get(&dir_name)
                    .copied()
                    .unwrap_or_else(SkillUlid::new);

                if let Ok(skill_info) = self.parse_skill_info(&path, &dir_name, id).await {
                    if !new_id_map.contains_key(&dir_name) {
                        new_id_map.insert(dir_name.clone(), id);
                    }
                    index.skills.push(skill_info);
                }
            }
        }

        if let Err(e) = self.save_skill_id_map(&new_id_map).await {
            eprintln!("Warning: failed to save skill index: {e}");
        }

        let mut guard = self.index.write().await;
        *guard = index.clone();

        Ok(index)
    }

    /// Reloads the skill index.
    ///
    /// # Errors
    ///
    /// Returns an error if reading the skills directory fails.
    pub async fn reload_index(&self) -> Result<SkillIndex, SkillServiceError> {
        let mut guard = self.index.write().await;
        guard.skills.clear();
        drop(guard);
        self.load_index().await
    }

    /// Parses skill information from a directory.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the skill directory
    /// * `dir_name` - Directory name (stable identifier)
    /// * `id` - Pre-assigned `SkillUlid` (from persistent storage or new)
    ///
    /// # Errors
    ///
    /// Returns an error if SKILL.md is missing or invalid.
    async fn parse_skill_info(
        &self,
        path: &Path,
        dir_name: &str,
        id: SkillUlid,
    ) -> Result<SkillInfo, SkillServiceError> {
        let skill_md_path = path.join("SKILL.md");
        if !skill_md_path.exists() {
            return Err(SkillServiceError::ValidationError(
                "SKILL.md not found".to_string(),
            ));
        }

        let content = tokio::fs::read_to_string(&skill_md_path).await?;
        let (metadata, _) = parse_yaml_frontmatter(&content)?;

        let name = metadata
            .get("name")
            .ok_or_else(|| SkillServiceError::ValidationError("name is required".to_string()))?
            .as_str()
            .ok_or_else(|| SkillServiceError::ValidationError("name must be a string".to_string()))?
            .to_string();

        let description = metadata
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let tags: Vec<String> = metadata
            .get("tags")
            .and_then(|v| v.as_sequence())
            .map(|seq| {
                seq.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        Ok(SkillInfo {
            id,
            dir_name: dir_name.to_string(),
            name,
            description,
            tags,
            base_path: Some(path.to_path_buf()),
        })
    }

    /// Loads a skill by ID.
    ///
    /// # Errors
    ///
    /// Returns an error if the skill is not found or loading fails.
    pub async fn load_skill(&self, id: &SkillUlid) -> Result<Skill, SkillServiceError> {
        let guard = self.index.read().await;
        let skill_info = guard
            .get(id)
            .ok_or_else(|| SkillServiceError::NotFound(id.to_string()))?;

        let base_path = skill_info.base_path.clone().ok_or_else(|| {
            SkillServiceError::ValidationError("base_path is missing".to_string())
        })?;

        let skill_md_path = base_path.join("SKILL.md");
        let content = tokio::fs::read_to_string(&skill_md_path).await?;

        let (_, instruction) = parse_yaml_frontmatter(&content)?;

        Ok(Skill {
            id: *id,
            name: skill_info.name.clone(),
            description: skill_info.description.clone(),
            content: instruction,
            tags: skill_info.tags.clone(),
            base_path,
        })
    }

    /// Activates a skill by ID.
    ///
    /// # Errors
    ///
    /// Returns an error if the skill is not found or activation fails.
    pub async fn activate(&self, id: &SkillUlid) -> Result<ActivatedSkill, SkillServiceError> {
        let skill = self.load_skill(id).await?;

        let base_path = &skill.base_path;
        let resources = self.load_resources(base_path).await?;

        let skill_md_path = base_path.join("SKILL.md");
        let content = tokio::fs::read_to_string(&skill_md_path).await?;
        let (yaml_metadata, _) = parse_yaml_frontmatter(&content)?;
        let metadata = parse_skill_metadata(&yaml_metadata);

        let mut tools = Vec::new();
        for name in resources.scripts.keys() {
            let tool = create_tool_definition(&skill.name, name);
            tools.push(tool);
        }

        let activated = ActivatedSkill {
            id: *id,
            name: skill.name,
            instruction: skill.content,
            metadata,
            resources,
            tools,
        };

        if let Some(ref registry) = self.tool_registry {
            register_skill_tools(&activated, registry.as_ref()).await?;
        }

        let mut guard = self.activated_skills.write().await;
        guard.insert(*id, activated.clone());

        Ok(activated)
    }

    /// Deactivates a skill by ID.
    ///
    /// # Errors
    ///
    /// Returns an error if the skill is not found.
    pub async fn deactivate(&self, id: &SkillUlid) -> Result<(), SkillServiceError> {
        let mut guard = self.activated_skills.write().await;
        let skill = guard
            .remove(id)
            .ok_or_else(|| SkillServiceError::NotFound(id.to_string()))?;

        if let Some(ref registry) = self.tool_registry {
            for tool in &skill.tools {
                registry.unregister(&tool.id).await;
            }
        }

        Ok(())
    }

    /// Loads skill resources from a directory.
    ///
    /// # Errors
    ///
    /// Returns an error if reading the directory fails.
    async fn load_resources(&self, base_path: &Path) -> Result<SkillResources, SkillServiceError> {
        let mut resources = SkillResources {
            base_path: base_path.to_path_buf(),
            scripts: HashMap::new(),
            references: HashMap::new(),
            assets: HashMap::new(),
        };

        let scripts_path = base_path.join("scripts");
        if scripts_path.exists() {
            let mut entries = tokio::fs::read_dir(&scripts_path).await?;
            while let Some(entry) = entries.next_entry().await? {
                let path = entry.path();
                if path.is_file()
                    && let Some(ext) = path.extension().and_then(|e| e.to_str())
                    && let Some(lang) = ScriptLanguage::from_extension(ext)
                {
                    let name = path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("unnamed")
                        .to_string();
                    resources.scripts.insert(
                        name.clone(),
                        ScriptInfo {
                            path: path.clone(),
                            language: lang,
                            entry_point: None,
                        },
                    );
                }
            }
        }

        let references_path = base_path.join("references");
        if references_path.exists() {
            let mut entries = tokio::fs::read_dir(&references_path).await?;
            while let Some(entry) = entries.next_entry().await? {
                let path = entry.path();
                if path.is_file() {
                    let name = path
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("unnamed")
                        .to_string();
                    let content_type = path
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("application/octet-stream")
                        .to_string();
                    resources
                        .references
                        .insert(name.clone(), ReferenceInfo { path, content_type });
                }
            }
        }

        let assets_path = base_path.join("assets");
        if assets_path.exists() {
            let mut entries = tokio::fs::read_dir(&assets_path).await?;
            while let Some(entry) = entries.next_entry().await? {
                let path = entry.path();
                if path.is_file() {
                    let name = path
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("unnamed")
                        .to_string();
                    let mime_type = path
                        .extension()
                        .and_then(|e| e.to_str())
                        .map_or_else(|| "application/octet-stream".to_string(), mime_from_ext);
                    resources
                        .assets
                        .insert(name.clone(), AssetInfo { path, mime_type });
                }
            }
        }

        Ok(resources)
    }
}

#[async_trait]
impl SkillService for DefaultSkillService {
    async fn discover_skills(&self) -> Result<Vec<SkillDefinition>, SkillServiceError> {
        let guard = self.index.read().await;
        let definitions: Vec<SkillDefinition> = guard
            .skills
            .iter()
            .map(|info| SkillDefinition {
                id: info.id,
                name: info.name.clone(),
                description: info.description.clone(),
                tags: info.tags.clone(),
            })
            .collect();
        Ok(definitions)
    }

    async fn activate(&self, skill_ulid: &SkillUlid) -> Result<ActivatedSkill, SkillServiceError> {
        DefaultSkillService::activate(self, skill_ulid).await
    }

    async fn deactivate(&self, skill_ulid: &SkillUlid) -> Result<(), SkillServiceError> {
        DefaultSkillService::deactivate(self, skill_ulid).await
    }

    async fn get_critical_reminder(
        &self,
        skill_ulid: &SkillUlid,
    ) -> Result<Option<String>, SkillServiceError> {
        let index = self.index.read().await;
        let skill_info = index
            .get(skill_ulid)
            .ok_or_else(|| SkillServiceError::NotFound(skill_ulid.to_string()))?;

        let base_path = skill_info
            .base_path
            .as_ref()
            .ok_or_else(|| SkillServiceError::NotFound("Base path not found".to_string()))?;

        let critical_reminder_path = base_path.join("critical_reminder.md");

        if !critical_reminder_path.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(&critical_reminder_path)
            .await
            .map_err(SkillServiceError::IoError)?;

        Ok(Some(content.trim().to_string()))
    }
}

/// Parses YAML frontmatter from content.
fn parse_yaml_frontmatter(content: &str) -> Result<(serde_yaml::Value, String), SkillServiceError> {
    let content = content.trim_start();
    if !content.starts_with("---") {
        return Ok((serde_yaml::Value::Null, content.to_string()));
    }

    let mut lines = content.lines();
    lines.next();

    let mut yaml_content = String::new();
    for line in &mut lines {
        if line.trim() == "---" {
            let remaining: String = lines.collect::<Vec<_>>().join("\n");
            let metadata: serde_yaml::Value = serde_yaml::from_str(&yaml_content)?;
            return Ok((metadata, remaining));
        }
        yaml_content.push_str(line);
        yaml_content.push('\n');
    }

    Err(SkillServiceError::ParseError(
        "Invalid frontmatter format".to_string(),
    ))
}

/// Gets MIME type from file extension.
#[must_use]
fn mime_from_ext(ext: &str) -> String {
    match ext.to_lowercase().as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        "webp" => "image/webp",
        "pdf" => "application/pdf",
        "zip" => "application/zip",
        "json" => "application/json",
        "txt" => "text/plain",
        "html" | "htm" => "text/html",
        "css" => "text/css",
        "js" => "application/javascript",
        "wasm" => "application/wasm",
        _ => "application/octet-stream",
    }
    .to_string()
}

/// Extracts a `ToolDefinition` from a skill and script.
fn create_tool_definition(skill_name: &str, script_name: &str) -> ToolDefinition {
    ToolDefinition {
        id: ToolId::from_string(&format!("skill::{skill_name}_{script_name}")).unwrap(),
        description: format!("Script from skill {skill_name}"),
        schema: serde_json::Value::Object(serde_json::Map::new()),
        capabilities: ToolCapabilities::default(),
        timeout: Duration::from_secs(30),
        category: ToolCategory::Common,
        prompt_component: None,
    }
}

/// Parses skill metadata from YAML frontmatter.
fn parse_skill_metadata(metadata: &serde_yaml::Value) -> SkillMetadata {
    SkillMetadata {
        version: metadata
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or("0.1.0")
            .to_string(),
        author: metadata
            .get("author")
            .and_then(|v| v.as_str())
            .map(String::from),
        tags: metadata
            .get("tags")
            .and_then(|v| v.as_sequence())
            .map(|seq| {
                seq.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default(),
        dependencies: metadata
            .get("dependencies")
            .and_then(|v| v.as_sequence())
            .map(|seq| {
                seq.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default(),
    }
}
/// Skill tool wrapper.
struct SkillToolWrapper {
    /// 脚本信息
    script_info: ScriptInfo,
    /// 工具定义
    definition: ToolDefinition,
}

impl SkillToolWrapper {
    /// 创建一个新的技能工具包装器。
    ///
    /// # 参数
    ///
    /// * `skill_name` - 技能名称
    /// * `script_name` - 脚本名称
    /// * `script_info` - 脚本信息
    ///
    /// # 返回值
    ///
    /// 返回一个新的 `SkillToolWrapper` 实例。
    fn new(skill_name: &str, script_name: &str, script_info: ScriptInfo) -> Self {
        let definition = create_tool_definition(skill_name, script_name);
        Self {
            script_info,
            definition,
        }
    }
}

/// Extracts command-line arguments from a JSON value.
fn extract_args_from_json(args: &serde_json::Value) -> Vec<String> {
    match args {
        serde_json::Value::Null => Vec::new(),
        serde_json::Value::String(s) => vec![s.clone()],
        serde_json::Value::Array(arr) => arr
            .iter()
            .filter_map(|v| match v {
                serde_json::Value::String(s) => Some(s.clone()),
                serde_json::Value::Number(n) => Some(n.to_string()),
                serde_json::Value::Bool(b) => Some(b.to_string()),
                _ => None,
            })
            .collect(),
        serde_json::Value::Object(obj) => obj
            .iter()
            .flat_map(|(k, v)| {
                let value_str = match v {
                    serde_json::Value::String(s) => s.clone(),
                    serde_json::Value::Number(n) => n.to_string(),
                    serde_json::Value::Bool(b) => b.to_string(),
                    _ => v.to_string(),
                };
                vec![format!("--{}", k), value_str]
            })
            .collect(),
        _ => vec![args.to_string()],
    }
}

/// Executes a Rust script using rust-script.
async fn execute_rust_script(
    script_path: &Path,
    args: &[String],
) -> Result<std::process::Output, ToolError> {
    let output = Command::new("rust-script")
        .arg(script_path)
        .args(args)
        .output()
        .await
        .map_err(|e: std::io::Error| ToolError::ExecutionFailed(e.to_string()))?;
    Ok(output)
}

#[async_trait]
impl ToolExecutor for SkillToolWrapper {
    fn definition(&self) -> &ToolDefinition {
        &self.definition
    }

    async fn execute(
        &self,
        _context: &neoco_core::ToolContext,
        args: serde_json::Value,
    ) -> Result<ToolResult, ToolError> {
        let args_vec = extract_args_from_json(&args);

        let output: std::process::Output = match self.script_info.language {
            ScriptLanguage::Shell => {
                let mut cmd = Command::new("sh");
                cmd.arg(&self.script_info.path);
                for arg in &args_vec {
                    cmd.arg(arg);
                }
                cmd.output()
                    .await
                    .map_err(|e: std::io::Error| ToolError::ExecutionFailed(e.to_string()))?
            },
            ScriptLanguage::Python => {
                let mut cmd = Command::new("python");
                cmd.arg(&self.script_info.path);
                for arg in &args_vec {
                    cmd.arg(arg);
                }
                cmd.output()
                    .await
                    .map_err(|e: std::io::Error| ToolError::ExecutionFailed(e.to_string()))?
            },
            ScriptLanguage::JavaScript => {
                let mut cmd = Command::new("node");
                cmd.arg(&self.script_info.path);
                for arg in &args_vec {
                    cmd.arg(arg);
                }
                cmd.output()
                    .await
                    .map_err(|e: std::io::Error| ToolError::ExecutionFailed(e.to_string()))?
            },
            ScriptLanguage::Rust => execute_rust_script(&self.script_info.path, &args_vec).await?,
        };
        let tool_output = if output.status.success() {
            ToolOutput::Text(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            ToolOutput::Text(String::from_utf8_lossy(&output.stderr).to_string())
        };
        Ok(ToolResult {
            output: tool_output,
            is_error: !output.status.success(),
            prompt_component: None,
        })
    }
}

/// 注册已激活技能的工具到工具注册表。
///
/// # 参数
///
/// * `activated_skill` - 已激活的技能，包含工具脚本信息
/// * `registry` - 工具注册表，用于注册工具
///
/// # 返回值
///
/// 返回注册的工具数量。
///
/// # Errors
///
/// 如果工具注册失败，返回 `SkillServiceError`。
///
/// # Panics
///
/// 如果工具 ID 无法从 skill 名称和 script 名称构造，则 panic。
pub async fn register_skill_tools(
    activated_skill: &ActivatedSkill,
    registry: &dyn ToolRegistry,
) -> Result<usize, SkillServiceError> {
    let mut count = 0;
    for (script_name, script_info) in &activated_skill.resources.scripts {
        let tool_id =
            ToolId::from_string(&format!("skill::{}_{}", activated_skill.name, script_name))
                .unwrap();
        if registry.get(&tool_id).await.is_some() {
            return Err(SkillServiceError::ActivationFailed(format!(
                "Tool {tool_id} already registered"
            )));
        }
        let wrapper =
            SkillToolWrapper::new(&activated_skill.name, script_name, script_info.clone());
        registry.register(Arc::new(wrapper)).await;
        count += 1;
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use tempfile::TempDir;

    fn create_test_skill(dir: &Path, name: &str, description: &str) {
        let skill_dir = dir.join(name);
        std::fs::create_dir_all(&skill_dir).unwrap();
        let content = format!(
            r"---
name: {name}
description: {description}
tags:
  - test
  - rust
---

# Skill Instructions
This is a test skill.
"
        );
        std::fs::write(skill_dir.join("SKILL.md"), content).unwrap();

        let scripts_dir = skill_dir.join("scripts");
        std::fs::create_dir_all(&scripts_dir).unwrap();
        std::fs::write(scripts_dir.join("test.sh"), "#!/bin/bash\necho hello").unwrap();

        let refs_dir = skill_dir.join("references");
        std::fs::create_dir_all(&refs_dir).unwrap();
        std::fs::write(refs_dir.join("readme.txt"), "Reference content").unwrap();
    }

    #[tokio::test]
    async fn test_discover_skills() {
        let temp_dir = TempDir::new().unwrap();
        create_test_skill(temp_dir.path(), "test-skill", "A test skill");

        let config_dir = TempDir::new().unwrap();
        let service = DefaultSkillService::new(
            temp_dir.path().to_path_buf(),
            config_dir.path().to_path_buf(),
            None,
        );
        service.load_index().await.unwrap();

        let skills = service.discover_skills().await.unwrap();
        assert!(!skills.is_empty());
        assert_eq!(skills.first().map(|s| s.name.as_str()), Some("test-skill"));
    }

    #[tokio::test]
    async fn test_activate_skill() {
        let temp_dir = TempDir::new().unwrap();
        create_test_skill(temp_dir.path(), "test-skill", "A test skill");

        let config_dir = TempDir::new().unwrap();
        let service = DefaultSkillService::new(
            temp_dir.path().to_path_buf(),
            config_dir.path().to_path_buf(),
            None,
        );
        service.load_index().await.unwrap();

        let skills = service.discover_skills().await.unwrap();
        let skill_id = skills.first().expect("No skills found").id;

        let activated = service.activate(&skill_id).await.unwrap();
        assert_eq!(activated.name, "test-skill");
        assert!(!activated.resources.scripts.is_empty());
    }

    #[tokio::test]
    async fn test_deactivate_skill() {
        let temp_dir = TempDir::new().unwrap();
        create_test_skill(temp_dir.path(), "test-skill", "A test skill");

        let config_dir = TempDir::new().unwrap();
        let service = DefaultSkillService::new(
            temp_dir.path().to_path_buf(),
            config_dir.path().to_path_buf(),
            None,
        );
        service.load_index().await.unwrap();

        let skills = service.discover_skills().await.unwrap();
        let skill_id = skills.first().expect("No skills found").id;

        service.activate(&skill_id).await.unwrap();
        service.deactivate(&skill_id).await.unwrap();
    }

    #[tokio::test]
    async fn test_parse_yaml_frontmatter() {
        let content = r"---
name: test-skill
description: A test
tags:
  - rust
  - test
---

# Instructions
Hello world
";
        let (metadata, instructions) = parse_yaml_frontmatter(content).unwrap();
        assert_eq!(
            metadata.get("name").and_then(|v| v.as_str()),
            Some("test-skill")
        );
        assert!(instructions.contains("Instructions"));
    }

    #[test]
    fn test_script_language_from_extension() {
        assert_eq!(
            ScriptLanguage::from_extension("rs"),
            Some(ScriptLanguage::Rust)
        );
        assert_eq!(
            ScriptLanguage::from_extension("py"),
            Some(ScriptLanguage::Python)
        );
        assert_eq!(
            ScriptLanguage::from_extension("sh"),
            Some(ScriptLanguage::Shell)
        );
        assert_eq!(
            ScriptLanguage::from_extension("js"),
            Some(ScriptLanguage::JavaScript)
        );
        assert_eq!(ScriptLanguage::from_extension("unknown"), None);
    }
}
