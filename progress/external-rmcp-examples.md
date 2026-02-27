# RMCP 实用示例和最佳实践

## 目录

1. [完整服务器示例](#完整服务器示例)
2. [完整客户端示例](#完整客户端示例)
3. [高级模式](#高级模式)
4. [测试策略](#测试策略)
5. [性能优化](#性能优化)
6. [安全性最佳实践](#安全性最佳实践)
7. [部署建议](#部署建议)

---

## 完整服务器示例

### 文件系统服务器

```rust
use rmcp::{
    ServerHandler, ServiceExt,
    handler::server::{
        tool::{ToolRouter, tool, tool_router, tool_handler},
        resource::{ResourceHandler, resource_handler},
    },
    model::*,
    transport::stdio,
    ErrorData as McpError,
};
use std::{path::Path, sync::Arc};
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct FileSystemServer {
    base_path: Arc<Path>,
    tool_router: ToolRouter<Self>,
}

impl FileSystemServer {
    pub fn new(base_path: String) -> Result<Self, std::io::Error> {
        let path = Path::new(&base_path).canonicalize()?;
        Ok(Self {
            base_path: Arc::new(path),
            tool_router: Self::tool_router(),
        })
    }
}

#[tool_router]
impl FileSystemServer {
    /// 读取文件内容
    #[tool(description = "Read the contents of a file")]
    async fn read_file(
        &self,
        #[tool(arg)] path: String,
    ) -> Result<CallToolResult, McpError> {
        // 安全检查：防止路径穿越攻击
        let full_path = self.safe_path(&path)?;

        // 读取文件
        let content = tokio::fs::read_to_string(&full_path)
            .await
            .map_err(|e| McpError {
                code: ErrorCode::InternalError,
                message: format!("Failed to read file: {}", e),
                data: None,
            })?;

        Ok(CallToolResult::success(vec![
            Content::text(content)
        ]))
    }

    /// 写入文件
    #[tool(description = "Write content to a file")]
    async fn write_file(
        &self,
        #[tool(arg)] path: String,
        #[tool(arg)] content: String,
    ) -> Result<CallToolResult, McpError> {
        let full_path = self.safe_path(&path)?;

        // 确保父目录存在
        if let Some(parent) = full_path.parent() {
            tokio::fs::create_dir_all(parent).await
                .map_err(|e| McpError {
                    code: ErrorCode::InternalError,
                    message: format!("Failed to create directory: {}", e),
                    data: None,
                })?;
        }

        // 写入文件
        tokio::fs::write(&full_path, content)
            .await
            .map_err(|e| McpError {
                code: ErrorCode::InternalError,
                message: format!("Failed to write file: {}", e),
                data: None,
            })?;

        Ok(CallToolResult::success(vec![
            Content::text(format!("File written: {}", path))
        ]))
    }

    /// 列出目录
    #[tool(description = "List files in a directory")]
    async fn list_directory(
        &self,
        #[tool(arg)] path: String,
    ) -> Result<CallToolResult, McpError> {
        let full_path = self.safe_path(&path)?;

        let mut entries = tokio::fs::read_dir(&full_path)
            .await
            .map_err(|e| McpError {
                code: ErrorCode::InternalError,
                message: format!("Failed to read directory: {}", e),
                data: None,
            })?;

        let mut result = Vec::new();
        while let Some(entry) = entries.next_entry().await
            .map_err(|e| McpError {
                code: ErrorCode::InternalError,
                message: format!("Failed to read entry: {}", e),
                data: None,
            })?
        {
            let name = entry.file_name().to_string_lossy().to_string();
            let file_type = entry.file_type().await
                .map_err(|e| McpError {
                    code: ErrorCode::InternalError,
                    message: format!("Failed to get file type: {}", e),
                    data: None,
                })?;

            let type_str = if file_type.is_dir() { "DIR" } else { "FILE" };
            result.push(format!("{}: {}", type_str, name));
        }

        Ok(CallToolResult::success(vec![
            Content::text(result.join("\n"))
        ]))
    }

    // 安全路径解析
    fn safe_path(&self, path: &str) -> Result<std::path::PathBuf, McpError> {
        let full_path = self.base_path.join(path);

        // 规范化路径并检查是否在基础路径内
        let canonical = full_path.canonicalize()
            .map_err(|e| McpError {
                code: ErrorCode::InternalError,
                message: format!("Invalid path: {}", e),
                data: None,
            })?;

        if !canonical.starts_with(&*self.base_path) {
            return Err(McpError {
                code: ErrorCode::InvalidParams,
                message: "Path traversal detected".into(),
                data: None,
            });
        }

        Ok(canonical)
    }
}

#[tool_handler]
impl ServerHandler for FileSystemServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            name: "filesystem-server".into(),
            version: "1.0.0".into(),
            instructions: Some(
                "A secure file system server with path traversal protection".into()
            ),
            capabilities: ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .build(),
            ..Default::default()
        }
    }
}

#[resource_handler]
impl ResourceHandler for FileSystemServer {
    async fn list_resources(
        &self,
        _request: ListResourcesRequestParams,
    ) -> Result<ListResourcesResult, McpError> {
        Ok(ListResourcesResult {
            resources: vec![
                Resource {
                    uri: "file://README.md".into(),
                    name: "README".into(),
                    description: Some("Project README".into()),
                    mime_type: Some("text/markdown".into()),
                    ..Default::default()
                }
            ],
            ..Default::default()
        })
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
    ) -> Result<ReadResourceResult, McpError> {
        // 从 URI 中提取路径
        let path = request.uri.strip_prefix("file://")
            .ok_or_else(|| McpError {
                code: ErrorCode::InvalidParams,
                message: "Invalid URI scheme".into(),
                data: None,
            })?;

        let full_path = self.safe_path(path)?;

        let content = tokio::fs::read_to_string(&full_path)
            .await
            .map_err(|e| McpError {
                code: ErrorCode::InternalError,
                message: format!("Failed to read: {}", e),
                data: None,
            })?;

        Ok(ReadResourceResult {
            contents: vec![
                ResourceContents {
                    uri: request.uri.clone(),
                    mime_type: Some("text/plain".into()),
                    text: Some(content),
                    ..Default::default()
                }
            ],
        })
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let server = FileSystemServer::new("/allowed/path")?;

    let service = server.serve(stdio()).await?;
    service.waiting().await?;

    Ok(())
}
```

---

## 完整客户端示例

### MCP 集成客户端

```rust
use rmcp::{
    ServiceExt,
    model::*,
    transport::{TokioChildProcess, ConfigureCommandExt},
};
use std::{collections::HashMap, sync::Arc};
use tokio::{process::Command, sync::RwLock};

#[derive(Clone)]
pub struct McpClient {
    name: String,
    service: DynClient,
    tools: Arc<RwLock<HashMap<String, Tool>>>,
}

impl McpClient {
    /// 创建新的客户端连接
    pub async fn new(name: String, command: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let parts: Vec<&str> = command.split_whitespace().collect();
        let mut cmd = Command::new(parts[0]);

        for arg in &parts[1..] {
            cmd.arg(arg);
        }

        let service = ()
            .serve(TokioChildProcess::new(cmd)?)
            .await?
            .into_dyn();

        // 初始化时获取所有工具
        let tools_response = service.list_tools(Default::default()).await?;
        let mut tools_map = HashMap::new();

        for tool in tools_response.tools {
            tools_map.insert(tool.name.clone(), tool);
        }

        Ok(Self {
            name,
            service,
            tools: Arc::new(RwLock::new(tools_map)),
        })
    }

    /// 获取所有可用工具
    pub async fn list_tools(&self) -> Vec<Tool> {
        self.tools.read().await.values().cloned().collect()
    }

    /// 获取特定工具信息
    pub async fn get_tool(&self, name: &str) -> Option<Tool> {
        self.tools.read().await.get(name).cloned()
    }

    /// 调用工具（带重试）
    pub async fn call_tool_with_retry(
        &self,
        name: String,
        arguments: Option<serde_json::Value>,
        max_retries: u32,
    ) -> Result<CallToolResult, Box<dyn std::error::Error>> {
        let mut last_error = None;

        for attempt in 0..max_retries {
            match self.call_tool(name.clone(), arguments.clone()).await {
                Ok(result) => {
                    // 检查是否是错误结果
                    if result.is_error.unwrap_or(false) {
                        last_error = Some(format!("Tool returned error: {:?}", result).into());
                    } else {
                        return Ok(result);
                    }
                }
                Err(e) => {
                    last_error = Some(e);
                }
            }

            if attempt < max_retries - 1 {
                tokio::time::sleep(tokio::time::Duration::from_millis(100 * (attempt + 1) as u64)).await;
            }
        }

        Err(last_error.unwrap_or_else(|| "Max retries exceeded".into()))
    }

    /// 调用工具（带超时）
    pub async fn call_tool_with_timeout(
        &self,
        name: String,
        arguments: Option<serde_json::Value>,
        timeout_secs: u64,
    ) -> Result<CallToolResult, Box<dyn std::error::Error>> {
        let call = self.call_tool(name, arguments);

        tokio::time::timeout(
            tokio::time::Duration::from_secs(timeout_secs),
            call
        )
        .await
        .map_err(|_| "Tool call timed out".into())?
    }

    /// 调用工具
    pub async fn call_tool(
        &self,
        name: String,
        arguments: Option<serde_json::Value>,
    ) -> Result<CallToolResult, Box<dyn std::error::Error>> {
        let result = self.service
            .call_tool(CallToolRequestParams {
                meta: None,
                name,
                arguments,
                task: None,
            })
            .await?;

        Ok(result)
    }

    /// 获取服务器信息
    pub async fn get_server_info(&self) -> ServerInfo {
        self.service.peer_info()
    }

    /// 列出资源
    pub async fn list_resources(&self) -> Result<Vec<Resource>, Box<dyn std::error::Error>> {
        let response = self.service.list_resources(Default::default()).await?;
        Ok(response.resources)
    }

    /// 读取资源
    pub async fn read_resource(&self, uri: String) -> Result<ResourceContents, Box<dyn std::error::Error>> {
        let response = self.service
            .read_resource(ReadResourceRequestParams {
                uri,
                ..Default::default()
            })
            .await?;

        response.contents.into_iter().next()
            .ok_or_else(|| "No content found".into())
    }
}

/// 多服务器管理器
pub struct McpManager {
    clients: Arc<RwLock<HashMap<String, Arc<McpClient>>>>,
}

impl McpManager {
    pub fn new() -> Self {
        Self {
            clients: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 添加服务器（懒加载）
    pub async fn get_or_create_client(
        &self,
        name: String,
        command: String,
    ) -> Result<Arc<McpClient>, Box<dyn std::error::Error>> {
        {
            let readers = self.clients.read().await;
            if let Some(client) = readers.get(&name) {
                return Ok(client.clone());
            }
        }

        {
            let mut writers = self.clients.write().await;
            // 再次检查，防止竞态条件
            if let Some(client) = writers.get(&name) {
                return Ok(client.clone());
            }

            let client = Arc::new(McpClient::new(name.clone(), &command).await?);
            writers.insert(name.clone(), client.clone());

            Ok(client)
        }
    }

    /// 在所有服务器上搜索工具
    pub async fn find_tool(&self, tool_name: &str) -> Vec<(String, Tool)> {
        let mut results = Vec::new();
        let readers = self.clients.read().await;

        for (server_name, client) in readers.iter() {
            if let Some(tool) = client.get_tool(tool_name).await {
                results.push((server_name.clone(), tool));
            }
        }

        results
    }

    /// 调用工具（自动查找服务器）
    pub async fn call_tool_auto(
        &self,
        tool_name: String,
        arguments: Option<serde_json::Value>,
    ) -> Result<CallToolResult, Box<dyn std::error::Error>> {
        let servers = self.find_tool(&tool_name).await;

        if servers.is_empty() {
            return Err(format!("Tool '{}' not found on any server", tool_name).into());
        }

        if servers.len() > 1 {
            return Err(format!(
                "Tool '{}' found on multiple servers: {:?}. Please specify server.",
                tool_name,
                servers.iter().map(|(n, _)| n).collect::<Vec<_>>()
            ).into());
        }

        let (server_name, _tool) = &servers[0];
        let client = self.get_or_create_client(server_name.clone(), String::new()).await?;
        client.call_tool(tool_name, arguments).await
    }

    /// 获取所有服务器的所有工具
    pub async fn list_all_tools(&self) -> HashMap<String, Vec<Tool>> {
        let mut all_tools = HashMap::new();
        let readers = self.clients.read().await;

        for (server_name, client) in readers.iter() {
            let tools = client.list_tools().await;
            all_tools.insert(server_name.clone(), tools);
        }

        all_tools
    }
}

impl Default for McpManager {
    fn default() -> Self {
        Self::new()
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manager = McpManager::new();

    // 添加文件系统服务器
    let fs_client = manager.get_or_create_client(
        "filesystem".into(),
        "uvx mcp-server-filesystem /tmp/allowed".into()
    ).await?;

    // 列出可用工具
    let tools = fs_client.list_tools().await;
    println!("Available tools:");
    for tool in tools {
        println!("  - {}: {}", tool.name, tool.description.as_deref().unwrap_or("No description"));
    }

    // 调用工具
    let result = fs_client.call_tool(
        "read_file".into(),
        Some(serde_json::json!({
            "path": "test.txt"
        }))
    ).await?;

    println!("Result: {:#?}", result);

    Ok(())
}
```

---

## 高级模式

### 1. 流式工具实现

```rust
use futures::Stream;

#[tool(description = "Stream large file content")]
async fn stream_file(
    &self,
    #[tool(arg)] path: String,
) -> Result<impl Stream<Item = String>, McpError> {
    let file = tokio::fs::File::open(path).await?;

    Ok(async_stream::stream! {
        let reader = tokio::io::BufReader::new(file);
        use tokio::io::AsyncBufReadExt;
        let mut lines = reader.lines();

        while let Some(line) = lines.next_line().await.unwrap() {
            yield line;
        }
    })
}
```

### 2. 批量工具调用

```rust
impl McpManager {
    pub async fn call_tools_batch(
        &self,
        calls: Vec<(String, String, Option<serde_json::Value>)>,
    ) -> Vec<Result<CallToolResult, Box<dyn std::error::Error>>> {
        let futures = calls.into_iter().map(|(server, tool, args)| {
            let manager = self.clone();
            async move {
                manager.call_tool_on_server(&server, tool, args).await
            }
        });

        futures::future::join_all(futures).await
    }
}
```

### 3. 工具链（Pipeline）

```rust
pub struct ToolChain {
    steps: Vec<ChainStep>,
}

pub struct ChainStep {
    server: String,
    tool: String,
    // 前一步的输出如何映射到这一步的输入
    input_mapping: HashMap<String, String>,
}

impl ToolChain {
    pub async fn execute(
        &self,
        manager: &McpManager,
        initial_input: serde_json::Value,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        let mut current_output = initial_input;

        for step in &self.steps {
            let args = self.map_input(&current_output, &step.input_mapping)?;
            let result = manager.call_tool_on_server(
                &step.server,
                step.tool.clone(),
                Some(args)
            ).await?;

            current_output = self.extract_output(result)?;
        }

        Ok(current_output)
    }
}
```

### 4. 缓存层

```rust
use lru::LruCache;
use std::num::NonZeroUsize;

pub struct CachedMcpClient {
    inner: Arc<McpClient>,
    cache: Arc<Mutex<LruCache<String, CallToolResult>>>,
}

impl CachedMcpClient {
    pub fn new(inner: Arc<McpClient>, cache_size: usize) -> Self {
        Self {
            inner,
            cache: Arc::new(Mutex::new(LruCache::new(
                NonZeroUsize::new(cache_size).unwrap()
            ))),
        }
    }

    pub async fn call_tool_cached(
        &self,
        name: String,
        arguments: Option<serde_json::Value>,
    ) -> Result<CallToolResult, Box<dyn std::error::Error>> {
        // 生成缓存键
        let cache_key = format!("{}:{:?}", name, arguments);

        // 检查缓存
        {
            let mut cache = self.cache.lock().await;
            if let Some(result) = cache.get(&cache_key) {
                return Ok(result.clone());
            }
        }

        // 调用工具
        let result = self.inner.call_tool(name, arguments).await?;

        // 更新缓存
        {
            let mut cache = self.cache.lock().await;
            cache.put(cache_key, result.clone());
        }

        Ok(result)
    }
}
```

---

## 测试策略

### 单元测试工具

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_safe_path() {
        let server = FileSystemServer::new("/tmp/test").unwrap();

        // 有效路径
        assert!(server.safe_path("file.txt").is_ok());

        // 路径穿越攻击
        assert!(server.safe_path("../../../etc/passwd").is_err());
    }

    #[tokio::test]
    async fn test_tool_call() {
        let server = FileSystemServer::new("/tmp/test").unwrap();

        let result = server.write_file("test.txt".into(), "Hello".into()).await;
        assert!(result.is_ok());

        let result = server.read_file("test.txt".into()).await;
        assert!(result.is_ok());
    }
}
```

### 集成测试

```rust
#[tokio::test]
async fn test_mcp_integration() {
    // 启动测试服务器
    let server = FileSystemServer::new("/tmp/test").unwrap();
    let service = server.serve(stdio()).await.unwrap();

    // 创建客户端连接
    let client = McpClient::new(
        "test".into(),
        "cargo run --bin test-server"
    ).await.unwrap();

    // 测试工具列表
    let tools = client.list_tools().await;
    assert!(!tools.is_empty());

    // 测试工具调用
    let result = client.call_tool(
        "read_file".into(),
        Some(serde_json::json!({"path": "test.txt"}))
    ).await;

    assert!(result.is_ok());
}
```

### Mock 测试

```rust
struct MockMcpClient {
    responses: HashMap<String, CallToolResult>,
}

impl MockMcpClient {
    fn new() -> Self {
        Self {
            responses: HashMap::new(),
        }
    }

    fn set_response(&mut self, tool: &str, response: CallToolResult) {
        self.responses.insert(tool.to_string(), response);
    }
}

#[async_trait::async_trait]
impl McpClient for MockMcpClient {
    async fn call_tool(
        &self,
        name: String,
        _arguments: Option<serde_json::Value>,
    ) -> Result<CallToolResult, Box<dyn std::error::Error>> {
        self.responses
            .get(&name)
            .cloned()
            .ok_or_else(|| "Tool not found".into())
    }
}
```

---

## 性能优化

### 1. 连接池

```rust
pub struct ConnectionPool {
    connections: Arc<RwLock<Vec<Arc<McpClient>>>>,
    max_size: usize,
}

impl ConnectionPool {
    pub fn new(max_size: usize) -> Self {
        Self {
            connections: Arc::new(RwLock::new(Vec::new())),
            max_size,
        }
    }

    pub async fn acquire(&self, name: String, command: String) -> Result<Arc<McpClient>, Box<dyn std::error::Error>> {
        let mut connections = self.connections.write().await;

        if connections.len() < self.max_size {
            let client = Arc::new(McpClient::new(name, &command).await?);
            connections.push(client.clone());
            Ok(client)
        } else {
            // 轮询返回连接
            let index = rand::random::<usize>() % connections.len();
            Ok(connections[index].clone())
        }
    }
}
```

### 2. 批量操作优化

```rust
use futures::stream::{self, StreamExt};

impl McpManager {
    pub async fn call_tools_concurrent(
        &self,
        calls: Vec<(String, String, Option<serde_json::Value>)>,
        max_concurrent: usize,
    ) -> Vec<Result<CallToolResult, Box<dyn std::error::Error>>> {
        stream::iter(calls)
            .map(|(server, tool, args)| {
                let manager = self.clone();
                async move {
                    manager.call_tool_on_server(&server, tool, args).await
                }
            })
            .buffer_unordered(max_concurrent)
            .collect()
            .await
    }
}
```

### 3. 内存优化

```rust
// 使用 Cow 避免不必要的克隆
use std::borrow::Cow;

#[tool]
async fn process_data(
    &self,
    #[tool(arg)] data: Cow<str>,
) -> Result<CallToolResult, McpError> {
    // data 可能是借用的或拥有的
    Ok(CallToolResult::success(vec![
        Content::text(format!("Processed: {}", data))
    ]))
}
```

---

## 安全性最佳实践

### 1. 输入验证

```rust
fn validate_path(path: &str) -> Result<(), McpError> {
    // 检查空路径
    if path.is_empty() {
        return Err(McpError {
            code: ErrorCode::InvalidParams,
            message: "Path cannot be empty".into(),
            data: None,
        });
    }

    // 检查路径长度
    if path.len() > 1024 {
        return Err(McpError {
            code: ErrorCode::InvalidParams,
            message: "Path too long".into(),
            data: None,
        });
    }

    // 检查非法字符
    if path.contains('\0') {
        return Err(McpError {
            code: ErrorCode::InvalidParams,
            message: "Null character detected".into(),
            data: None,
        });
    }

    Ok(())
}
```

### 2. 沙箱执行

```rust
use tempfile::TempDir;

pub struct SandboxedExecutor {
    temp_dir: TempDir,
}

impl SandboxedExecutor {
    pub fn new() -> Result<Self, std::io::Error> {
        Ok(Self {
            temp_dir: TempDir::new()?,
        })
    }

    pub async fn execute_tool(&self, tool_code: &str) -> Result<String, Box<dyn std::error::Error>> {
        // 在临时目录中执行工具
        let output = tokio::process::Command::new("secure-executor")
            .current_dir(self.temp_dir.path())
            .arg(tool_code)
            .output()
            .await?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            Err(format!("Execution failed: {}", String::from_utf8_lossy(&output.stderr)).into())
        }
    }
}
```

### 3. 速率限制

```rust
use governor::{Quota, RateLimiter};

pub struct RateLimitedClient {
    inner: Arc<McpClient>,
    limiter: RateLimiter<...>,
}

impl RateLimitedClient {
    pub fn new(inner: Arc<McpClient>, calls_per_second: u32) -> Self {
        let quota = Quota::per_second(nonzero!(calls_per_second));
        let limiter = RateLimiter::direct(quota);

        Self { inner, limiter }
    }

    pub async fn call_tool(
        &self,
        name: String,
        arguments: Option<serde_json::Value>,
    ) -> Result<CallToolResult, Box<dyn std::error::Error>> {
        // 等待速率限制
        self.limiter.until_ready().await;

        self.inner.call_tool(name, arguments).await
    }
}
```

---

## 部署建议

### 1. Docker 部署

```dockerfile
FROM rust:1.85 as builder

WORKDIR /app
COPY . .

RUN cargo build --release

FROM debian:bookworm-slim

RUN apt-get update && \
    apt-get install -y ca-certificates && \
    rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/mcp-server /usr/local/bin/

EXPOSE 8080

CMD ["mcp-server"]
```

### 2. Systemd 服务

```ini
[Unit]
Description=MCP Server
After=network.target

[Service]
Type=simple
User=mcp
ExecStart=/usr/local/bin/mcp-server
Restart=always
RestartSec=5

Environment="RUST_LOG=info"

[Install]
WantedBy=multi-user.target
```

### 3. 配置管理

```toml
# config.toml

[server]
name = "my-mcp-server"
version = "1.0.0"

[transport]
type = "stdio"  # 或 "http"

[http]
host = "127.0.0.1"
port = 8080

[security]
enable_auth = false
max_request_size = 10485760  # 10MB

[limits]
max_concurrent_requests = 100
request_timeout_secs = 30
```

```rust
use serde::Deserialize;

#[derive(Deserialize)]
struct ServerConfig {
    server: ServerSection,
    transport: TransportSection,
    security: SecuritySection,
    limits: LimitsSection,
}

fn load_config(path: &str) -> Result<ServerConfig, Box<dyn std::error::Error>> {
    let content = tokio::fs::read_to_string(path).await?;
    Ok(toml::from_str(&content)?)
}
```

---

## 总结

这些示例和最佳实践展示了如何在实际项目中使用 RMCP：

1. **完整的服务器和客户端实现**：可以直接用于生产环境
2. **高级模式**：流式处理、批量操作、工具链等
3. **测试策略**：单元测试、集成测试、Mock
4. **性能优化**：连接池、并发控制、内存优化
5. **安全实践**：输入验证、沙箱执行、速率限制
6. **部署建议**：Docker、Systemd、配置管理

通过这些模式，你可以构建安全、高效、可扩展的 MCP 应用。
