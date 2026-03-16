//! Prompt Components Module
//!
//! This module provides functionality for loading and composing prompt components
//! from the file system and embedded default prompts.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use include_dir::include_dir;
use thiserror::Error;

/// Embedded prompts directory.
static EMBEDDED_PROMPTS: include_dir::Dir<'static> =
    include_dir!("$CARGO_MANIFEST_DIR/../assets/prompts");

/// Prompt component metadata.
#[derive(Debug, Clone)]
pub struct PromptComponent {
    /// Component identifier, e.g., "fs::read"
    pub id: String,
    /// File name without extension, e.g., "fs--read"
    pub file_name: String,
    /// Component content (lazy loaded)
    pub content: Option<String>,
}

/// Errors that can occur when loading or building prompts.
#[derive(Debug, Error)]
pub enum PromptError {
    /// Prompt component not found.
    #[error("提示词组件不存在: {0}")]
    NotFound(String),

    /// Encoding error when reading prompt file.
    #[error("提示词编码错误: {0}")]
    Encoding(String),

    /// Invalid path provided.
    #[error("提示词路径无效: {0}")]
    InvalidPath(String),

    /// Failed to load prompt from file system.
    #[error("提示词加载失败: {0}")]
    LoadFailed(String),
}

/// Trait for loading prompt components from file system.
pub trait PromptLoader: Send + Sync {
    /// Load prompt content by component ID.
    fn load(&self, id: &str) -> Result<String, PromptError>;

    /// List all available components.
    fn list_components(&self) -> Result<Vec<PromptComponent>, PromptError>;

