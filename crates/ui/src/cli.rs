//! # 命令行界面模块
//!
//! 本模块提供 `NeoCo` 的命令行界面实现，支持直接发送消息。
//!
//! ## 主要组件
//!
//! - [`CliArgs`]：命令行参数解析
//! - [`CliInterface`]：CLI 接口实现

use crate::error::UiError;
use crate::service::ServiceInitializer;
use crate::traits::UserInterface;
use async_trait::async_trait;
use clap::Parser;
use neoco_agent::engine::AgentDefinitionRepository;
use neoco_core::SessionUlid;
use neoco_model::ModelClient;
use neoco_session::{SessionMetadata, SessionType};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::tui::TuiInterface;

/// Dummy agent definition repository that always returns None.
struct DummyAgentDefRepo;

#[async_trait]
impl AgentDefinitionRepository for DummyAgentDefRepo {
    async fn load_definition(
        &self,
        _: &str,
    ) -> Result<Option<neoco_agent::engine::AgentDefinition>, neoco_agent::AgentError> {
        Ok(None)
    }
}

/// 命令行参数
///
/// 解析和存储从命令行传入的所有参数，包括子命令、消息、模型选择等。
#[derive(Debug, Parser, Clone)]
#[command(name = "neoco")]
#[command(about = "NeoCo - 多智能体协作AI应用", long_about = None)]
#[command(version)]
pub struct CliArgs {
    /// 直接发送的消息内容（CLI 模式）
    #[arg(short = 'M', long, global = true, help = "直接发送消息（CLI模式）")]
    pub message: Option<String>,

    /// 模型引用 (格式: provider/name?params)
    #[arg(
        short,
        long,
        global = true,
        help = "指定模型 (格式: provider/name?temperature=0.1)"
    )]
    pub model: Option<String>,

    /// 模型组名称
    #[arg(
        long = "model-group",
        global = true,
        help = "指定模型组 (从配置中选择)"
    )]
    pub model_group: Option<String>,

    /// 指定的 Session ULID
    #[arg(short = 's', long, global = true, help = "指定Session ULID")]
    pub session: Option<SessionUlid>,

    /// 配置文件路径
    #[arg(short = 'c', long, global = true, help = "配置文件路径")]
    pub config: Option<PathBuf>,

    /// 工作目录路径
    #[arg(
        short = 'w',
        long,
        global = true,
        default_value = ".",
        help = "工作目录"
    )]
    pub working_dir: PathBuf,
}

/// CLI 接口实现
///
/// 负责处理命令行模式的用户交互，包括参数解析和消息发送。
pub struct CliInterface {
    /// 命令行参数
    args: CliArgs,
    /// 应用配置
    #[allow(dead_code)]
    config: neoco_config::Config,
}

impl CliInterface {
    /// 创建新的 CLI 接口实例
    ///
    /// # 参数
    ///
    /// * `args` - 解析后的命令行参数
    /// * `config` - 应用配置
    #[must_use]
    pub fn new(args: CliArgs, config: neoco_config::Config) -> Self {
        Self { args, config }
    }

    /// 运行 CLI 接口
    ///
    /// 根据命令行参数执行相应的操作：
    /// - 如果指定了消息参数，运行 Agent 模式
    ///
    /// # Errors
    ///
    /// 如果消息内容为空，返回错误
    ///
    /// # 返回
    ///
    /// 返回退出码，0 表示成功，非 0 表示错误
    pub async fn run(&self) -> Result<i32, UiError> {
        if let Some(msg) = &self.args.message
            && msg.trim().is_empty()
        {
            return Err(UiError::BadRequest("消息内容不能为空".to_string()));
        }

        if let Some(msg) = &self.args.message {
            return self.run_agent_mode(msg).await;
        }

        let mut tui = self.init_tui().await?;
        tui.init().await?;
        tui.run().await?;
        Ok(0)
    }

