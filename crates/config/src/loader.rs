//! Configuration loading module

use std::fmt::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::{Config, ConfigError};

/// Configuration loader with caching support.
pub struct ConfigLoader {
    /// Directories to search for configuration files.
    config_dirs: Vec<PathBuf>,
    /// Cached configuration.
    cache: Mutex<Option<Config>>,
    /// Override model from CLI/env.
    model_override: Option<String>,
    /// Override model group from CLI/env.
    model_group_override: Option<String>,
}

impl ConfigLoader {
    /// 创建默认配置加载器
    ///
    /// 使用默认配置目录列表初始化加载器
    #[must_use]
    pub fn new() -> Self {
        let dirs = Self::default_config_dirs();
        Self {
            config_dirs: dirs,
            cache: Mutex::new(None),
            model_override: None,
            model_group_override: None,
        }
    }

    /// 使用指定目录列表创建配置加载器
    ///
    /// # 参数
    ///
    /// * `dirs` - 配置文件搜索目录列表
    #[must_use]
    pub fn with_dirs(dirs: Vec<PathBuf>) -> Self {
        Self {
            config_dirs: dirs,
            cache: Mutex::new(None),
            model_override: None,
            model_group_override: None,
        }
    }

    /// 设置命令行/环境变量模型覆盖
    ///
    /// # 参数
    ///
    /// * `model` - 模型名称（如 "openai/gpt-4"）
    #[must_use]
    pub fn with_model(mut self, model: Option<String>) -> Self {
        self.model_override = model;
        self
    }

    /// 设置命令行/环境变量模型组覆盖
    ///
    /// # 参数
    ///
    /// * `model_group` - 模型组名称
    #[must_use]
    pub fn with_model_group(mut self, model_group: Option<String>) -> Self {
        self.model_group_override = model_group;
        self
    }

    /// 获取默认配置目录列表
    ///
    /// 返回按优先级排序的配置目录：
    /// - `.neoco` (当前目录)
    /// - `.agents` (当前目录)
    /// - `$XDG_CONFIG_HOME/neoco` 或 `$HOME/.config/neoco` (Unix/macOS)
    /// - `%APPDATA%\neoco` (Windows)
    /// - `$HOME/.agents`
    #[must_use]
    pub fn default_config_dirs() -> Vec<PathBuf> {
        use super::Paths;

        let mut dirs = Vec::new();

        dirs.push(PathBuf::from(".neoco"));
        dirs.push(PathBuf::from(".agents"));

        // 使用统一的Paths模块获取XDG_CONFIG_HOME/neoco
        let paths = Paths::new();
        dirs.push(paths.config_dir);

        // 保持兼容性，添加 ~/.agents
        if let Some(home) = dirs::home_dir() {
            dirs.push(home.join(".agents"));
        }

        dirs
    }

    /// Loads configuration from cache or filesystem.
    ///
    /// Configuration priority (low to high):
    /// 1. Default configuration
    /// 2. Global configuration (~/.config/neoco/)
    /// 3. Project configuration (.neoco/)
    /// 4. Environment variables
    /// 5. Command-line arguments
    ///
    /// # Errors
    ///
    /// Returns `ConfigError` if loading or validation fails.
    ///
    /// # Panics
    ///
    /// Panics if the cache mutex is poisoned.
    #[allow(clippy::unwrap_used)]
    pub fn load(&self) -> Result<Config, ConfigError> {
        let mut cache = self.cache.lock().unwrap();
        if let Some(config) = cache.as_ref() {
            return Ok(config.clone());
        }

        let config = self.load_config()?;

        let config = self.apply_env_overrides(config);

        let config = self.apply_cli_overrides(config);

        crate::ConfigValidator::validate(&config)?;
        let result = config.clone();
        *cache = Some(config);
        Ok(result)
    }