    /// Load prompt for a tool by tool ID.
    fn load_for_tool(&self, tool_id: &str) -> Result<Option<String>, PromptError> {
        let file_name = tool_id.replace("::", "--");
        match self.load(&file_name) {
            Ok(content) => Ok(Some(content)),
            Err(PromptError::NotFound(_)) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

/// Load embedded prompts from compile-time directory.
fn load_embedded_prompts() -> HashMap<String, String> {
    let mut map = HashMap::new();

    for file in EMBEDDED_PROMPTS.files() {
        let file_name = file
            .path()
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default();

        if file.path().extension().and_then(|s| s.to_str()) == Some("md") {
            let content = file.contents_utf8().unwrap_or_default();
            map.insert(file_name.to_string(), content.to_string());
        }
    }

    map
}

/// Implementation of PromptLoader.
pub struct PromptLoaderImpl {
    /// Config directories to search.
    config_dirs: Vec<PathBuf>,
    /// Embedded prompts.
    embedded: HashMap<String, String>,
}

impl PromptLoaderImpl {
    /// Create a new PromptLoaderImpl with the given config directories.
    /// Includes embedded default prompts.
    #[must_use]
    pub fn new(config_dirs: Vec<PathBuf>) -> Self {
        Self {
            config_dirs,
            embedded: load_embedded_prompts(),
        }
    }

    /// Create a new PromptLoaderImpl with custom config directories and embedded prompts.
    #[must_use]
    pub fn with_embedded(config_dirs: Vec<PathBuf>, embedded: HashMap<String, String>) -> Self {
        Self {
            config_dirs,
            embedded,
        }
    }

    /// Create a new PromptLoaderImpl with default config directories.
    ///
    /// Searches for prompt files in the following priority order:
    /// 1. .neoco/prompts/ (project root)
    /// 2. .agents/prompts/ (project root)
    /// 3. ~/.config/neoco/prompts/
    /// 4. ~/.agents/prompts/
    /// 5. Embedded default prompts (assets/prompts/) - lowest priority
    ///
    /// Returns a new PromptLoaderImpl with the default directories.
    #[must_use]
    pub fn new_with_default_dirs() -> Self {
        let config_dirs = Self::build_default_config_dirs();
        Self::new(config_dirs)
    }

    /// Create a new PromptLoaderImpl with default config dirs for a specific project root.
    ///
    /// The project root is used for the first two priority directories (.neoco/prompts/ and .agents/prompts/).
    /// The home directory paths (~/.config/neoco/prompts/ and ~/.agents/prompts/) are always included.
    /// Embedded default prompts (assets/prompts/) have the lowest priority.
    #[must_use]
    pub fn new_with_project_root(project_root: &Path) -> Self {
        let mut config_dirs = Vec::new();

        // Priority 1: .neoco/prompts/ (project root)
        let neoco_dir = project_root.join(".neoco").join("prompts");
        config_dirs.push(neoco_dir);

        // Priority 2: .agents/prompts/ (project root)
        let agents_dir = project_root.join(".agents").join("prompts");
        config_dirs.push(agents_dir);

        // Priority 3: ~/.config/neoco/prompts/
        if let Some(config_dir) = dirs::config_dir() {
            let neoco_config_dir = config_dir.join("neoco").join("prompts");
            config_dirs.push(neoco_config_dir);
        }

        // Priority 4: ~/.agents/prompts/
        if let Some(home_dir) = dirs::home_dir() {
            let agents_home_dir = home_dir.join(".agents").join("prompts");
            config_dirs.push(agents_home_dir);
        }

        Self::new(config_dirs)
    }

    /// Build the default config directories list.
    fn build_default_config_dirs() -> Vec<PathBuf> {
        let mut config_dirs = Vec::new();

        // Get current working directory for project root
        let project_root = std::env::current_dir().unwrap_or_default();

        // Priority 1: .neoco/prompts/ (project root)
        let neoco_dir = project_root.join(".neoco").join("prompts");
        if neoco_dir.exists() {
            config_dirs.push(neoco_dir);
        }

        // Priority 2: .agents/prompts/ (project root)
        let agents_dir = project_root.join(".agents").join("prompts");
        if agents_dir.exists() {
            config_dirs.push(agents_dir);
        }

        // Priority 3: ~/.config/neoco/prompts/
        if let Some(config_dir) = dirs::config_dir() {
            let neoco_config_dir = config_dir.join("neoco").join("prompts");
            if neoco_config_dir.exists() {
                config_dirs.push(neoco_config_dir);
            }
        }

        // Priority 4: ~/.agents/prompts/
        if let Some(home_dir) = dirs::home_dir() {
            let agents_home_dir = home_dir.join(".agents").join("prompts");
            if agents_home_dir.exists() {
                config_dirs.push(agents_home_dir);
            }
        }

        config_dirs
    }

    /// Finds a prompt file by ID.
    fn find_prompt_file(&self, id: &str) -> Result<PathBuf, PromptError> {
        let file_name = Self::id_to_file_name(id);
        let file_path = format!("{}.md", file_name);

        for dir in &self.config_dirs {
            let prompts_dir = dir.join("prompts");
            let full_path = prompts_dir.join(&file_path);

            if full_path.exists() {
                return Ok(full_path);
            }
        }

        Err(PromptError::NotFound(format!(
            "提示词组件 '{}' (file: {}) 不存在",
            id, file_path
        )))
    }

    /// Converts an ID to a file name.
    fn id_to_file_name(id: &str) -> String {
        id.replace("::", "--")
    }

    /// Validates a prompt ID.
    fn validate_id(id: &str) -> Result<(), PromptError> {
        if id.contains("..") {
            return Err(PromptError::InvalidPath("路径不允许包含 '..'".to_string()));
        }

        if Path::new(id).is_absolute() {
            return Err(PromptError::InvalidPath("路径不能是绝对路径".to_string()));
        }

        if id.starts_with('/') || id.starts_with('\\') {
            return Err(PromptError::InvalidPath("路径不能以斜杠开头".to_string()));
        }

        Ok(())
    }

    /// Processes prompt content.
    fn process_content(content: &str) -> String {
        let content = content.trim_start_matches('\u{feff}');
        let content = content.replace("\r\n", "\n");
        let content = content.replace('\r', "\n");

        let lines: Vec<&str> = content.lines().collect();
        let processed: Vec<String> = lines
            .iter()
            .map(|line| line.trim_end().to_string())
            .collect();

        processed.join("\n")
    }

    /// Lists prompt files.
    #[allow(clippy::unnecessary_wraps)]
    fn list_prompt_files(&self) -> Vec<PathBuf> {
        let mut files = Vec::new();

        for dir in &self.config_dirs {
            let prompts_dir = dir.join("prompts");
            if !prompts_dir.exists() {
                continue;
            }

            if let Ok(entries) = std::fs::read_dir(prompts_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|s| s.to_str()) == Some("md") {
                        files.push(path);
                    }
                }
            }
        }

        files
    }

    /// Converts a file name to an ID.
    fn file_name_to_id(file_name: &str) -> String {
        file_name.replace("--", "::")
    }

    /// Gets an embedded prompt by ID.
    fn get_embedded(&self, id: &str) -> Option<String> {
        self.embedded.get(id).cloned()
    }
}

impl PromptLoader for PromptLoaderImpl {
    fn load(&self, id: &str) -> Result<String, PromptError> {
        Self::validate_id(id)?;

        // Priority 1-4: Search in config directories
        match self.find_prompt_file(id) {
            Ok(file_path) => {
                let content = std::fs::read_to_string(&file_path)
                    .map_err(|e| PromptError::LoadFailed(format!("读取文件失败: {}", e)))?;
                return Ok(Self::process_content(&content));
            },
            Err(PromptError::NotFound(_)) => {
                // Continue to embedded prompts
            },
            Err(e) => return Err(e),
        }

        // Priority 5: Fall back to embedded default prompts
        let file_name = Self::id_to_file_name(id);
        // First try direct match
        if let Some(content) = self.get_embedded(&file_name) {
            return Ok(Self::process_content(&content));
        }
        // If not found and doesn't already start with "tool--" or "context--", try with prefix
        // This handles cases like "multi-agent" -> "tool--multi-agent"
        if !file_name.starts_with("tool--") && !file_name.starts_with("context--") {
            let with_prefix = format!("tool--{}", file_name);
            if let Some(content) = self.get_embedded(&with_prefix) {
                return Ok(Self::process_content(&content));
            }
        }

        Err(PromptError::NotFound(format!("提示词组件 '{}' 不存在", id)))
    }

