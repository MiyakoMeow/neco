//! Context tools.

use std::sync::Arc;
use std::sync::LazyLock;
use std::time::Duration;

use async_trait::async_trait;
use neoco_context::{
    CompressionService as CompressionServiceTrait, CompressionServiceImpl, ContextConfig,
    ContextObservation, ContextObserver as ContextObserverTrait, SimpleCounter,
};
use neoco_core::ids::ToolId;
use neoco_core::traits::MessageRepository;
use neoco_core::{
    AgentUlid, PromptLoader, ToolCapabilities, ToolCategory, ToolContext, ToolDefinition,
    ToolError, ToolExecutor, ToolOutput, ToolResult,
};
use neoco_model::client::ModelClient;
use serde_json::{Map, Value};

/// 创建上下文观察参数的 JSON Schema。
fn make_observe_schema() -> Value {
    let mut map = Map::new();
    map.insert("type".to_string(), Value::String("object".to_string()));
    map.insert(
        "$schema".to_string(),
        Value::String("https://json-schema.org/draft/2020-12/schema".to_string()),
    );
    let mut props = Map::new();
    let mut filter_prop = Map::new();
    filter_prop.insert("type".to_string(), Value::String("object".to_string()));
    filter_prop.insert(
        "description".to_string(),
        Value::String("可选的过滤条件".to_string()),
    );
    props.insert("filter".to_string(), Value::Object(filter_prop));
    map.insert("properties".to_string(), Value::Object(props));
    Value::Object(map)
}

/// 创建上下文压缩参数的 JSON Schema。
fn make_compact_schema() -> Value {
    let mut map = Map::new();
    map.insert("type".to_string(), Value::String("object".to_string()));
    map.insert(
        "$schema".to_string(),
        Value::String("https://json-schema.org/draft/2020-12/schema".to_string()),
    );
    let mut props = Map::new();
    let mut tag_prop = Map::new();
    tag_prop.insert("type".to_string(), Value::String("string".to_string()));
    tag_prop.insert(
        "description".to_string(),
        Value::String("压缩起点标记，从该标记到当前位置的消息将被压缩".to_string()),
    );
    props.insert("tag".to_string(), Value::Object(tag_prop));
    map.insert("properties".to_string(), Value::Object(props));
    Value::Object(map)
}

/// 上下文观察参数的 JSON Schema。
static OBSERVE_SCHEMA: LazyLock<Value> = LazyLock::new(make_observe_schema);
/// 上下文压缩参数的 JSON Schema。
static COMPACT_SCHEMA: LazyLock<Value> = LazyLock::new(make_compact_schema);

/// 上下文观察工具定义。
static OBSERVE_DEFINITION: LazyLock<ToolDefinition> = LazyLock::new(|| ToolDefinition {
    id: ToolId::from_string("context::observe").unwrap(),
    description: "观测上下文状态，获取内存使用仪表盘".to_string(),
    schema: OBSERVE_SCHEMA.clone(),
    capabilities: ToolCapabilities {
        streaming: false,
        requires_network: false,
        resource_level: neoco_core::ResourceLevel::Low,
        concurrent: false,
    },
    timeout: Duration::from_secs(5),
    category: ToolCategory::Common,
    prompt_component: Some("tool::context::observe".to_string()),
});

/// 上下文压缩工具定义。
static COMPACT_DEFINITION: LazyLock<ToolDefinition> = LazyLock::new(|| ToolDefinition {
    id: ToolId::from_string("context::compact").unwrap(),
    description: "主动压缩上下文，将历史消息压缩为摘要（Agent主动管理内存）".to_string(),
    schema: COMPACT_SCHEMA.clone(),
    capabilities: ToolCapabilities {
        streaming: false,
        requires_network: false,
        resource_level: neoco_core::ResourceLevel::Low,
        concurrent: false,
    },
    timeout: Duration::from_secs(30),
    category: ToolCategory::Common,
    prompt_component: Some("tool::context::compact".to_string()),
});

/// 上下文观察工具。
///
/// 此工具用于观测当前上下文的状态，获取内存使用等仪表盘信息。
pub struct ContextObserveTool {
    /// 上下文观察者实例
    observer: Arc<dyn ContextObserverTrait>,
}

impl ContextObserveTool {
    /// Create a new context observe tool.
    pub fn new(observer: Arc<dyn ContextObserverTrait>) -> Self {
        Self { observer }
    }
}

#[async_trait]
impl ToolExecutor for ContextObserveTool {
    fn definition(&self) -> &ToolDefinition {
        &OBSERVE_DEFINITION
    }