    /// 从环境变量加载配置覆盖
    ///
    /// 支持的环境变量：
    /// - `NEOCO_MODEL`: 覆盖默认模型
    /// - `NEOCO_MODEL_GROUP`: 覆盖默认模型组
    /// - `NEOCO_SESSION_DIR`: 覆盖会话目录
    #[allow(clippy::unused_self)]
    fn apply_env_overrides(&self, mut config: Config) -> Config {
        if let Ok(model) = std::env::var("NEOCO_MODEL")
            && !model.is_empty()
        {
            config.model = Some(model);
        }

        if let Ok(model_group) = std::env::var("NEOCO_MODEL_GROUP")
            && !model_group.is_empty()
        {
            config.model_group = Some(model_group);
        }

        if let Ok(session_dir) = std::env::var("NEOCO_SESSION_DIR")
            && !session_dir.is_empty()
        {
            config.system.storage.session_dir = PathBuf::from(session_dir);
        }

        config
    }

    /// 应用命令行参数覆盖
    fn apply_cli_overrides(&self, mut config: Config) -> Config {
        if let Some(model) = &self.model_override {
            config.model = Some(model.clone());
        }

        if let Some(model_group) = &self.model_group_override {
            config.model_group = Some(model_group.clone());
        }

        config
    }

    /// Loads configuration from configured directories.
    ///
    /// # Errors
    ///
    /// Returns `ConfigError` if loading fails.
    pub fn load_config(&self) -> Result<Config, ConfigError> {
        let mut merged = Config::default();

        for dir in &self.config_dirs {
            if dir.exists() {
                let config = self.load_from_dir(dir)?;
                merged = crate::ConfigMerger::merge(&merged, &config);
            }
        }

        // 合并内置 provider（用户配置的同名 provider 会覆盖内置值）
        let builtin = crate::builtin_providers();
        merged = crate::ConfigMerger::merge(
            &merged,
            &Config {
                model_providers: builtin,
                ..Default::default()
            },
        );

        Ok(merged)
    }

