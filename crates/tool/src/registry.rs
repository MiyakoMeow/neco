//! Tool registry implementation.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use neoco_core::{ToolDefinition, ToolExecutor, ToolId};
use tokio::sync::RwLock;

use crate::exec::ToolExecutorImpl;

pub use neoco_core::ToolRegistry;

/// 默认的工具注册表实现。
///
/// 此注册表管理所有可用的工具，提供注册、获取和查询功能。
/// 它使用读写锁来支持并发访问。
pub struct DefaultToolRegistry {
    /// 已注册的工具映射表，键为工具 ID，值为工具执行器
    tools: RwLock<HashMap<ToolId, Arc<dyn ToolExecutor>>>,
    /// 工具超时配置，键为工具名称前缀，值为超时时长
    timeouts: RwLock<HashMap<String, Duration>>,
}

impl Default for DefaultToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl DefaultToolRegistry {
    /// 创建一个新的空工具注册表。
    ///
    /// # 返回
    ///
    /// 返回一个没有任何注册工具的新注册表实例。
    pub fn new() -> Self {
        Self {
            tools: RwLock::new(HashMap::new()),
            timeouts: RwLock::new(HashMap::new()),
        }
    }

    /// 向注册表注册一个工具。
    ///
    /// # 参数
    ///
    /// * `tool` - 要注册的工具执行器
    pub async fn register(&self, tool: Arc<dyn ToolExecutor>) {
        let def = tool.definition();
        let id = def.id.clone();
        let wrapped = Arc::new(ToolExecutorImpl::new(tool)) as Arc<dyn ToolExecutor>;
        let mut tools = self.tools.write().await;
        tools.insert(id, wrapped);
    }

    /// 获取已包装的工具执行器。
    ///
    /// # 参数
    ///
    /// * `id` - 工具 ID
    ///
    /// # 返回
    ///
    /// 如果找到工具则返回包装后的执行器，否则返回 `None`。
    pub async fn get_wrapped(&self, id: &ToolId) -> Option<Arc<dyn ToolExecutor>> {
        self.tools.read().await.get(id).cloned()
    }

    /// 根据工具 ID 获取工具执行器。
    ///
    /// # 参数
    ///
    /// * `id` - 工具 ID
    ///
    /// # 返回
    ///
    /// 如果找到工具则返回执行器，否则返回 `None`。
    pub async fn get(&self, id: &ToolId) -> Option<Arc<dyn ToolExecutor>> {
        self.tools.read().await.get(id).cloned()
    }

    /// 获取所有已注册工具的定义。
    ///
    /// # 返回
    ///
    /// 返回所有已注册工具的定义列表。
    async fn definitions_impl(&self) -> Vec<ToolDefinition> {
        self.tools
            .read()
            .await
            .values()
            .map(|tool| tool.definition().clone())
            .collect()
    }

    /// 获取工具的超时配置。
    ///
    /// # 参数
    ///
    /// * `id` - 工具 ID
    ///
    /// # 返回
    ///
    /// 如果工具定义了超时则返回超时时长，否则返回配置的前缀匹配超时。
    async fn timeout_impl(&self, id: &ToolId) -> Option<Duration> {
        // 首先检查工具定义中的超时设置
        {
            let tools = self.tools.read().await;
            if let Some(tool) = tools.get(id) {
                return Some(tool.definition().timeout);
            }
        }

        // 使用前缀匹配和最长优先规则查找超时
        let tool_id_str = id.as_str();
        let timeouts = self.timeouts.read().await;

        // 收集所有匹配的前缀
        let mut matches: Vec<(&str, &Duration)> = timeouts
            .iter()
            .filter(|(prefix, _)| tool_id_str.starts_with(*prefix))
            .map(|(k, v)| (k.as_str(), v))
            .collect();

        // 按前缀长度降序排序（最长优先）
        matches.sort_by(|a, b| b.0.len().cmp(&a.0.len()));

        // 返回最长匹配的超时
        matches.first().copied().map(|(_, duration)| *duration)
    }

    /// 为工具名称前缀设置默认超时。
    ///
    /// # 参数
    ///
    /// * `prefix` - 工具名称前缀
    /// * `duration` - 超时时长
    async fn set_timeout_impl(&self, prefix: &str, duration: Duration) {
        self.timeouts
            .write()
            .await
            .insert(prefix.to_string(), duration);
    }

    /// 列出所有已注册的工具 ID。
    ///
    /// # 返回
    ///
    /// 返回所有已注册工具的 ID 列表。
    async fn list_tools_impl(&self) -> Vec<ToolId> {
        self.tools.read().await.keys().cloned().collect()
    }

    /// 从注册表中注销工具。
    ///
    /// # 参数
    ///
    /// * `id` - 工具 ID
    pub async fn unregister(&self, id: &ToolId) {
        let mut tools = self.tools.write().await;
        tools.remove(id);
    }
}

#[async_trait]
impl ToolRegistry for DefaultToolRegistry {
    async fn register(&self, tool: Arc<dyn ToolExecutor>) {
        self.register(tool).await;
    }

    async fn get(&self, id: &ToolId) -> Option<Arc<dyn ToolExecutor>> {
        self.get(id).await
    }

    async fn definitions(&self) -> Vec<ToolDefinition> {
        self.definitions_impl().await
    }

    async fn timeout(&self, id: &ToolId) -> Option<Duration> {
        self.timeout_impl(id).await
    }

    async fn set_timeout(&self, prefix: &str, duration: Duration) {
        self.set_timeout_impl(prefix, duration).await;
    }

    async fn list_tools(&self) -> Vec<ToolId> {
        self.list_tools_impl().await
    }

    async fn unregister(&self, id: &ToolId) {
        self.unregister(id).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    struct TestTool;

    #[async_trait]
    impl ToolExecutor for TestTool {
        fn definition(&self) -> &ToolDefinition {
            static DEF: std::sync::LazyLock<ToolDefinition> =
                std::sync::LazyLock::new(|| ToolDefinition {
                    id: ToolId::from_string("test::tool").unwrap(),
                    description: "Test tool".to_string(),
                    schema: serde_json::json!({"type": "object"}),
                    capabilities: neoco_core::ToolCapabilities::default(),
                    timeout: Duration::from_secs(5),
                    category: neoco_core::ToolCategory::Common,
                    prompt_component: None,
                });
            &DEF
        }

        async fn execute(
            &self,
            _context: &neoco_core::ToolContext,
            _args: serde_json::Value,
        ) -> Result<neoco_core::ToolResult, neoco_core::ToolError> {
            Ok(neoco_core::ToolResult {
                output: neoco_core::ToolOutput::Text("test result".to_string()),
                is_error: false,
                prompt_component: None,
            })
        }
    }

    #[tokio::test]
    async fn test_register_and_get() {
        let registry = DefaultToolRegistry::new();

        let tool = Arc::new(TestTool);
        registry.register(tool).await;

        let id = ToolId::from_string("test::tool").unwrap();
        let retrieved = registry.get(&id).await;
        assert!(retrieved.is_some());
    }
}
