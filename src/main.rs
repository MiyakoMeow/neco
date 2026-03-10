//! `NeoCo` - Multi-agent AI collaboration application

#![allow(unused_crate_dependencies)]

use async_trait::async_trait;
use clap::Parser;
use neoco_agent::engine::AgentDefinitionRepository;
use neoco_ui::{CliArgs, CliInterface};
use std::collections::HashMap;
use std::path::PathBuf;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Clone)]
#[allow(dead_code)]
/// Dummy agent definition repository for testing purposes.
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

/// Model information: (provider, `model_name`, params)
type ModelInfo = (String, String, HashMap<String, serde_json::Value>);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let args = CliArgs::parse();
    let working_dir = args.working_dir.clone();

    let config = neoco_ui::CliInterface::load_config(args.config.as_ref(), &working_dir, &args)?;

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    runtime.block_on(async { run_app(args, config, working_dir).await })
}

/// 运行应用程序
async fn run_app(
    args: CliArgs,
    config: neoco_config::Config,
    _working_dir: PathBuf,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(msg) = &args.message
        && msg.trim().is_empty()
    {
        eprintln!("error: 消息内容不能为空");
        std::process::exit(1);
    }

    if args.message.is_some() {
        run_cli_mode(args.clone(), config).await?;
    } else {
        run_tui_mode(args.clone(), config).await?;
    }

    Ok(())
}

/// 运行 CLI 模式
async fn run_cli_mode(
    args: CliArgs,
    config: neoco_config::Config,
) -> Result<(), Box<dyn std::error::Error>> {
    let cli = CliInterface::new(args, config);
    let exit_code = cli.run().await?;
    std::process::exit(exit_code);
}

/// 解析 TUI 模式的模型信息
fn resolve_model_for_tui(
    args: &CliArgs,
    config: &neoco_config::Config,
) -> Result<ModelInfo, Box<dyn std::error::Error>> {
    let mut params: HashMap<String, serde_json::Value> = HashMap::new();

    if let Some(group_name) = &args.model_group {
        let group = config
            .model_groups
            .get(group_name.as_str())
            .ok_or_else(|| format!("模型组 '{group_name}' 未在配置中找到"))?;

        if let Some(first_model) = group.models.first() {
            let model_params: HashMap<String, serde_json::Value> = first_model
                .params
                .iter()
                .map(|(k, v)| (k.to_string(), v.clone()))
                .collect();
            let mut merged = model_params;
            for (k, v) in params {
                merged.insert(k, v);
            }
            return Ok((
                first_model.provider.to_string(),
                first_model.name.to_string(),
                merged,
            ));
        }
        return Err(format!("模型组 '{group_name}' 中没有模型").into());
    }

    if let Some(model_ref) = &args.model {
        let (provider, name_with_params) = model_ref
            .split_once('/')
            .ok_or_else(|| "无效的模型格式: expected 'provider/name?params'".to_string())?;

        let (name, query_params) = if let Some((name, query)) = name_with_params.split_once('?') {
            let mut parsed_params: HashMap<String, serde_json::Value> = HashMap::new();
            for pair in query.split('&') {
                let (key, value) = pair
                    .split_once('=')
                    .ok_or_else(|| format!("无效的查询参数: {pair}"))?;
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

    if let Some(group_name) = &config.model_group {
        let group = config
            .model_groups
            .get(group_name.as_str())
            .ok_or_else(|| format!("模型组 '{group_name}' 未在配置中找到"))?;

        if let Some(first_model) = group.models.first() {
            let model_params: HashMap<String, serde_json::Value> = first_model
                .params
                .iter()
                .map(|(k, v)| (k.to_string(), v.clone()))
                .collect();
            let mut merged = model_params;
            for (k, v) in params {
                merged.insert(k, v);
            }
            return Ok((
                first_model.provider.to_string(),
                first_model.name.to_string(),
                merged,
            ));
        }
        return Err(format!("模型组 '{group_name}' 中没有模型").into());
    }

    if let Some(model_ref) = &config.model {
        let (provider, name_with_params) = model_ref
            .split_once('/')
            .ok_or_else(|| "无效的模型格式: expected 'provider/name?params'".to_string())?;

        let (name, query_params) = if let Some((name, query)) = name_with_params.split_once('?') {
            let mut parsed_params = std::collections::HashMap::new();
            for pair in query.split('&') {
                let (key, value) = pair
                    .split_once('=')
                    .ok_or_else(|| format!("无效的查询参数: {pair}"))?;
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

    let provider = std::env::var("NEOCO_MODEL_PROVIDER").unwrap_or_else(|_| "zhipu".to_string());
    Ok((provider, "glm-4".to_string(), params))
}

/// 运行 TUI 模式
#[allow(clippy::too_many_lines)]
async fn run_tui_mode(
    args: CliArgs,
    config: neoco_config::Config,
) -> Result<(), Box<dyn std::error::Error>> {
    use neoco_model::ModelClient;
    use neoco_ui::service::ServiceInitializer;
    use neoco_ui::tui::TuiInterface;
    use std::sync::Arc;

    let (provider, model_name, _model_params) = resolve_model_for_tui(&args, &config)?;

    let model_provider = config
        .model_providers
        .get(provider.as_str())
        .ok_or_else(|| format!("Model provider '{provider}' not found in config"))?;

    let client: Box<dyn ModelClient> = neoco_model::providers::build_client(
        model_provider,
        provider.as_str(),
        Some(model_name.as_str()),
    )
    .map_err(|e| format!("创建模型客户端失败: {e}"))?;

    let model_client: Arc<dyn ModelClient> = Arc::from(client);

    info!("使用模型: {provider}/{model_name}");

    let dummy_agent_def_repo: Arc<dyn AgentDefinitionRepository> = Arc::new(DummyAgentDefRepo);

    let (_service_ctx, engine) =
        ServiceInitializer::init(&config, model_client.clone(), dummy_agent_def_repo)
            .await
            .map_err(|e| format!("服务初始化失败: {e}"))?;

    let mut tui = TuiInterface::new(100, engine, None, None)?;
    tui.run().await?;
    Ok(())
}
