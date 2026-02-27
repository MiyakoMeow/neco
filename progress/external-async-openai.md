# async-openai Crate 探索报告

> 版本：0.33.0 | 最新更新：2026年2月27日
>
> 本文档全面探索 async-openai crate，涵盖核心API、流式输出、工具调用、错误处理等关键特性。

---

## 目录

1. [项目概览](#1-项目概览)
2. [核心API](#2-核心api)
3. [流式输出](#3-流式输出)
4. [工具调用](#4-工具调用)
5. [错误处理](#5-错误处理)
6. [配置](#6-配置)
7. [性能特性](#7-性能特性)
8. [与其他客户端对比](#8-与其他客户端对比)
9. [API调用流程](#9-api调用流程)

---

## 1. 项目概览

### 1.1 基本信息

| 属性 | 值 |
|------|-----|
| **仓库名称** | async-openai |
| **最新版本** | 0.33.0 |
| **License** | MIT |
| **维护者** | Himanshu Neema (@64bit) |
| **GitHub Stars** | 1.8k+ |
| **下载量** | 328万+ (总计), 149万+ (近期) |
| **版本数** | 104个版本 |
| **Rust Edition** | 2021 |
| **关键字** | ai, async, openai, openapi |
| **分类** | API bindings, Asynchronous, Web programming |

### 1.2 项目定位

`async-openai` 是一个**非官方**的 Rust OpenAI 客户端库，基于 [OpenAI OpenAPI 规范](https://github.com/openai/openai-openapi) 构建。它实现了 OpenAPI 规范中的**所有 API**。

### 1.3 核心特性

| 特性 | 说明 |
|------|------|
| **BYOT** | Bring Your Own Types - 支持自定义请求/响应类型 |
| **SSE 流式输出** | Server-Sent Events 流式传输 |
| **自动重试** | 速率限制时自动指数退避重试（SSE 除外） |
| **Builder 模式** | 所有请求对象使用构建器模式 |
| **细粒度特性** | 支持单独启用类型或 API |
| **Azure 支持** | 支持 Azure OpenAI Service |
| **WASM 支持** | WebAssembly 目标支持（流式和重试未实现） |

---

## 2. 核心API

### 2.1 支持的 API 类别

#### Responses API (最新)
- **Feature**: `responses`
- **类型**: `response-types`
- **功能**: Responses、Conversations、Streaming events

#### Platform APIs

| API | Feature | 用途 |
|-----|---------|------|
| Audio | `audio` | 语音转文字、文字转语音、音频流式传输 |
| Video | `video` | 视频生成 (Sora) |
| Images | `image` | 图像生成、编辑、变体、流式传输 |
| Embeddings | `embedding` | 文本向量化 |
| Evals | `evals` | 评估创建和管理 |
| Fine-tuning | `finetuning` | 模型微调 |
| Batch | `batch` | 批量 API 请求（50%折扣） |
| Files | `file` | 文件上传和管理 |
| Upload | `upload` | 文件上传（与 file 配合使用） |
| Models | `model` | 模型列表和详情 |
| Moderations | `moderation` | 内容审核 |
| Vector Store | `vectorstore` | 向量存储（用于文件检索） |

#### 其他 API

| API | Feature | 说明 |
|-----|---------|------|
| Chat Completions | `chat-completion` | Chat Completions、Streaming |
| ChatKit | `chatkit` | ChatKit API（新的聊天框架） |
| Assistants | `assistant` | Assistants、Threads、Messages、Runs（Beta） |
| Realtime | `realtime` | Realtime API、WebRTC 连接 |
| Container | `container` | 容器 API |
| Skill | `skill` | 技能 API |
| Administration | `administration` | Admin API Keys、Users、Projects |
| Completions | `completions` | 旧的 Completions API（不推荐） |

### 2.2 客户端 API 访问器

```rust
let client = Client::new();

client.responses()      // Responses API
client.chat()           // Chat Completions
client.chatkit()        // ChatKit API
client.audio()          // Audio API
client.images()         // Images API
client.embeddings()     // Embeddings API
client.files()          // Files API
client.models()         // Models API
client.batches()        // Batch API
client.assistants()     // Assistants API
client.realtime()       // Realtime API
client.container()      // Container API
client.skill()          // Skill API
client.admin()          // Administration API
client.evals()          // Evals API
client.uploads()        // Upload API
client.vector_stores()  // Vector Store API
```

### 2.3 核心数据结构

#### Chat Completions API

| 类型 | 用途 |
|------|------|
| `ChatCompletionArgs` | 请求参数构建器 |
| `ChatCompletionMessage` | 消息内容 |
| `CreateChatCompletionResponse` | 响应类型 |
| `Role` | 角色枚举（System/User/Assistant/Tool） |

#### Responses API

| 类型 | 用途 |
|------|------|
| `CreateResponseArgs` | 请求参数构建器 |
| `Input` | 输入类型（String/Vec<Message>） |
| `CreateResponseResponse` | 响应类型 |

#### ChatKit API

| 类型 | 用途 |
|------|------|
| `CreateChatKitArgs` | ChatKit 请求参数 |
| `ChatKitResponse` | ChatKit 响应类型 |

#### Container API

| 类型 | 用途 |
|------|------|
| `ContainerArgs` | 容器请求参数 |
| `ContainerResponse` | 容器响应类型 |

#### Skill API

| 类型 | 用途 |
|------|------|
| `SkillArgs` | 技能请求参数 |
| `SkillResponse` | 技能响应类型 |

#### Evals API

| 类型 | 用途 |
|------|------|
| `CreateEvalArgs` | 评估创建参数 |
| `EvalResponse` | 评估响应类型 |
| `Grader` | 评估器配置 |

---

## 3. 流式输出

### 3.1 支持流式的 API

| API | 方法 | 说明 |
|-----|------|------|
| Responses | `create_stream()` | Responses API 流式输出 |
| Chat Completions | `create_stream()` | 聊天补全流式输出 |
| Assistants | `create_stream()` | Assistants 流式运行 |
| Audio | `create_stream()` | 语音合成流式输出 |
| Images | `create_stream()` | 图像生成流式输出 |

### 3.2 核心机制

- **reqwest-eventsource** (0.6.0) - EventSource 客户端
- **eventsource-stream** (0.2) - SSE 流处理
- **tokio-stream** (0.1) - Tokio 流工具

### 3.3 流式输出示例

```rust
use async_openai::{Client, types::chat::*};
use futures::stream::StreamExt;

let client = Client::new();
let request = ChatCompletionArgs::default()
    .model("gpt-4o")
    .messages(vec![
        ChatCompletionMessage {
            role: Role::User,
            content: "写一首关于Rust编程的诗",
            ..Default::default()
        }
    ])
    .build()?;

let mut stream = client.chat().create_stream(request).await?;

while let Some(response) = stream.next().await {
    if let Ok(resp) = response {
        if let Some(choices) = resp.choices {
            for choice in choices {
                if let Some(delta) = choice.delta.content {
                    print!("{}", delta);
                    std::io::stdout().flush().ok();
                }
            }
        }
    }
}
```

### 3.4 流式输出特点

✅ **优点**：真正的流式输出、符合 Rust 异步生态、自动处理 SSE 解析

⚠️ **限制**：WASM 不支持、流式请求不支持自动重试

---

## 4. 工具调用

### 4.1 工具类型

| 工具类型 | API | 说明 |
|---------|-----|------|
| Function Calling | Chat Completions, Responses | 调用自定义函数 |
| Code Interpreter | Assistants | 代码解释器 |
| File Search | Assistants | 文件检索 |

### 4.2 Function Calling 结构

| 类型 | 用途 |
|------|------|
| `Tool` | 工具枚举（Function/Retrieval/CodeInterpreter） |
| `Function` | 函数定义（name, description, parameters） |
| `ToolCall` | 工具调用结果（id, type, function） |

### 4.3 工具调用示例

```rust
use async_openai::{Client, types::chat::*};
use futures::stream::StreamExt;
use serde_json::json;
use std::io::Write;

let client = Client::new();

// 定义工具
let weather_function = Function {
    name: "get_weather".to_string(),
    description: Some("Get the current weather".to_string()),
    parameters: Some(json!({
        "type": "object",
        "properties": {
            "location": {"type": "string"}
        },
        "required": ["location"]
    })),
};

let request = ChatCompletionArgs::default()
    .model("gpt-4o")
    .messages(vec![/* ... */])
    .tools(vec![Tool::Function(weather_function)])
    .build()?;

let response = client.chat().create(request).await?;

// 处理工具调用
if let Some(choice) = response.choices.first() {
    if let Some(tool_calls) = &choice.message.tool_calls {
        for tool_call in tool_calls {
            // 执行工具逻辑并返回结果
        }
    }
}
```

### 4.4 辅助 Crate

**[openai-func-enums](https://github.com/frankfralick/openai-func-enums)** - 提供宏简化工具调用组合

---

## 5. 错误处理

### 5.1 错误类型层次

```
OpenAIError
├── ApiError(ApiError)         // OpenAI API 错误
├── StreamError(StreamError)   // 流式输出错误
├── SerializationError         // JSON 序列化错误
├── NetworkError               // 网络请求错误
├── IOError                    // 文件 I/O 错误
└── NotImplemented            // 未实现的功能
```

### 5.2 核心错误类型

#### `ApiError`

| 字段 | 类型 | 说明 |
|------|------|------|
| `message` | `String` | 错误消息 |
| `type_` | `String` | 错误类型 |
| `param` | `Option<String>` | 相关参数 |
| `code` | `Option<String>` | 错误代码 |

常见错误类型：`invalid_request_error`、`authentication_error`、`rate_limit_error`

#### `StreamError`

| 变体 | 说明 |
|------|------|
| `ConnectionClosed` | SSE 连接中断 |
| `InvalidEvent` | 无效的 SSE 事件 |
| `ParseError` | 解析错误 |
| `NetworkError` | 网络错误 |

### 5.3 错误处理示例

```rust
use async_openai::{Client, error::OpenAIError};

match client.chat().create(request).await {
    Ok(response) => { /* 处理响应 */ }
    Err(OpenAIError::ApiError(e)) => {
        eprintln!("API Error [{}]: {}", e.type_, e.message);
    }
    Err(OpenAIError::StreamError(e)) => {
        eprintln!("Stream error: {:?}", e);
    }
    Err(e) => {
        eprintln!("Unexpected error: {}", e);
    }
}
```

### 5.4 重试机制

- **触发条件**：速率限制 (429 状态码)
- **重试策略**：指数退避
- **使用库**：`backoff` (0.4.0)
- **限制**：SSE 流式请求不支持自动重试

---

## 6. 配置

### 6.1 基本配置

```rust
use async_openai::{Client, config::OpenAIConfig};

// 从环境变量加载
let client = Client::new();

// 自定义 API Key
let config = OpenAIConfig::new()
    .with_api_key("sk-...")
    .with_org_id("org-...");
let client = Client::with_config(config);
```

**支持的环境变量**：
- `OPENAI_API_KEY` - API 密钥
- `OPENAI_BASE_URL` - 自定义 Base URL
- `OPENAI_ORG_ID` - 组织 ID
- `OPENAI_PROJECT_ID` - 项目 ID

### 6.2 高级配置

```rust
use async_openai::{Client, config::OpenAIConfig};

// 自定义 HTTP 客户端
let http_client = reqwest::ClientBuilder::new()
    .user_agent("MyApp/1.0")
    .timeout(std::time::Duration::from_secs(120))
    .build()?;

let client = Client::new().with_http_client(http_client);

// 全局请求头和查询参数
let config = OpenAIConfig::new()
    .with_api_key("sk-...")
    .with_http_headers(vec![
        ("X-Custom-Header".to_string(), "value".to_string())
    ])
    .with_query_parameters(vec![
        ("custom_param".to_string(), "value".to_string())
    ]);
```

### 6.3 Azure OpenAI 配置

```rust
use async_openai::{Client, config::AzureConfig};

let config = AzureConfig::new()
    .with_api_base("https://my-resource.openai.azure.com")
    .with_api_version("2024-02-01")
    .with_deployment_id("my-deployment")
    .with_api_key("...");

let client = Client::with_config(config);
```

### 6.4 单请求配置

```rust
use async_openai::{Client, types::chat::*};

let client = Client::new();

// 为单个请求添加配置
client
    .chat()
    .query(&[("custom", "value")])?
    .header("X-Custom", "value")?
    .create(request)
    .await?;
```

---

## 7. 性能特性

### 7.1 特性标志优化

```toml
# 最小依赖
async-openai = { version = "0.33", features = ["rustls"] }

# 仅使用类型
async-openai = { version = "0.33", features = ["types"] }

# 仅启用特定 API
async-openai = { version = "0.33", features = [
    "chat-completion",
    "embedding",
] }

# 启用所有功能
async-openai = { version = "0.33", features = ["full"] }

# BYOT
async-openai = { version = "0.33", features = ["byot"] }
```

### 7.2 编译时优化

| 特性标志 | 编译时间 | 运行时影响 |
|---------|---------|-----------|
| `types` | 低 | 无 |
| 单个 API 特性 | 中 | 无 |
| `full` | 高 | 无 |
| `byot` | 低 | 极低 |

### 7.3 运行时性能

- **零拷贝**：使用 `bytes::Bytes` 处理二进制数据
- **流式处理**：减少内存占用
- **智能指针**：使用 `Arc` 共享配置

### 7.4 并发处理示例

```rust
use futures::stream::{self, StreamExt};

async fn process_concurrent(client: &Client, prompts: Vec<String>) {
    stream::iter(prompts)
        .map(|prompt| async {
            let request = ChatCompletionArgs::default()
                .model("gpt-4o-mini")
                .messages(vec![/* ... */])
                .build()?;
            client.chat().create(request).await
        })
        .buffer_unordered(10)
        .collect::<Vec<_>>()
        .await;
}
```

---

## 8. 与其他客户端对比

### 8.1 Rust 生态对比

| 特性 | async-openai | openai-rust |
|------|-------------|-------------|
| **版本** | 0.33.0 | 0.14.0 |
| **Stars** | 1.8k | 3.4k |
| **API 覆盖** | ✅ 全部 API | ✅ 主要 API |
| **流式输出** | ✅ 原生支持 | ✅ 原生支持 |
| **工具调用** | ✅ 支持 | ✅ 支持 |
| **Azure 支持** | ✅ | ✅ |
| **WASM 支持** | ✅ 实验性 | ✅ 完整 |
| **类型分离** | ✅ 类型+API 分离 | 一体化 |

### 8.2 async-openai 优势

1. **API 覆盖最完整** - 实现所有 OpenAI API
2. **BYOT 支持** - 可使用自定义类型
3. **细粒度特性** - 编译时优化
4. **自动重试** - 速率限制自动处理

---

## 9. API调用流程

### 9.1 完整流程

```
初始化 Client → 选择 API 组 → 构建请求 → 发送请求 → 处理响应
```

### 9.2 Chat Completion 示例

```rust
use async_openai::{Client, types::chat::*};

// 1. 初始化客户端
let client = Client::new();

// 2. 构建请求
let request = ChatCompletionArgs::default()
    .model("gpt-4o")
    .messages(vec![
        ChatCompletionMessage {
            role: Role::User,
            content: "What is Rust?".to_string(),
            ..Default::default()
        }
    ])
    .temperature(0.7)
    .max_tokens(1000u32)
    .build()?;

// 3. 发送请求（自动重试）
let response = client.chat().create(request).await?;

// 4. 处理响应
if let Some(choice) = response.choices.first() {
    println!("Response: {}", choice.message.content.as_ref().unwrap());
}
```

### 9.3 流式示例

```rust
use async_openai::{Client, types::chat::*};
use futures::stream::StreamExt;

let client = Client::new();
let request = ChatCompletionArgs::default()
    .model("gpt-4o")
    .messages(vec![/* ... */])
    .build()?;

let mut stream = client.chat().create_stream(request).await?;

while let Some(result) = stream.next().await {
    if let Ok(delta) = result {
        if let Some(choices) = delta.choices {
            for choice in choices {
                if let Some(content) = choice.delta.content {
                    print!("{}", content);
                }
            }
        }
    }
}
```

---

## 附录

### Feature Flags 完整列表

```toml
# TLS 后端
default = ["rustls"]
rustls = ["dep:reqwest", "reqwest/rustls-tls-native-roots"]
rustls-webpki-roots = ["dep:reqwest", "reqwest/rustls-tls-webpki-roots"]
native-tls = ["dep:reqwest", "reqwest/native-tls"]
native-tls-vendored = ["dep:reqwest", "reqwest/native-tls-vendored"]

# 特殊功能
byot = ["dep:async-openai-macros"]

# API 功能
responses = ["response-types", "_api"]
webhook = ["webhook-types", "dep:base64", "dep:thiserror", "dep:hmac", "dep:sha2", "dep:hex"]
audio = ["audio-types", "_api"]
video = ["video-types", "_api"]
image = ["image-types", "_api"]
embedding = ["embedding-types", "_api"]
chat-completion = ["chat-completion-types", "_api"]
chatkit = ["chatkit-types", "_api"]
assistant = ["assistant-types", "_api"]
realtime = ["realtime-types", "_api", "dep:tokio-tungstenite"]
container = ["container-types", "_api"]
skill = ["skill-types", "_api"]
administration = ["administration-types", "_api"]
completions = ["completion-types", "_api"]
evals = ["eval-types", "_api"]
finetuning = ["finetuning-types", "_api"]
file = ["file-types", "_api"]
upload = ["upload-types", "_api"]
model = ["model-types", "_api"]
moderation = ["moderation-types", "_api"]
vectorstore = ["vectorstore-types", "_api"]
batch = ["batch-types", "_api"]
grader = ["grader-types"]

# 组合功能
types = [...]  # 所有类型（共20种类型）
full = [...]   # 所有功能
```

### 核心依赖树

```
async-openai 0.33.0
├── serde (always)
├── serde_json (always)
├── derive_builder (optional, for types)
├── bytes (optional, for binary data)
│
├── [API dependencies - when _api is enabled]
│   ├── reqwest 0.12
│   ├── tokio 1
│   ├── tokio-stream 0.1
│   ├── tokio-util (optional)
│   ├── futures 0.3
│   ├── backoff 0.4.0
│   ├── reqwest-eventsource 0.6.0
│   ├── eventsource-stream 0.2
│   ├── tracing 0.1
│   ├── secrecy (optional)
│   ├── serde_urlencoded (optional)
│   ├── url (optional)
│   ├── rand (optional)
│   └── async-openai-macros (optional, for BYOT)
│
├── [Realtime dependencies]
│   └── tokio-tungstenite
│
└── [Webhook dependencies]
    ├── base64 0.22
    ├── hmac 0.12
    ├── sha2 0.10
    └── hex
```

### 参考资源

- **官方仓库**: https://github.com/64bit/async-openai
- **文档**: https://docs.rs/async-openai
- **Crates.io**: https://crates.io/crates/async-openai
- **OpenAI API**: https://platform.openai.com/docs

---

**文档生成时间**: 2026年2月27日
**分析版本**: async-openai 0.33.0