    fn list_components(&self) -> Result<Vec<PromptComponent>, PromptError> {
        let mut components = Vec::new();
        let mut seen_ids: std::collections::HashSet<String> = std::collections::HashSet::new();

        // List files from config directories
        let files = self.list_prompt_files();
        for file_path in files {
            if let Some(file_name) = file_path.file_stem() {
                let file_name = file_name.to_string_lossy();
                let id = Self::file_name_to_id(&file_name);
                if seen_ids.insert(id.clone()) {
                    components.push(PromptComponent {
                        id,
                        file_name: file_name.to_string(),
                        content: None,
                    });
                }
            }
        }

        // Add embedded prompts (only if not already in config dirs)
        for (id, content) in &self.embedded {
            if seen_ids.insert(id.clone()) {
                let file_name = id.replace("::", "--");
                components.push(PromptComponent {
                    id: id.clone(),
                    file_name,
                    content: Some(content.clone()),
                });
            }
        }

        Ok(components)
    }

    fn load_for_tool(&self, tool_id: &str) -> Result<Option<String>, PromptError> {
        let file_name = tool_id.replace("::", "--");

        match self.load(&file_name) {
            Ok(content) => Ok(Some(content)),
            Err(PromptError::NotFound(_)) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

/// Trait for building composed prompts from multiple components.
pub trait PromptBuilder: Send + Sync {
    /// Build a complete prompt from component IDs.
    fn build(&self, components: &[String]) -> Result<String, PromptError>;
}

/// Generic implementation of PromptBuilder.
pub struct PromptBuilderImpl<L: PromptLoader> {
    /// The loader used to fetch prompts.
    loader: L,
}

impl<L: PromptLoader> PromptBuilderImpl<L> {
    /// Create a new builder with the given loader.
    #[must_use]
    pub fn new(loader: L) -> Self {
        Self { loader }
    }
}

impl<L: PromptLoader> PromptBuilder for PromptBuilderImpl<L> {
    fn build(&self, components: &[String]) -> Result<String, PromptError> {
        let mut result = Vec::new();

        for component_id in components {
            let content = self.loader.load(component_id)?;
            result.push(content);
        }

        Ok(result.join("\n\n"))
    }
}

impl PromptBuilderImpl<PromptLoaderImpl> {
    /// Create a builder from config directories.
    #[must_use]
    pub fn from_config_dirs(config_dirs: Vec<PathBuf>) -> Self {
        Self::new(PromptLoaderImpl::new(config_dirs))
    }
}