    /// 解析并返回模型信息
    ///
    /// 优先级：
    /// 1. CLI 参数 --model-group
    /// 2. CLI 参数 --model
    /// 3. config 中的 `model_group`
    /// 4. config 中的 model
    /// 5. 默认值
    fn resolve_model(
        &self,
    ) -> Result<
        (
            String,
            String,
            std::collections::HashMap<String, serde_json::Value>,
        ),
        UiError,
    > {
        let mut params = std::collections::HashMap::new();

        if let Some(group_name) = &self.args.model_group {
            let group = self
                .config
                .model_groups
                .get(group_name.as_str())
                .ok_or_else(|| {
                    UiError::BadRequest(format!("模型组 '{group_name}' 未在配置中找到"))
                })?;

            if let Some(first_model) = group.models.first() {
                let mut model_params: std::collections::HashMap<String, serde_json::Value> =
                    first_model
                        .params
                        .iter()
                        .map(|(k, v)| (k.to_string(), v.clone()))
                        .collect();
                for (k, v) in params {
                    model_params.insert(k, v);
                }
                return Ok((
                    first_model.provider.to_string(),
                    first_model.name.to_string(),
                    model_params,
                ));
            }
            return Err(UiError::BadRequest(format!(
                "模型组 '{group_name}' 中没有模型"
            )));
        }

        if let Some(model_ref) = &self.args.model {
            let (provider, name_with_params) = model_ref.split_once('/').ok_or_else(|| {
                UiError::BadRequest("无效的模型格式: expected 'provider/name?params'".to_string())
            })?;

            let (name, query_params) = if let Some((name, query)) = name_with_params.split_once('?')
            {
                let mut parsed_params = std::collections::HashMap::new();
                for pair in query.split('&') {
                    let (key, value) = pair
                        .split_once('=')
                        .ok_or_else(|| UiError::BadRequest(format!("无效的查询参数: {pair}")))?;
                    parsed_params.insert(
                        key.to_string(),
                        serde_json::Value::String(value.to_string()),
                    );
                }
                (name.to_string(), Some(parsed_params))
            } else {
                (name_with_params.to_string(), None)
            };

            if let Some(p) = query_params {
                params = p;
            }

            return Ok((provider.to_string(), name, params));
        }

        if let Some(group_name) = &self.config.model_group {
            let group = self
                .config
                .model_groups
                .get(group_name.as_str())
                .ok_or_else(|| {
                    UiError::BadRequest(format!("模型组 '{group_name}' 未在配置中找到"))
                })?;

            if let Some(first_model) = group.models.first() {
                let mut model_params: std::collections::HashMap<String, serde_json::Value> =
                    first_model
                        .params
                        .iter()
                        .map(|(k, v)| (k.to_string(), v.clone()))
                        .collect();
                for (k, v) in params {
                    model_params.insert(k, v);
                }
                return Ok((
                    first_model.provider.to_string(),
                    first_model.name.to_string(),
                    model_params,
                ));
            }
            return Err(UiError::BadRequest(format!(
                "模型组 '{group_name}' 中没有模型"
            )));
        }

        if let Some(model_ref) = &self.config.model {
            let (provider, name_with_params) = model_ref.split_once('/').ok_or_else(|| {
                UiError::BadRequest("无效的模型格式: expected 'provider/name?params'".to_string())
            })?;

            let (name, query_params) = if let Some((name, query)) = name_with_params.split_once('?')
            {
                let mut parsed_params = std::collections::HashMap::new();
                for pair in query.split('&') {
                    let (key, value) = pair
                        .split_once('=')
                        .ok_or_else(|| UiError::BadRequest(format!("无效的查询参数: {pair}")))?;
                    parsed_params.insert(
                        key.to_string(),
                        serde_json::Value::String(value.to_string()),
                    );
                }
                (name.to_string(), Some(parsed_params))
            } else {
                (name_with_params.to_string(), None)
            };

            if let Some(p) = query_params {
                params = p;
            }

            return Ok((provider.to_string(), name, params));
        }

        Ok(("zhipu".to_string(), "glm-4".to_string(), params))
    }

    /// 运行 Agent 模式
    #[allow(clippy::print_stdout)]
    async fn run_agent_mode(&self, message: &str) -> Result<i32, UiError> {
        let (provider, model_name, _model_params) = self.resolve_model()?;

        let client: Box<dyn ModelClient> = {
            let model_provider = self
                .config
                .model_providers
                .get(provider.as_str())
                .ok_or_else(|| {
                    UiError::BadRequest(format!("Model provider '{provider}' not found in config"))
                })?;
            neoco_model::providers::build_client(
                model_provider,
                provider.as_str(),
                Some(model_name.as_str()),
            )
            .map_err(|e| UiError::Internal(format!("创建模型客户端失败: {e}")))?
        };

        let model_client: Arc<dyn ModelClient> = Arc::from(client);

        println!("[INFO] 使用模型: {provider}/{model_name}");

        let dummy_agent_def_repo: Arc<dyn AgentDefinitionRepository> = Arc::new(DummyAgentDefRepo);

        let (service_ctx, engine) =
            ServiceInitializer::init(&self.config, model_client.clone(), dummy_agent_def_repo)
                .await
                .map_err(|e| UiError::Internal(format!("服务初始化失败: {e}")))?;

        let (session_id, root_agent_id) = if let Some(session_ulid) = &self.args.session {
            match self.load_session(session_ulid).await {
                Ok(session) => {
                    let root_agent_id = *session.root_agent_ulid();
                    println!("已恢复会话: {session_ulid}");
                    (*session_ulid, root_agent_id)
                },
                Err(e) => {
                    return Err(UiError::BadRequest(format!(
                        "无法恢复会话 {session_ulid}: {e}"
                    )));
                },
            }
        } else {
            let working_dir = self.args.working_dir.clone();
            let session = service_ctx
                .session_manager
                .create_session(
                    SessionType::Direct {
                        initial_message: Some(message.to_string()),
                    },
                    SessionMetadata {
                        working_dir,
                        ..Default::default()
                    },
                )
                .await
                .map_err(|e| UiError::Internal(format!("创建Session失败: {e}")))?;
            let session_id = *session.id();
            let root_agent_id = *session.root_agent_ulid();
            println!("创建新会话: {session_id}");
            (session_id, root_agent_id)
        };

        match engine.run_agent(root_agent_id, message.to_string()).await {
            Ok(result) => {
                println!("{}", result.output);
                println!();
                println!("[INFO] 使用以下命令接续对话:");
                println!("  --session {session_id}");
            },
            Err(e) => {
                return Err(UiError::Internal(format!("Agent执行失败: {e}")));
            },
        }

        Ok(0)
    }