    /// Loads tagged configuration files from a directory.
    ///
    /// Tagged files are loaded in sorted order: neoco.<tag>.yaml -> neoco.<tag>.toml
    /// Tags are sorted alphabetically, so "dev" < "prod"
    ///
    /// # Errors
    ///
    /// Returns `ConfigError` if loading fails.
    fn load_tagged_configs(dir: &Path) -> Result<Config, ConfigError> {
        let mut merged = Config::default();

        let mut tagged_files: Vec<(String, String)> = Vec::new();

        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let file_name = entry.file_name();
                let file_name_str = file_name.to_string_lossy();

                if let Some(tag) = Self::extract_tag(&file_name_str) {
                    tagged_files.push((tag, entry.path().to_string_lossy().to_string()));
                }
            }
        }

        tagged_files.sort_by(|a, b| a.0.cmp(&b.0));

        for (_tag, file_path) in tagged_files {
            let content = std::fs::read_to_string(&file_path)?;
            let is_toml = file_path.to_lowercase().ends_with(".toml");
            let processed = Self::preprocess_config(&content, is_toml);
            let config: Config = if file_path.to_lowercase().ends_with(".toml") {
                toml::from_str(&processed).map_err(|e| ConfigError::ParseError(e.to_string()))?
            } else {
                serde_yaml::from_str(&processed)
                    .map_err(|e| ConfigError::ParseError(e.to_string()))?
            };
            merged = crate::ConfigMerger::merge(&merged, &config);
        }

        Ok(merged)
    }

    /// Extracts tag from filename like "neoco.dev.toml" -> "dev"
    fn extract_tag(file_name: &str) -> Option<String> {
        let name = file_name.strip_prefix("neoco.")?;
        name.strip_suffix(".toml")
            .map(std::string::ToString::to_string)
            .or_else(|| {
                name.strip_suffix(".yaml")
                    .map(std::string::ToString::to_string)
            })
    }

    /// Preprocesses configuration content to handle special array syntax.
    ///
    /// Syntax:
    /// - `+<item>` - append string item to array
    /// - `+{...}` - append object to array
    /// - `++<item>` - literal string starting with `+`
    ///
    /// When append syntax is detected, the array is output to `append_models` field
    /// instead of `models` field, so the merger can handle append logic.
    fn preprocess_config(content: &str, is_toml: bool) -> String {
        let mut result = content.to_string();

        if is_toml {
            result = result.replace("models = [+", "append_models = [+");
            result = result.replace("models=[+", "append_models=[+");
        }

        if !result.contains('[') || !result.contains('+') {
            return result;
        }

        if !result.contains("[+") {
            return result;
        }

        let mut chars = result.chars().peekable();
        let mut output = String::new();
        let mut in_array = false;
        let mut array_depth = 0;
        let mut in_string = false;
        let mut escaped = false;

        while let Some(c) = chars.next() {
            if escaped {
                output.push(c);
                escaped = false;
                continue;
            }

            if c == '\\' && !in_string {
                escaped = true;
                output.push(c);
                continue;
            }

            if c == '"' {
                in_string = !in_string;
                output.push(c);
                continue;
            }

            if in_string {
                output.push(c);
                continue;
            }

            if c == '[' && !in_string {
                array_depth += 1;
                if array_depth == 1 {
                    in_array = true;
                }
                output.push(c);
            } else if c == ']' && !in_string {
                array_depth -= 1;
                if array_depth == 0 {
                    in_array = false;
                }
                output.push(c);
            } else if in_array && array_depth == 1 && c == '+' {
                let next = chars.peek();
                if let Some(&next_char) = next {
                    if next_char == '+' {
                        chars.next();
                        let remaining: String = chars
                            .by_ref()
                            .take_while(|&c| c != ',' && c != ']')
                            .collect();
                        let _ = write!(
                            output,
                            "\"+{escaped_value}\"",
                            escaped_value = remaining.trim()
                        );
                        if let Some(&',') = chars.peek() {
                            output.push(',');
                            chars.next();
                        }
                        continue;
                    } else if next_char == '{' {
                        chars.next();
                        let mut brace_depth = 1;
                        let mut item_content = String::new();
                        for ch in chars.by_ref() {
                            if ch == '{' {
                                brace_depth += 1;
                            } else if ch == '}' {
                                brace_depth -= 1;
                                if brace_depth == 0 {
                                    break;
                                }
                            }
                            item_content.push(ch);
                        }
                        let item_str = format!("+{{{item_content}}}");
                        let processed = Self::process_array_item(&item_str);
                        output.push_str(&processed);
                        if let Some(&peek_c) = chars.peek()
                            && peek_c == ','
                        {
                            output.push(',');
                            chars.next();
                        }
                        continue;
                    } else if next_char == '"' || next_char.is_alphanumeric() {
                        let remaining: String = chars
                            .by_ref()
                            .take_while(|&c| c != ',' && c != ']')
                            .collect();
                        let full_item = format!("+{remaining}");
                        let processed = Self::process_array_item(&full_item);
                        output.push_str(&processed);
                        if let Some(&peek_c) = chars.peek()
                            && peek_c == ','
                        {
                            output.push(',');
                            chars.next();
                        }
                        continue;
                    }
                }
                output.push(c);
            } else {
                output.push(c);
            }
        }

        output
    }

    /// Processes a single array item with +/- prefix.
    fn process_array_item(item: &str) -> String {
        let trimmed = item.trim();
        if let Some(stripped) = trimmed.strip_prefix("++") {
            format!("\"+{stripped}\"")
        } else if let Some(value) = trimmed.strip_prefix('+') {
            if let Some(inner) = value.strip_prefix('{') {
                let inner = inner.trim_end_matches('}').trim();
                format!("{{{}}}", Self::process_object_properties(inner))
            } else {
                format!("\"{value}\"")
            }
        } else {
            item.to_string()
        }
    }

    /// Processes object properties to handle + prefix in values.
    fn process_object_properties(props: &str) -> String {
        let mut result = String::new();
        let mut chars = props.chars().peekable();
        let mut in_string = false;
        let mut escaped = false;

        while let Some(c) = chars.next() {
            if escaped {
                result.push(c);
                escaped = false;
                continue;
            }

            match c {
                '\\' if !in_string => {
                    escaped = true;
                    result.push(c);
                },
                '"' => {
                    in_string = !in_string;
                    result.push(c);
                },
                ',' => {
                    result.push(',');
                },
                '+' if !in_string => {
                    let next = chars.peek();
                    if let Some(&next_char) = next {
                        if next_char == '+' {
                            chars.next();
                            let value: String = chars
                                .by_ref()
                                .take_while(|&c| c != ',' && c != '"')
                                .collect();
                            let _ = write!(result, "\"+{}\"", value.trim());
                            continue;
                        } else if next_char.is_alphanumeric() || next_char == '-' {
                            let value: String = chars
                                .by_ref()
                                .take_while(|&c| c != ',' && c != '"')
                                .collect();
                            let _ = write!(result, "\"{}\"", value.trim());
                            continue;
                        }
                    }
                    result.push(c);
                },
                _ => result.push(c),
            }
        }

        result
    }

    /// Loads configuration from a specific directory.
    ///
    /// # Errors
    ///
    /// Returns `ConfigError` if loading fails.
    pub fn load_from_dir(&self, dir: &Path) -> Result<Config, ConfigError> {
        let mut merged = Config::default();

        let base_files = ["neoco.yaml", "neoco.toml"];

        for file_name in base_files {
            let file_path = dir.join(file_name);
            if file_path.exists() {
                let content = std::fs::read_to_string(&file_path)?;
                let processed =
                    Self::preprocess_config(&content, file_name.to_lowercase().ends_with(".yaml"));
                let config: Config = if file_name.eq_ignore_ascii_case("neoco.toml") {
                    toml::from_str(&processed)
                        .map_err(|e| ConfigError::ParseError(e.to_string()))?
                } else {
                    serde_yaml::from_str(&processed)
                        .map_err(|e| ConfigError::ParseError(e.to_string()))?
                };
                merged = crate::ConfigMerger::merge(&merged, &config);
            }
        }

        let tagged = Self::load_tagged_configs(dir)?;
        merged = crate::ConfigMerger::merge(&merged, &tagged);

        Ok(merged)
    }

    /// Reloads configuration, clearing the cache.
    ///
    /// # Errors
    ///
    /// Returns `ConfigError` if loading or validation fails.
    ///
    /// # Panics
    ///
    /// Panics if the cache mutex is poisoned.
    #[allow(clippy::unwrap_used)]
    pub fn reload(&self) -> Result<Config, ConfigError> {
        let mut cache = self.cache.lock().unwrap();
        *cache = None;
        drop(cache);
        self.load()
    }

    /// Loads workflow-specific configuration.
    ///
    /// # Errors
    ///
    /// Returns `ConfigError` if loading or validation fails.
    pub fn load_workflow_config(&self, workflow_dir: &Path) -> Result<Config, ConfigError> {
        let mut config = self.load()?;
        let workflow_config = self.load_from_dir(workflow_dir)?;
        config = crate::ConfigMerger::merge(&config, &workflow_config);
        crate::ConfigValidator::validate(&config)?;
        Ok(config)
    }

    /// 获取配置目录列表
    ///
    /// # 返回值
    ///
    /// 配置文件搜索目录的切片引用
    pub fn config_dirs(&self) -> &[PathBuf] {
        &self.config_dirs
    }

    /// 获取所有配置目录下的 prompts 子目录列表
    ///
    /// 按内置优先级顺序返回 prompts 目录：
    /// 1. .neoco/prompts (当前目录)
    /// 2. .agents/prompts (当前目录)
    /// 3. `$XDG_CONFIG_HOME/neoco/prompts` 或 `$HOME/.config/neoco/prompts`
    /// 4. `$HOME/.agents/prompts`
    ///
    /// 仅返回存在的目录，优先级由内置顺序决定而非 `config_dirs` 参数
    #[must_use]
    pub fn prompt_dirs(&self) -> Vec<PathBuf> {
        let mut dirs = Vec::new();

        let priority_dirs = Self::default_config_dirs();

        for dir in priority_dirs {
            let prompts_dir = dir.join("prompts");
            if prompts_dir.exists() && prompts_dir.is_dir() {
                dirs.push(prompts_dir);
            }
        }

        dirs
    }
}

