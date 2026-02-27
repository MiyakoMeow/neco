# RMCP (Rust MCP Client) 深度探索

> 版本: v0.16.0 | 更新时间: 2026-02-27

## 目录

1. [概述](#概述)
2. [MCP协议规范](#mcp协议规范)
3. [核心架构](#核心架构)
4. [服务器实现](#服务器实现)
5. [客户端实现](#客户端实现)
6. [工具系统](#工具系统)
7. [资源系统](#资源系统)
8. [提示系统](#提示系统)
9. [任务系统](#任务系统)
10. [传输层](#传输层)
11. [核心API参考](#核心api参考)
12. [最佳实践](#最佳实践)

---

## 概述

### 什么是RMCP

RMCP (Rust Model Context Protocol) 是 Model Context Protocol (MCP) 的官方 Rust 实现。MCP 是一个开放协议,用于在 LLM(大语言模型)应用与外部数据源和工具之间建立标准化的连接方式。

### 核心特性

- **双向通信**: 基于 JSON-RPC 2.0 的消息协议
- **异步运行时**: 基于 Tokio 的高性能异步处理
- **类型安全**: Rust 的类型系统提供编译时安全保证
- **模块化设计**: 支持服务器、客户端或两者同时启用
- **丰富的传输层**: 支持 stdio、HTTP (SSE)、子进程等多种传输方式

### 项目信息

| 属性 | 值 |
|------|-----|
| **GitHub仓库** | [modelcontextprotocol/rust-sdk](https://github.com/modelcontextprotocol/rust-sdk) |
| **Crates.io** | [rmcp](https://crates.io/crates/rmcp) |
| **文档** | [docs.rs/rmcp](https://docs.rs/rmcp) |
| **最新版本** | v0.16.0 |
| **许可证** | Apache-2.0 |
| **核心依赖** | tokio, serde, schemars |

### 快速开始

```toml
[dependencies]
rmcp = { version = "0.16", features = ["server", "client"] }
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
schemars = "0.8"
```

---

## MCP协议规范

### 协议架构

MCP 采用三层架构:

```
Host应用 → Client(MCP客户端) → Server(MCP服务器) → Tools/Resources/Prompts
```

### 核心概念

#### 角色(Roles)

- **Host(主机)**: 发起连接的 LLM 应用
- **Client(客户端)**: 主机应用中的连接器
- **Server(服务器)**: 提供上下文和功能的服务

#### 服务器功能(Server Features)

| 功能 | 控制方 | 描述 | 示例 |
|------|--------|------|------|
| **Prompts** | 用户控制 | 交互式模板,由用户选择触发 | 斜杠命令、菜单选项 |
| **Resources** | 应用控制 | 由客户端附加和管理的上下文数据 | 文件内容、git历史 |
| **Tools** | 模型控制 | 暴露给LLM执行的操作函数 | API POST请求、文件写入 |

#### 安全原则

1. **用户同意和控制**: 用户必须明确同意并理解所有数据访问和操作
2. **数据隐私**: 主机必须获得明确同意才能向服务器暴露用户数据
3. **工具安全**: 工具代表任意代码执行,必须谨慎处理
4. **LLM采样控制**: 用户必须明确批准任何LLM采样请求

---

## 核心架构

### 模块结构

```
rmcp
├── handler/          # 处理器
│   ├── server/       # 服务器端处理器
│   └── client/       # 客户端处理器
├── model/            # 数据模型
├── service/          # 服务层抽象
├── transport/        # 传输层实现
│   ├── io/          # stdio 传输
│   ├── child_process/ # 子进程传输
│   └── streamable_http/ # HTTP 流传输
└── macros/           # 过程宏(独立 crate)
```

### 核心组件

#### 1. Handler(处理器)

- **ServerHandler**: 服务器端处理器trait
- **ClientHandler**: 客户端处理器trait
- **ToolHandler**: 工具调用处理器
- **ResourceHandler**: 资源处理器
- **PromptHandler**: 提示处理器

#### 2. Model(数据模型)

定义协议数据结构:
- 请求/响应类型 (Request/Response)
- 内容类型 (Content, ResourceContents)
- 错误类型 (ErrorData, ErrorCode)

#### 3. Service(服务层)

- **ServiceExt**: 服务扩展trait,提供高层API
- **Peer**: 对等端点抽象,用于与对端通信
- **Role**: 客户端/服务器角色标记 (RoleClient, RoleServer)

#### 4. Transport(传输层)

- **Transport trait**: 统一传输接口
- **IntoTransport trait**: 类型转换辅助
- **具体实现**: stdio、HTTP、子进程等

### Feature Flags

| Feature | 描述 |
|---------|------|
| `client` | 启用客户端功能 |
| `server` | 启用服务器功能(默认) |
| `macros` | 启用 `#[tool]` 宏(默认) |
| `transport-io` | I/O流支持 |
| `transport-child-process` | 子进程支持 |
| `transport-streamable-http-client` | HTTP流客户端 |
| `transport-streamable-http-server` | HTTP流服务器 |
| `auth` | OAuth2认证支持 |
| `schemars` | JSON Schema生成 |

---

## 服务器实现

### 基础服务器

```rust
use rmcp::{
    ServerHandler, ServiceExt,
    handler::server::tool::{tool, tool_handler, tool_router},
    model::*,
    transport::stdio,
    ErrorData as McpError,
};

#[derive(Clone)]
pub struct MyServer {
    // 服务器状态
}

#[tool_router]
impl MyServer {
    #[tool(description = "Say hello")]
    async fn hello(&self, #[tool(arg)] name: String) -> Result<CallToolResult, McpError> {
        Ok(CallToolResult::success(vec![
            Content::text(format!("Hello, {}!", name))
        ]))
    }
}

#[tool_handler]
impl ServerHandler for MyServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            name: "my-server".into(),
            version: "1.0.0".into(),
            capabilities: ServerCapabilities::builder()
                .enable_tools()
                .build(),
            ..Default::default()
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let server = MyServer;
    let service = server.serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
```

### 服务器能力

```rust
let capabilities = ServerCapabilities::builder()
    .enable_tools()                    // 启用工具
    .enable_resources()                // 启用资源
    .enable_prompts()                  // 启用提示
    .enable_logging()                  // 启用日志
    .enable_completions()              // 启用自动完成
    .resources(ResourcesCapability {
        subscribe: true,               // 支持资源订阅
        list_changed: true,            // 支持列表变更通知
    })
    .tools(ToolsCapability {
        list_changed: true,            // 支持工具列表变更通知
    })
    .build();
```

### ServerHandler Trait

服务器必须实现 `ServerHandler` trait:

```rust
#[async_trait]
pub trait ServerHandler: Clone + Send + Sync + 'static {
    /// 服务器信息
    fn get_info(&self) -> ServerInfo;

    /// 初始化回调
    async fn on_initialize(
        &self,
        params: InitializeRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<InitializeResult, McpError>;

    /// 进度通知回调
    async fn on_progress(
        &self,
        notification: ProgressNotificationParam,
        context: NotificationContext<RoleServer>,
    );
}
```

---

## 客户端实现

### 基础客户端

```rust
use rmcp::{ServiceExt, model::CallToolRequestParams};
use rmcp::transport::{ConfigureCommandExt, TokioChildProcess};
use tokio::process::Command;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 连接到子进程服务器
    let service = ()
        .serve(TokioChildProcess::new(
            Command::new("uvx").configure(|cmd| {
                cmd.arg("mcp-server-git");
            })?
        )?)
        .await?;

    // 获取服务器信息
    let server_info = service.peer_info();
    println!("Connected to: {:#?}", server_info);

    // 列出可用工具
    let tools = service.list_tools(Default::default()).await?;
    println!("Available tools: {:#?}", tools);

    // 调用工具
    let result = service
        .call_tool(CallToolRequestParams {
            name: "git_status".into(),
            arguments: serde_json::json!({ "repo_path": "." }).as_object().cloned(),
            ..Default::default()
        })
        .await?;

    // 优雅关闭
    service.cancel().await?;
    Ok(())
}
```

### ClientHandler Trait

客户端可以可选实现 `ClientHandler` trait:

```rust
#[async_trait]
pub trait ClientHandler: Clone + Send + Sync + 'static {
    /// 客户端信息
    fn get_info(&self) -> ClientInfo;

    /// 初始化回调
    async fn on_initialize(
        &self,
        params: InitializeRequestParams,
        context: RequestContext<RoleClient>,
    ) -> Result<InitializeResult, McpError>;
}
```

### 客户端能力

```rust
let capabilities = ClientCapabilities::builder()
    .enable_sampling()              // 支持采样功能
    .enable_roots()                 // 支持根目录
    .build();
```

---

## 工具系统

### 工具定义

工具是可由语言模型调用的可执行函数,使用 `#[tool]` 宏定义:

```rust
#[tool(description = "Get weather information")]
async fn get_weather(
    &self,
    #[tool(arg)] location: String,
) -> Result<CallToolResult, McpError> {
    Ok(CallToolResult::success(vec![
        Content::text(format!("Weather in {}: Sunny, 72°F", location))
    ]))
}
```

### 工具属性

`#[tool]` 宏支持的属性:

| 属性 | 描述 | 示例 |
|------|------|------|
| `name` | 工具名称 | `#[tool(name = "calc")]` |
| `description` | 工具描述 | `#[tool(description = "Calculate")]` |
| `arg` | 标记参数 | `#[tool(arg)] param: String` |

### 工具路由

使用 `#[tool_router]` 自动注册所有标记的工具:

```rust
#[tool_router]
impl MyServer {
    // 所有 #[tool] 标记的方法会自动注册
}
```

### 结构化输出

使用 `Json` 包装器返回带 schema 的结构化数据:

```rust
use rmcp::handler::server::wrapper::Json;

#[derive(Serialize, Deserialize, JsonSchema)]
struct CalculatorRequest {
    a: i32,
    b: i32,
    operation: String,
}

#[derive(Serialize, Deserialize, JsonSchema)]
struct CalculatorResult {
    result: i32,
    operation: String,
}

#[tool(description = "Perform calculation")]
async fn calculate(
    &self,
    #[tool(arg)] request: Parameters<CalculatorRequest>,
) -> Result<Json<CalculatorResult>, String> {
    let result = match request.0.operation.as_str() {
        "add" => request.0.a + request.0.b,
        "multiply" => request.0.a * request.0.b,
        _ => return Err("Unknown operation".to_string()),
    };

    Ok(Json(CalculatorResult {
        result,
        operation: request.0.operation,
    }))
}
```

### 工具调用流程

```
Client → tools/list → Server: 返回工具列表
Client → tools/call → Server: 执行工具
Server → Client: 返回工具结果
```

---

## 资源系统

### 资源定义

资源是服务器提供的上下文数据,通过 URI 唯一标识:

```rust
use rmcp::handler::server::resource::{ResourceHandler, resource_handler};

#[resource_handler]
impl ResourceHandler for MyServer {
    async fn list_resources(
        &self,
        _request: ListResourcesRequestParams,
    ) -> Result<ListResourcesResult, McpError> {
        Ok(ListResourcesResult {
            resources: vec![
                Resource {
                    uri: "file:///project/src/main.rs".into(),
                    name: "main.rs".into(),
                    description: Some("Main entry point".into()),
                    mime_type: Some("text/x-rust".into()),
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
        Ok(ReadResourceResult {
            contents: vec![
                ResourceContents {
                    uri: request.uri.clone(),
                    mime_type: Some("text/plain".into()),
                    text: Some("content here".into()),
                    ..Default::default()
                }
            ],
        })
    }
}
```

### ResourceHandler Trait

```rust
#[async_trait]
pub trait ResourceHandler: Clone + Send + Sync + 'static {
    async fn list_resources(
        &self,
        request: ListResourcesRequestParams,
    ) -> Result<ListResourcesResult, McpError>;

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
    ) -> Result<ReadResourceResult, McpError>;

    async fn subscribe(
        &self,
        request: SubscribeRequestParams,
    ) -> Result<(), McpError>;

    async fn unsubscribe(
        &self,
        request: UnsubscribeRequestParams,
    ) -> Result<(), McpError>;
}
```

### 资源订阅

支持资源变更通知:

```rust
async fn subscribe(
    &self,
    request: SubscribeRequestParams,
) -> Result<(), McpError> {
    // 处理订阅请求,开始监控资源变更
    Ok(())
}

async fn unsubscribe(
    &self,
    request: UnsubscribeRequestParams,
) -> Result<(), McpError> {
    // 取消订阅
    Ok(())
}
```

---

## 提示系统

### 提示定义

提示是预定义的模板消息,由用户控制:

```rust
use rmcp::handler::server::prompt::{PromptHandler, prompt_handler};

#[prompt_handler]
impl PromptHandler for MyServer {
    async fn list_prompts(
        &self,
        _request: ListPromptsRequestParams,
    ) -> Result<ListPromptsResult, McpError> {
        Ok(ListPromptsResult {
            prompts: vec![
                Prompt {
                    name: "code_review".into(),
                    description: Some("Review code quality".into()),
                    arguments: Some(vec![
                        PromptArgument {
                            name: "code".into(),
                            description: Some("The code to review".into()),
                            required: Some(true),
                            ..Default::default()
                        }
                    ]),
                    ..Default::default()
                }
            ],
            ..Default::default()
        })
    }

    async fn get_prompt(
        &self,
        request: GetPromptRequestParams,
    ) -> Result<GetPromptResult, McpError> {
        let code = request.arguments
            .and_then(|args| args.get("code").cloned())
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_default();

        Ok(GetPromptResult {
            description: Some("Code review prompt".into()),
            messages: vec![
                PromptMessage {
                    role: PromptMessageRole::User,
                    content: PromptMessageContent::text(
                        format!("Please review this code:\n{}", code)
                    ),
                }
            ],
            ..Default::default()
        })
    }
}
```

### PromptHandler Trait

```rust
#[async_trait]
pub trait PromptHandler: Clone + Send + Sync + 'static {
    async fn list_prompts(
        &self,
        request: ListPromptsRequestParams,
    ) -> Result<ListPromptsResult, McpError>;

    async fn get_prompt(
        &self,
        request: GetPromptRequestParams,
    ) -> Result<GetPromptResult, McpError>;
}
```

---

## 任务系统

RMCP 实现了 SEP-1686 任务生命周期,支持长时间运行的异步工具调用。

### 任务生命周期

```
Queued → Processing → Completed/Failed/Cancelled
```

### 任务操作

#### 创建任务

```rust
let result = service
    .call_tool(CallToolRequestParams {
        name: "long_running_task".into(),
        task: Some(true),  // 启用任务模式
        ..Default::default()
    })
    .await?;

let task_id = result.task_id.unwrap();
```

#### 查询任务

```rust
// 获取任务信息
let task_info = service
    .get_task_info(GetTaskInfoRequestParams {
        id: task_id.clone(),
        ..Default::default()
    })
    .await?;

// 获取任务结果
let task_result = service
    .get_task_result(GetTaskResultRequestParams {
        id: task_id.clone(),
        ..Default::default()
    })
    .await?;
```

#### 取消任务

```rust
service
    .cancel_task(CancelTaskRequestParams {
        id: task_id,
        ..Default::default()
    })
    .await?;
```

### TaskHandler Trait

```rust
#[async_trait]
pub trait TaskHandler: Clone + Send + Sync + 'static {
    type Processor: OperationProcessor;

    fn processor(&self) -> Self::Processor;
}
```

### 任务元数据

```rust
pub struct TaskInfo {
    pub id: String,
    pub status: TaskStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub poll_interval: Option<Duration>,
}

pub enum TaskStatus {
    Queued,
    Processing,
    Completed,
    Failed,
    Cancelled,
}
```

---

## 传输层

### Transport Trait

所有传输类型实现 `Transport` trait:

```rust
pub trait Transport: Send + Sync + 'static {
    /// 并发发送消息
    fn send_msg(&self, msg: Message) -> BoxFuture<'_, Result<(), Error>>;

    /// 顺序接收消息
    fn recv_msg(&self) -> BoxFuture<'_, Result<Option<Message>, Error>>;
}
```

### 传输类型

| 传输类型 | 用途 | 服务器 | 客户端 | 特点 |
|----------|------|--------|--------|------|
| stdio | 本地进程通信 | ✓ | ✓ | 简单、广泛支持 |
| HTTP SSE | Web应用 | ✓ | ✓ | 支持流式传输 |
| Child Process | 独立进程 | - | ✓ | 隔离性好 |

### STDIO 传输

```rust
use rmcp::transport::stdio;

// 服务器端
let service = server.serve(stdio()).await?;

// 客户端端(子进程)
use rmcp::transport::TokioChildProcess;
use tokio::process::Command;

let transport = TokioChildProcess::new(
    Command::new("mcp-server")
)?;
```

### HTTP 流传输

```rust
// 服务器端
use rmcp::transport::streamable_http_server::tower::StreamableHttpService;

let service = StreamableHttpService::new(server);

// 客户端端
use rmcp::transport::streamable_http_client::StreamableHttpClientTransport;

let transport = StreamableHttpClientTransport::new(url)?;
```

### 自定义传输

```rust
use rmcp::transport::IntoTransport;

// 从 Sink + Stream 创建
let transport = (sink, stream).into_transport();

// 从 AsyncRead + AsyncWrite 创建
let transport = (reader, writer).into_transport();
```

---

## 核心API参考

### ServiceExt Trait

核心服务扩展trait,提供所有 MCP 操作:

#### 服务器信息

```rust
async fn peer_info(&self) -> ServerInfo;
async fn initialize(&self, params: InitializeRequestParams) -> Result<InitializeResult, Error>;
```

#### 工具操作

```rust
async fn list_tools(&self, params: ListToolsRequestParams) -> Result<ListToolsResult, Error>;
async fn call_tool(&self, params: CallToolRequestParams) -> Result<CallToolResult, Error>;
```

#### 资源操作

```rust
async fn list_resources(&self, params: ListResourcesRequestParams) -> Result<ListResourcesResult, Error>;
async fn read_resource(&self, params: ReadResourceRequestParams) -> Result<ReadResourceResult, Error>;
async fn subscribe(&self, params: SubscribeRequestParams) -> Result<(), Error>;
async fn unsubscribe(&self, params: UnsubscribeRequestParams) -> Result<(), Error>;
```

#### 提示操作

```rust
async fn list_prompts(&self, params: ListPromptsRequestParams) -> Result<ListPromptsResult, Error>;
async fn get_prompt(&self, params: GetPromptRequestParams) -> Result<GetPromptResult, Error>;
```

#### 任务操作

```rust
async fn get_task_info(&self, params: GetTaskInfoRequestParams) -> Result<TaskInfo, Error>;
async fn get_task_result(&self, params: GetTaskResultRequestParams) -> Result<TaskResult, Error>;
async fn cancel_task(&self, params: CancelTaskRequestParams) -> Result<TaskResult, Error>;
```

#### 根目录操作

```rust
async fn list_roots(&self, params: ListRootsRequestParams) -> Result<ListRootsResult, Error>;
```

#### 日志操作

```rust
async fn set_logging_level(&self, params: SetLoggingLevelRequestParams) -> Result<SetLoggingLevelResult, Error>;
```

#### 补全操作

```rust
async fn complete(&self, params: CompleteRequestParams) -> Result<CompleteResult, Error>;
```

### Peer 接口

对等端点接口,用于主动发送消息:

```rust
pub struct Peer<Role> {
    // 内部实现
}

impl<Role> Peer<Role> {
    /// 发送进度通知
    pub async fn notify_progress(&self, params: ProgressNotificationParam) -> Result<(), Error>;

    /// 发送日志消息
    pub async fn notify_logging_message(&self, params: LoggingMessageNotificationParam) -> Result<(), Error>;

    /// 发送资源更新
    pub async fn notify_resource_list_changed(&self, params: ResourceListChangedNotificationParam) -> Result<(), Error>;

    /// 发送工具列表更新
    pub async fn notify_tool_list_changed(&self, params: ToolListChangedNotificationParam) -> Result<(), Error>;

    /// 发送取消通知
    pub async fn notify_cancelled(&self, params: CancelledNotificationParam) -> Result<(), Error>;
}
```

### Context 接口

请求和通知上下文,提供对 Peer 的访问:

```rust
pub struct RequestContext<Role> {
    pub peer: Peer<Role>,
    pub request_id: String,
}

pub struct NotificationContext<Role> {
    pub peer: Peer<Role>,
}
```

### 错误处理

#### 错误代码

| 代码 | 名称 | 描述 |
|------|------|------|
| -32700 | Parse error | JSON 解析错误 |
| -32600 | Invalid Request | 无效请求 |
| -32601 | Method not found | 方法未找到 |
| -32602 | Invalid params | 无效参数 |
| -32603 | Internal error | 内部错误 |
| -32002 | Resource not found | 资源未找到 |
| -32004 | Feature not supported | 功能不支持 |

#### 错误类型

```rust
pub struct ErrorData {
    pub code: ErrorCode,
    pub message: String,
    pub data: Option<serde_json::Value>,
}

pub enum ErrorCode {
    ParseError,
    InvalidRequest,
    MethodNotFound,
    InvalidParams,
    InternalError,
    Custom(i32),
}
```

---

## 最佳实践

### 1. 错误处理

使用 `Result<CallToolResult, McpError>` 返回工具错误:

```rust
#[tool]
async fn safe_operation(&self, input: String) -> Result<CallToolResult, McpError> {
    match process_input(&input) {
        Ok(result) => Ok(CallToolResult::success(vec![Content::text(result)])),
        Err(e) => Ok(CallToolResult {
            content: vec![Content::text(format!("Error: {}", e))],
            is_error: Some(true),
            ..Default::default()
        }),
    }
}
```

### 2. 访问对等端点

从 handler 上下文访问 Peer 接口:

```rust
impl ServerHandler for MyHandler {
    async fn on_progress(
        &self,
        notification: ProgressNotificationParam,
        context: NotificationContext<RoleServer>,
    ) {
        let peer = context.peer;

        // 发送日志消息
        let _ = peer
            .notify_logging_message(LoggingMessageNotificationParam {
                level: LoggingLevel::Info,
                logger: None,
                data: serde_json::json!({"message": "Processing..."}),
            })
            .await;
    }
}
```

### 3. 状态管理

使用 `Arc<Mutex<T>>` 管理可变状态:

```rust
#[derive(Clone)]
pub struct MyServer {
    state: Arc<Mutex<ServerState>>,
}

pub struct ServerState {
    counter: i32,
}
```

### 4. 测试

使用 mock transport 进行测试:

```rust
#[tokio::test]
async fn test_server() {
    let server = MyServer::new();

    // 使用测试 transport
    let (client_transport, server_transport) = create_test_transport();

    let server_service = server.serve(server_transport).await.unwrap();

    // 测试逻辑...
}
```

### 5. 安全性

- 验证所有输入参数
- 限制文件系统访问范围
- 使用沙箱执行不受信任的代码
- 实施速率限制
- 记录所有操作日志

---

## 参考资源

- [MCP 规范](https://modelcontextprotocol.io/specification/2025-11-25)
- [RMCP 文档](https://docs.rs/rmcp)
- [GitHub 仓库](https://github.com/modelcontextprotocol/rust-sdk)
- [示例代码](https://github.com/modelcontextprotocol/rust-sdk/tree/main/examples)