    /// 初始化 TUI
    ///
    /// 创建并初始化 TUI 接口实例。
    ///
    /// # Errors
    ///
    /// 如果初始化失败，返回错误
    ///
    /// # 返回
    ///
    /// 返回 TUI 接口实例或错误
    async fn init_tui(&self) -> Result<TuiInterface, UiError> {
        let (provider, model_name, _model_params) = self.resolve_model()?;

        let client: Box<dyn ModelClient> = {
            let model_provider = self
                .config
                .model_providers
                .get(provider.as_str())
                .ok_or_else(|| {
                    UiError::BadRequest(format!("Model provider '{provider}' not found in config"))
                })?;
            neoco_model::providers::build_client(
                model_provider,
                provider.as_str(),
                Some(model_name.as_str()),
            )
            .map_err(|e| UiError::Internal(format!("创建模型客户端失败: {e}")))?
        };

        let model_client: Arc<dyn ModelClient> = Arc::from(client);

        let dummy_agent_def_repo: Arc<dyn AgentDefinitionRepository> = Arc::new(DummyAgentDefRepo);

        let (service_ctx, engine) =
            ServiceInitializer::init(&self.config, model_client.clone(), dummy_agent_def_repo)
                .await
                .map_err(|e| UiError::Internal(format!("服务初始化失败: {e}")))?;

        let restore_session = if let Some(session_ulid) = &self.args.session {
            #[allow(clippy::print_stdout)]
            let result = match self.load_session(session_ulid).await {
                Ok(session) => {
                    let root_agent_id = *session.root_agent_ulid();
                    println!("已恢复会话: {session_ulid}");
                    Some((*session_ulid, root_agent_id))
                },
                Err(e) => {
                    return Err(UiError::BadRequest(format!(
                        "无法恢复会话 {session_ulid}: {e}"
                    )));
                },
            };
            result
        } else {
            None
        };

        TuiInterface::new(1000, engine, service_ctx.context_manager, restore_session)
            .map_err(|e| UiError::Internal(format!("创建TUI失败: {e}")))
    }

