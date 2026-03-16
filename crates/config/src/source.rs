//! Configuration source trait and implementations

use std::path::PathBuf;

use crate::{Config, ConfigError};
use futures::Stream;
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::mpsc;

/// 配置源 trait
///
/// 定义了从不同来源加载配置的统一接口
pub trait ConfigSource: Send + Sync {
    /// Load configuration from the source.
    ///
    /// # Errors
    ///
    /// Returns an error if loading fails.
    fn load(&self) -> Result<Config, ConfigError>;
    /// Watch for configuration changes.
    ///
    /// # Errors
    ///
    /// Returns an error if watching is not supported.
    fn watch(
        &self,
    ) -> Result<Box<dyn Stream<Item = Result<Config, ConfigError>> + Send>, ConfigError>;
}

/// 配置监视器
///
/// 监视配置文件变化并产生新的配置值
pub struct ConfigWatcher {
    /// 配置文件路径
    path: PathBuf,
    /// notify 文件监视器
    _watcher: RecommendedWatcher,
    /// 事件接收通道
    rx: mpsc::Receiver<Result<Event, notify::Error>>,
}

impl ConfigWatcher {
    /// 创建新的配置监视器
    ///
    /// # 参数
    ///
    /// * `path` - 配置文件路径
    /// * `watcher` - notify 文件监视器
    /// * `rx` - 事件接收通道
    #[must_use]
    pub fn new(
        path: PathBuf,
        watcher: RecommendedWatcher,
        rx: mpsc::Receiver<Result<Event, notify::Error>>,
    ) -> Self {
        Self {
            path,
            _watcher: watcher,
            rx,
        }
    }
}

impl Stream for ConfigWatcher {
    type Item = Result<Config, ConfigError>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        use notify::EventKind;
        use std::task::Poll;

        match std::pin::Pin::new(&mut self.rx).poll_recv(cx) {
            Poll::Ready(Some(Ok(event))) => {
                if matches!(event.kind, EventKind::Modify(_) | EventKind::Create(_)) {
                    let content = std::fs::read_to_string(&self.path);
                    match content {
                        Ok(content) => match toml::from_str::<Config>(&content) {
                            Ok(config) => {
                                if crate::ConfigValidator::validate(&config).is_ok() {
                                    Poll::Ready(Some(Ok(config)))
                                } else {
                                    Poll::Ready(Some(Err(ConfigError::HotReloadFailed(
                                        "配置验证失败".to_string(),
                                    ))))
                                }
                            },
                            Err(e) => Poll::Ready(Some(Err(ConfigError::HotReloadFailed(
                                format!("解析错误: {e}"),
                            )))),
                        },
                        Err(e) => Poll::Ready(Some(Err(ConfigError::HotReloadFailed(format!(
                            "读取文件错误: {e}",
                        ))))),
                    }
                } else {
                    Poll::Pending
                }
            },
            Poll::Ready(Some(Err(e))) => Poll::Ready(Some(Err(ConfigError::HotReloadFailed(
                format!("监视错误: {e}"),
            )))),
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

/// 文件配置源
///
/// 从单个 TOML 配置文件加载配置
pub struct FileConfigSource {
    /// Path to the configuration file.
    path: PathBuf,
}

impl FileConfigSource {
    /// 创建文件配置源
    ///
    /// # 参数
    ///
    /// * `path` - 配置文件路径
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// 从目录创建文件配置源
    ///
    /// 自动在指定目录中查找 `neoco.toml` 文件
    ///
    /// # 参数
    ///
    /// * `dir` - 配置文件所在目录
    pub fn from_dir(dir: impl Into<PathBuf>) -> Self {
        let dir = dir.into();
        let path = dir.join("neoco.toml");
        Self::new(path)
    }
}

impl ConfigSource for FileConfigSource {
    fn load(&self) -> Result<Config, ConfigError> {
        if !self.path.exists() {
            return Err(ConfigError::FileNotFound(self.path.clone()));
        }

        let content = std::fs::read_to_string(&self.path)?;
        let config: Config =
            toml::from_str(&content).map_err(|e| ConfigError::ParseError(e.to_string()))?;

        crate::ConfigValidator::validate(&config)?;
        Ok(config)
    }

    fn watch(
        &self,
    ) -> Result<Box<dyn Stream<Item = Result<Config, ConfigError>> + Send>, ConfigError> {
        use notify::EventKind;

        let (tx, rx) = mpsc::channel(100);
        let path = self.path.clone();

        let mut watcher = RecommendedWatcher::new(
            move |res: Result<Event, notify::Error>| {
                if let Ok(event) = res
                    && matches!(event.kind, EventKind::Modify(_) | EventKind::Create(_))
                {
                    let _ = tx.blocking_send(Ok(event));
                }
            },
            notify::Config::default(),
        )
        .map_err(|e| ConfigError::HotReloadFailed(format!("创建监视器失败: {e}")))?;

        watcher
            .watch(&path, RecursiveMode::NonRecursive)
            .map_err(|e| ConfigError::HotReloadFailed(format!("监视文件失败: {e}")))?;

        let watcher = ConfigWatcher::new(path, watcher, rx);
        Ok(Box::new(watcher))
    }
}

/// 多目录配置源
///
/// 从多个目录加载配置并自动合并
pub struct MultiDirConfigSource {
    /// Directories to search for configuration files.
    dirs: Vec<PathBuf>,
}

impl MultiDirConfigSource {
    /// 创建多目录配置源
    ///
    /// # 参数
    ///
    /// * `dirs` - 配置文件搜索目录列表
    #[must_use]
    pub fn new(dirs: Vec<PathBuf>) -> Self {
        Self { dirs }
    }
}

impl ConfigSource for MultiDirConfigSource {
    fn load(&self) -> Result<Config, ConfigError> {
        let loader = crate::ConfigLoader::with_dirs(self.dirs.clone());
        loader.load()
    }

    fn watch(
        &self,
    ) -> Result<Box<dyn Stream<Item = Result<Config, ConfigError>> + Send>, ConfigError> {
        Err(ConfigError::HotReloadFailed(
            "多目录配置源暂不支持热重载".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_file_config_source_not_found() {
        let source = FileConfigSource::new("/nonexistent/path.toml");
        let result = source.load();
        result.unwrap_err();
    }

    #[test]
    fn test_file_config_source_load() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("neoco.toml");

        let config_content = r#"
[model_groups.test]
models = [{ provider = "test", name = "gpt-4" }]

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
        fs::write(&config_path, config_content).unwrap();

        let source = FileConfigSource::new(config_path);
        let result = source.load();
        result.unwrap();
    }

    #[test]
    fn test_multi_dir_config_source() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("neoco.toml");

        let config_content = r#"
[model_groups.test]
models = [{ provider = "test", name = "gpt-4" }]

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
        fs::write(&config_path, config_content).unwrap();

        let source = MultiDirConfigSource::new(vec![temp_dir.path().to_path_buf()]);
        let result = source.load();
        result.unwrap();
    }
}