    async fn execute(&self, context: &ToolContext, args: Value) -> Result<ToolResult, ToolError> {
        let filter = args
            .get("filter")
            .and_then(|v| serde_json::from_value(v.clone()).ok());

        let observation = self
            .observer
            .observe(&context.agent_ulid, filter)
            .await
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

        let dashboard = format_observation(&observation);

        Ok(ToolResult {
            output: ToolOutput::Text(dashboard),
            is_error: false,
            prompt_component: None,
        })
    }
}

/// 格式化上下文观察结果为可读字符串。
fn format_observation(observation: &ContextObservation) -> String {
    let stats = &observation.stats;
    let usage_percent = stats.usage_percent * 100.0;
    let total_tokens = stats.total_tokens;
    let max_tokens = stats.estimated_turns_left * 2000 + total_tokens;
    let message_count = observation.messages.len();

    let pruning_status = stats.pruning_stage.map_or_else(
        || "None".to_string(),
        |s| {
            let base = s.to_string();
            match s {
                neoco_context::PruningStage::Stage1SoftTrim => format!("{base} approaching"),
                neoco_context::PruningStage::Stage2HardClear => format!("{base} active"),
                neoco_context::PruningStage::Stage3Graded => format!("{base} critical"),
            }
        },
    );

    let steps_since_tag = if let Some(ref tag) = stats.last_tag {
        format!("{} (last: '{tag}')", stats.steps_since_tag)
    } else {
        stats.steps_since_tag.to_string()
    };

    let format_tokens = |tokens: usize| -> String {
        if tokens >= 1024 {
            format!("{}k", tokens / 1024)
        } else {
            tokens.to_string()
        }
    };

    format!(
        "[Context Dashboard]\n• Usage:           {:.1}% ({}/{})\n• Message count:   {}\n• Steps since tag: {}\n• Pruning status:  {}\n• Est. turns left: ~{}",
        usage_percent,
        format_tokens(total_tokens),
        format_tokens(max_tokens),
        message_count,
        steps_since_tag,
        pruning_status,
        stats.estimated_turns_left
    )
}

/// 上下文压缩工具。
///
/// 此工具用于主动压缩上下文，将历史消息压缩为摘要，
/// 帮助 Agent 管理内存使用。
///
/// # 类型参数
///
/// * `S` - 压缩服务类型，必须实现 `CompressionServiceTrait`
pub struct ContextCompactTool<S: CompressionServiceTrait> {
    /// 压缩服务实例
    compression_service: Arc<S>,
}

impl<S: CompressionServiceTrait> ContextCompactTool<S> {
    /// Create a new context compact tool.
    pub fn new(compression_service: Arc<S>) -> Self {
        Self {
            compression_service,
        }
    }
}

#[async_trait]
impl<S: CompressionServiceTrait + 'static> ToolExecutor for ContextCompactTool<S> {
    fn definition(&self) -> &ToolDefinition {
        &COMPACT_DEFINITION
    }

    async fn execute(&self, context: &ToolContext, args: Value) -> Result<ToolResult, ToolError> {
        let tag = args.get("tag").and_then(|v| v.as_str());

        let summary = self
            .compression_service
            .compress(&context.agent_ulid, tag)
            .await
            .map_err(ToolError::ExecutionFailed)?;

        Ok(ToolResult {
            output: ToolOutput::Text(summary),
            is_error: false,
            prompt_component: None,
        })
    }
}

/// 用于测试的虚拟上下文观察者。
///
/// 此实现总是返回固定的模拟数据，用于单元测试和开发。
pub struct DummyContextObserver;

#[async_trait]
impl ContextObserverTrait for DummyContextObserver {
    async fn observe(
        &self,
        _agent_ulid: &AgentUlid,
        _filter: Option<neoco_context::ContextFilter>,
    ) -> Result<ContextObservation, neoco_context::ContextError> {
        Ok(ContextObservation {
            messages: vec![],
            stats: neoco_context::ContextStats::new(),
        })
    }
}

/// 用于测试的虚拟压缩服务。
///
/// 此实现总是返回固定的成功消息，用于单元测试和开发。
pub struct DummyCompressionService;

impl Default for DummyCompressionService {
    fn default() -> Self {
        Self
    }
}

#[async_trait]
impl neoco_context::CompressionService for DummyCompressionService {
    async fn compress(
        &self,
        _agent_ulid: &AgentUlid,
        _tag: Option<&str>,
    ) -> Result<String, String> {
        Ok("Context compressed successfully".to_string())
    }
}

/// Create a `CompressionServiceImpl` with the given dependencies.
pub fn create_compression_service(
    message_repo: Arc<dyn MessageRepository>,
    model_client: Arc<dyn ModelClient>,
    config: ContextConfig,
    prompt_loader: Arc<dyn PromptLoader>,
) -> Arc<CompressionServiceImpl> {
    let token_counter = Arc::new(SimpleCounter::new());
    Arc::new(CompressionServiceImpl::new(
        message_repo,
        token_counter,
        model_client,
        config,
        prompt_loader,
    ))
}