    /// 加载配置文件
    ///
    /// 根据命令行参数加载应用配置：
    /// - 如果提供了 `--config` 参数，直接加载该文件
    /// - 否则按优先级合并以下配置文件：
    ///   1. `{working_dir}/.neoco/neoco.toml`
    ///   2. `{working_dir}/.agents/neoco.toml`
    ///   3. `~/.config/neoco/neoco.toml`
    ///   4. `~/.agents/neoco.toml`
    ///
    /// CLI 参数（如 --model, --working-dir）会被合并到配置中，优先级高于配置文件
    ///
    /// # 参数
    ///
    /// * `config_path` - 可选的配置文件路径（来自 -c 参数）
    /// * `working_dir` - 工作目录路径
    /// * `cli_args` - 命令行参数（用于合并到配置中）
    ///
    /// # Errors
    ///
    /// 如果配置加载失败，返回错误
    ///
    /// # 返回
    ///
    /// 返回加载并合并了 CLI 参数的配置或错误
    #[allow(clippy::print_stdout)]
    pub fn load_config(
        config_path: Option<&PathBuf>,
        working_dir: &Path,
        cli_args: &CliArgs,
    ) -> Result<neoco_config::Config, UiError> {
        let config = if let Some(path) = config_path {
            if !path.exists() {
                return Err(UiError::BadRequest(format!(
                    "配置文件未找到: {}",
                    path.display()
                )));
            }
            println!("[INFO] Loading config from: {}", path.display());
            if path.is_dir() {
                let loader = neoco_config::ConfigLoader::with_dirs(vec![path.clone()]);
                let result = loader.load().map_err(UiError::Config)?;
                println!("[INFO] Config loaded successfully (single file)");
                result
            } else {
                let parent = path
                    .parent()
                    .ok_or_else(|| UiError::BadRequest("无效的配置文件路径".to_string()))?;
                let loader = neoco_config::ConfigLoader::with_dirs(vec![parent.to_path_buf()]);
                let result = loader.load().map_err(UiError::Config)?;
                println!("[INFO] Config loaded successfully (single file)");
                result
            }
        } else {
            let default_dirs = neoco_config::ConfigLoader::default_config_dirs();
            let dirs: Vec<PathBuf> = default_dirs
                .iter()
                .map(|d| {
                    if d.is_relative() {
                        working_dir.join(d)
                    } else {
                        d.clone()
                    }
                })
                .collect();

            println!("[INFO] Merging config files:");
            for (i, dir) in dirs.iter().enumerate() {
                println!("[INFO]   - {} (priority {})", dir.display(), i + 1);
            }

            let loader = neoco_config::ConfigLoader::with_dirs(dirs);
            let result = loader.load().map_err(UiError::Config)?;
            println!("[INFO] Config merged successfully");
            result
        };

        let cli_override = Self::create_cli_override_config(working_dir, cli_args);
        let merged_config = neoco_config::ConfigMerger::merge(&config, &cli_override);

        Ok(merged_config)
    }

    /// 根据 CLI 参数创建覆盖配置
    ///
    /// 将 CLI 参数转换为配置结构，用于覆盖配置文件中的值
    fn create_cli_override_config(working_dir: &Path, _cli_args: &CliArgs) -> neoco_config::Config {
        let system = neoco_config::SystemConfig {
            storage: neoco_config::StorageConfig {
                session_dir: working_dir.to_path_buf(),
                ..Default::default()
            },
            ..Default::default()
        };

        neoco_config::Config {
            system,
            ..Default::default()
        }
    }

    /// Load a session from storage.
    async fn load_session(
        &self,
        session_ulid: &SessionUlid,
    ) -> Result<neoco_session::Session, UiError> {
        let storage = neoco_session::storage::FileStorage::new(self.args.working_dir.clone());

        let session_manager = neoco_session::SessionManager::new(Arc::new(storage));

        session_manager
            .load_session(session_ulid)
            .await
            .map_err(|e| UiError::Session(neoco_session::SessionError::Storage(e.to_string())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_args_parsing() {
        let args = CliArgs::parse_from(["neoco", "-M", "hello"]);
        assert_eq!(args.message, Some("hello".to_string()));
    }

    #[test]
    fn test_cli_args_session() {
        let args = CliArgs::parse_from(["neoco", "-s", "01HF8X5JQC8ZXJ3YKZ0J9K2D9Z"]);
        assert!(args.session.is_some());
    }

    #[test]
    fn test_cli_args_working_dir() {
        let args = CliArgs::parse_from(["neoco", "-w", "/tmp"]);
        assert_eq!(args.working_dir, PathBuf::from("/tmp"));
    }

    #[test]
    fn test_cli_args_empty_message() {
        let result = CliArgs::try_parse_from(["neoco", "-M", ""]);
        result.unwrap();
    }

    #[test]
    fn test_cli_args_config_path() {
        let args = CliArgs::parse_from(["neoco", "-c", "/path/to/config.toml"]);
        assert_eq!(args.config, Some(PathBuf::from("/path/to/config.toml")));
    }

    #[test]
    fn test_cli_args_model_zhipu() {
        let args = CliArgs::parse_from(["neoco", "-M", "hello", "--model", "zhipu/glm-4"]);
        assert_eq!(args.model, Some("zhipu/glm-4".to_string()));
        assert_eq!(args.message, Some("hello".to_string()));
    }

    #[test]
    fn test_cli_args_model_minimax() {
        let args =
            CliArgs::try_parse_from(["neoco", "-M", "hello", "--model", "minimax-cn/MiniMax-M2.5"])
                .unwrap();
        assert_eq!(args.model, Some("minimax-cn/MiniMax-M2.5".to_string()));
    }

    #[test]
    fn test_cli_args_model_group() {
        let args =
            CliArgs::try_parse_from(["neoco", "-M", "hello", "--model-group", "balanced"]).unwrap();
        assert_eq!(args.model_group, Some("balanced".to_string()));
    }
}