impl Default for ConfigLoader {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for ConfigLoader {
    fn clone(&self) -> Self {
        Self {
            config_dirs: self.config_dirs.clone(),
            cache: Mutex::new(None),
            model_override: self.model_override.clone(),
            model_group_override: self.model_group_override.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_default_config_dirs() {
        let dirs = ConfigLoader::default_config_dirs();
        assert!(!dirs.is_empty());
        assert_eq!(dirs.first(), Some(&PathBuf::from(".neoco")));
        assert_eq!(dirs.get(1), Some(&PathBuf::from(".agents")));
    }

    #[test]
    fn test_with_dirs() {
        let dirs = vec![PathBuf::from("/custom/path")];
        let loader = ConfigLoader::with_dirs(dirs.clone());
        assert_eq!(loader.config_dirs(), &dirs);
    }

    #[test]
    fn test_load_from_empty_dir() {
        let temp_dir = TempDir::new().unwrap();
        let loader = ConfigLoader::new();
        let config = loader.load_from_dir(temp_dir.path()).unwrap();
        assert!(config.model_groups.is_empty());
    }

    #[test]
    fn test_load_from_dir_with_toml() {
        let temp_dir = TempDir::new().unwrap();
        let config_content = r#"
[model_groups.test]
models = [{ provider = "openai", name = "gpt-4" }]

[model_providers.openai]
type = "openai"
name = "OpenAI"
base_url = "https://api.openai.com/v1"
timeout = 60
api_key = { source = "literal", name = "test-api-key" }

[system]
[system.storage]
[system.context]
[system.tools]
[system.ui]
"#;
        fs::write(temp_dir.path().join("neoco.toml"), config_content).unwrap();

        let loader = ConfigLoader::new();
        let config = loader.load_from_dir(temp_dir.path()).unwrap();

        assert!(config.model_groups.contains_key("test"));
    }

    #[test]
    fn test_load_workflow_config() {
        let temp_dir = TempDir::new().unwrap();
        let loader = ConfigLoader::new();

        let workflow_path = temp_dir.path().join("workflow");
        fs::create_dir(&workflow_path).unwrap();

        let workflow_config = r#"
[model_groups.workflow_test]
models = [{ provider = "test", name = "test-model" }]

[model_providers.test]
type = "openai"
name = "Test"
base_url = "https://api.test.com"
timeout = 60
api_key = { source = "literal", name = "test-api-key" }

[system]
[system.storage]
[system.context]
[system.tools]
[system.ui]
"#;
        fs::write(workflow_path.join("neoco.toml"), workflow_config).unwrap();

        let result = loader.load_workflow_config(&workflow_path);
        result.unwrap();
    }

    #[test]
    fn test_tagged_config_loading_order() {
        let temp_dir = TempDir::new().unwrap();

        let config_z = r#"
[model_groups.z_test]
models = [{ provider = "z", name = "z-model" }]

[model_providers.z]
type = "openai"
name = "Z"
base_url = "https://api.z.com"
timeout = 60
api_key = { source = "literal", name = "test-api-key" }

[system]
[system.storage]
[system.context]
[system.tools]
[system.ui]
"#;
        fs::write(temp_dir.path().join("neoco.z.toml"), config_z).unwrap();

        let config_a = r#"
[model_groups.a_test]
models = [{ provider = "a", name = "a-model" }]

[model_providers.a]
type = "openai"
name = "A"
base_url = "https://api.a.com"
timeout = 60
api_key = { source = "literal", name = "test-api-key" }

[system]
[system.storage]
[system.context]
[system.tools]
[system.ui]
"#;
        fs::write(temp_dir.path().join("neoco.a.toml"), config_a).unwrap();

        let loader = ConfigLoader::new();
        let config = loader.load_from_dir(temp_dir.path()).unwrap();

        assert!(config.model_groups.contains_key("a_test"));
        assert!(config.model_groups.contains_key("z_test"));
    }

    #[test]
    fn test_string_append_syntax() {
        let content = r"
[model_groups.test]
models = [+new-model]

[system]
[system.storage]
[system.context]
[system.tools]
[system.ui]
";
        let processed = ConfigLoader::preprocess_config(content, true);
        assert!(processed.contains("\"new-model\""));
    }

    #[test]
    fn test_escape_syntax() {
        let content = r"
[model_groups.test]
models = [++my-value, +normal-value]

[system]
[system.storage]
[system.context]
[system.tools]
[system.ui]
";
        let processed = ConfigLoader::preprocess_config(content, false);
        assert!(processed.contains("\"+my-value\""));
        assert!(processed.contains("\"normal-value\""));
    }

    #[test]
    fn test_extract_tag() {
        assert_eq!(
            ConfigLoader::extract_tag("neoco.dev.toml"),
            Some("dev".to_string())
        );
        assert_eq!(
            ConfigLoader::extract_tag("neoco.prod.yaml"),
            Some("prod".to_string())
        );
        assert_eq!(ConfigLoader::extract_tag("neoco.toml"), None);
        assert_eq!(ConfigLoader::extract_tag("other.toml"), None);
    }

    #[test]
    fn test_process_array_item() {
        assert_eq!(ConfigLoader::process_array_item("+item"), "\"item\"");
        assert_eq!(ConfigLoader::process_array_item("++item"), "\"+item\"");
        assert_eq!(
            ConfigLoader::process_array_item("+{ key = \"value\" }"),
            "{key = \"value\"}"
        );
    }

    #[test]
    fn test_with_model_override() {
        let loader = ConfigLoader::new().with_model(Some("openai/gpt-4".to_string()));
        assert_eq!(loader.model_override, Some("openai/gpt-4".to_string()));
    }

    #[test]
    fn test_with_model_group_override() {
        let loader = ConfigLoader::new().with_model_group(Some("fast".to_string()));
        assert_eq!(loader.model_group_override, Some("fast".to_string()));
    }

    #[test]
    fn test_clone_with_overrides() {
        let loader = ConfigLoader::new()
            .with_model(Some("openai/gpt-4".to_string()))
            .with_model_group(Some("balanced".to_string()));

        let cloned = loader.clone();
        assert_eq!(cloned.model_override, Some("openai/gpt-4".to_string()));
        assert_eq!(cloned.model_group_override, Some("balanced".to_string()));
    }

    #[test]
    fn test_config_priority_order() {
        let temp_dir = TempDir::new().unwrap();

        let global_config = r#"
[model_groups.test]
models = [{ provider = "minimax-cn", name = "global-model" }]

[model_providers.minimax-cn]
type = "openai"
name = "MiniMax"
base_url = "https://api.minimaxi.com/v1"
api_key_env = "MINIMAX_API_KEY"
timeout = 60

[system]
[system.storage]
[system.context]
[system.tools]
[system.ui]
"#;
        fs::write(temp_dir.path().join("neoco.toml"), global_config).unwrap();

        let loader = ConfigLoader::with_dirs(vec![temp_dir.path().to_path_buf()]);
        let config = loader.load().unwrap();

        assert!(config.model_groups.contains_key("test"));
    }
}
