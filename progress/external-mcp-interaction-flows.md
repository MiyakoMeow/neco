# MCP 交互流程图

本文档描述 Model Context Protocol (MCP) 的核心交互流程，基于 **MCP 2025-11-25** 规范。

**版本说明**：
- 当前协议版本：`2025-11-25`
- 新增实验性功能：Tasks（任务系统），支持长时间运行的异步操作
- 协议使用字符串版本标识符（`YYYY-MM-DD` 格式）

## 架构概述

MCP 使用 JSON-RPC 2.0 消息格式，在三个角色之间建立通信：

- **Host（宿主应用）**: LLM 应用，发起连接
- **Client（客户端）**: 宿主应用内的连接器
- **Server（服务器）**: 提供上下文和能力的服务

---

## 1. 连接生命周期

```mermaid
sequenceDiagram
    participant Host as Host 应用
    participant Client as MCP 客户端
    participant Server as MCP 服务器

    Note over Host,Server: 初始化阶段
    Host->>Client: 启动连接
    Client->>Server: initialize 请求 (protocolVersion, capabilities)
    Server-->>Client: initialize 响应 (capabilities, protocolVersion)
    Client->>Server: initialized 通知

    Note over Host,Server: 发现阶段
    Client->>Server: tools/list
    Server-->>Client: 工具列表
    Client->>Server: resources/list
    Server-->>Client: 资源列表
    Client->>Server: prompts/list
    Server-->>Client: 提示列表

    Note over Host,Server: 交互阶段
    Host->>Client: 用户请求
    Client->>Server: tools/call 调用
    Server-->>Client: 工具结果
    Client-->>Host: 展示结果

    Note over Host,Server: 清理阶段
    Client->>Server: shutdown 请求
    Server-->>Client: shutdown 响应
```

---

## 2. 工具调用流程

```mermaid
flowchart TD
    Start[客户端发起工具调用] --> Validate[验证工具名称和参数]

    Validate -->|无效参数| Error1[返回参数错误 -32602]
    Validate -->|参数有效| CheckMode{执行模式?}

    CheckMode -->|同步执行| DirectExec[直接执行工具]
    CheckMode -->|Tasks 实验性| CreateTask[创建任务增强请求]

    CreateTask --> ReturnTaskResult[返回 CreateTaskResult<br/>taskId + status: working]
    ReturnTaskResult --> Poll[客户端轮询 tasks/get]

    Poll --> CheckStatus{任务状态?}
    CheckStatus -->|working| Poll
    CheckStatus -->|input_required| WaitForInput[等待客户端输入]
    CheckStatus -->|completed/failed| GetResult[获取最终结果]

    WaitForInput --> Poll

    DirectExec --> Execute[执行工具逻辑]
    Execute --> CheckResult{执行结果?}

    CheckResult -->|成功| Success[返回成功结果]
    CheckResult -->|失败| Failure[返回错误结果]

    Success --> End[客户端接收结果]
    Failure --> End
    GetResult --> End
    Error1 --> End

    note1[注意：Tasks 为实验性功能<br/>需双方协商 capabilities.tasks]
```

**工具级别任务支持协商**：

工具在 `tools/list` 响应中可通过 `execution.taskSupport` 声明任务支持级别：

| 值 | 说明 |
|---|---|
| `"required"` | 必须作为任务调用 |
| `"optional"` | 可选作为任务调用 |
| `"forbidden"` | 不支持任务调用 |

---

## 3. 资源访问流程

```mermaid
flowchart LR
    Client[客户端] --> ListReq{resources/list}
    ListReq -->|有效响应| Resources[资源列表]
    Resources --> Select{用户选择}

    Select --> Read[resources/read]
    Select --> Subscribe[resources/subscribe]

    Read --> ValidateURI{验证 URI}
    ValidateURI -->|有效| Fetch[获取资源内容]
    ValidateURI -->|无效| URIError[返回 -32002 错误]

    Fetch --> Encode[编码内容]
    Encode --> Return[返回资源]

    Subscribe --> Monitor[开始监控]
    Monitor --> Update[检测变化]
    Update --> Notify[发送 updated 通知]
```

---

## 4. 能力协商流程

```mermaid
sequenceDiagram
    participant C as 客户端
    participant S as 服务器

    Note over C,S: 初始化请求
    C->>S: initialize
    Note right of C: protocolVersion: "2025-11-25"<br/>capabilities: {<br/>  roots?: {...},<br/>  sampling?: {...}<br/>}

    Note over S: 验证协议版本<br/>确定服务器能力

    S->>C: initialize 响应
    Note left of S: protocolVersion: "2025-11-25"<br/>capabilities: {<br/>  tools?: {...},<br/>  resources?: {...},<br/>  prompts?: {...}<br/>}

    C->>S: initialized 通知
    Note over C,S: 连接就绪，<br/>仅使用协商的功能
```

### 服务器能力

| 能力 | 说明 |
|------|------|
| `tools` | 服务器提供可执行的工具 |
| `resources` | 服务器提供数据资源 |
| `prompts` | 服务器提供模板化提示 |
| `tasks` | 实验性：支持任务增强的异步操作 |

### 客户端能力

| 能力 | 说明 |
|------|------|
| `sampling` | 支持服务器发起的 LLM 采样 |
| `roots` | 支持服务器查询 URI/文件系统边界 |
| `elicitation` | 支持服务器请求额外用户信息 |
| `tasks` | 实验性：支持任务增强的异步操作 |

---

## 5. 多模态内容处理

```mermaid
flowchart LR
    Input[工具输入] --> Type{内容类型?}

    Type -->|text| Text[文本内容]
    Type -->|image| Image[图像内容<br/>base64 或 URI]
    Type -->|resource| Resource[资源引用]

    Text --> Process[处理内容]
    Image --> Decode[解码/加载图像]
    Decode --> Process

    Resource --> Resolve[解析 URI]
    Resolve --> Fetch[获取资源内容]
    Fetch --> Process

    Process --> Output[工具输出]
    Output --> Format{输出格式?}

    Format -->|单一文本| Simple[简单文本响应]
    Format -->|多部分| Multipart[多部分响应<br/>text + image + resource]

    Multipart --> Client[客户端展示]
    Simple --> Client
```

---

## 6. 传输层

### 6.1 stdio 传输

```mermaid
sequenceDiagram
    participant Host as 宿主进程
    participant Server as MCP 服务器

    Note over Host,Server: 通过 stdin/stdout 通信
    Host->>Server: JSON-RPC 消息 (stdin)
    Server->>Host: JSON-RPC 响应 (stdout)

    loop 消息交换
        Host->>Server: 请求
        Server-->>Host: 响应/通知
    end
```

### 6.2 HTTP/SSE 传输

```mermaid
sequenceDiagram
    participant Client as HTTP 客户端
    participant Server as HTTP 服务器
    participant MCP as MCP 处理器

    Client->>Server: POST /messages<br/>设置 SSE

    Server->>MCP: 建立连接
    MCP-->>Server: 就绪

    loop 消息传输
        Client->>Server: POST JSON-RPC 请求
        Server->>MCP: 转发消息
        MCP-->>Server: 处理响应
        Server-->>Client: event: message<br/>data: {JSON-RPC 响应}
    end

    Client->>Server: 关闭连接
    Server->>MCP: 清理资源
```

---

## 7. 错误处理

```mermaid
flowchart TD
    Request[收到请求] --> Validate{验证输入}

    Validate -->|无效 JSON| ParseError[Parse error -32700]
    Validate -->|无效请求| InvalidReq[Invalid Request -32600]
    Validate -->|方法不存在| MethodNotFound[Method not found -32601]
    Validate -->|无效参数| InvalidParams[Invalid params -32602]
    Validate -->|内部错误| InternalError[Internal error -32603]

    Validate -->|有效| Execute{执行操作}

    Execute -->|资源未找到| NotFound[Resource not found -32002]
    Execute -->|工具执行失败| ToolError[工具错误结果]

    ParseError --> SendErrorResponse[发送错误响应]
    InvalidReq --> SendErrorResponse
    MethodNotFound --> SendErrorResponse
    InvalidParams --> SendErrorResponse
    InternalError --> SendErrorResponse
    NotFound --> SendErrorResponse

    ToolError --> CheckType{错误类型?}
    CheckType -->|可恢复| Retry[重试]
    CheckType -->|不可恢复| ReturnError[返回带 is_error=true 的结果]

    Retry --> Execute
    SendErrorResponse --> Client[客户端处理]
    ReturnError --> Client
```

### JSON-RPC 标准错误码

| 错误码 | 说明 |
|--------|------|
| `-32700` | Parse error（解析错误） |
| `-32600` | Invalid Request（无效请求） |
| `-32601` | Method not found（方法未找到） |
| `-32602` | Invalid params（无效参数） |
| `-32603` | Internal error（内部错误） |

### MCP 特定错误码

| 错误码 | 说明 |
|--------|------|
| `-32000` to `-32099` | 服务器/实现特定错误 |
| `-32002` | Resource not found（资源未找到） |
| `-32601` | Method not found（方法不支持，如尝试对不支持 tasks 的工具使用任务增强） |

---

## 8. 进度和取消

### 8.1 传统进度机制

```mermaid
stateDiagram-v2
    [*] --> Idle: 任务创建
    Idle --> Executing: 开始执行
    Executing --> Progress: 报告进度
    Progress --> Executing: 继续执行
    Executing --> Completed: 完成
    Executing --> Cancelled: 收集取消请求
    Completed --> [*]
    Cancelled --> [*]

    note right of Progress
        progress token
        持续更新进度
    end note

    note right of Cancelled
        通过取消 token
        或 shutdown
    end note
```

### 8.2 Tasks 系统（实验性功能）

> **注意**：Tasks 在 2025-11-25 版本中引入，目前为**实验性功能**。其设计和行为可能在未来的协议版本中发生变化。

Tasks 提供了一种持久化状态机，用于跟踪请求的执行状态，支持长时间运行的操作和延迟结果检索。

#### 任务状态流转

```mermaid
stateDiagram-v2
    [*] --> working: 创建任务
    
    working --> input_required: 需要客户端输入
    working --> completed: 成功完成
    working --> failed: 执行失败
    working --> cancelled: 被取消
    
    input_required --> working: 获得所需输入
    input_required --> completed: 完成
    input_required --> failed: 失败
    input_required --> cancelled: 被取消
    
    completed --> [*]
    failed --> [*]
    cancelled --> [*]
    
    note right of working
        正在处理请求
        通过 tasks/get 轮询状态
    end note
    
    note right of input_required
        服务器需要客户端提供
        额外信息才能继续
        应调用 tasks/result
    end note
    
    note right of completed
        任务成功完成
        可通过 tasks/result 获取结果
    end note
```

#### 任务交互流程

```mermaid
sequenceDiagram
    participant C as 客户端 (请求方)
    participant S as 服务器 (接收方)

    Note over C,S: 1. 创建任务
    C->>S: tools/call (task: {ttl: 60000})
    activate S
    S-->>C: CreateTaskResult (taskId, status: working, pollInterval)
    deactivate S

    Note over C,S: 2. 轮询状态 (遵循 pollInterval)
    loop 轮询
        C->>S: tasks/get (taskId)
        activate S
        S-->>C: Task 状态 (working/input_required/completed/failed/cancelled)
        deactivate S
    end

    Note over C,S: 3. 获取结果
    C->>S: tasks/result (taskId)
    activate S
    Note over S: 如果未完成，阻塞直到任务完成
    S-->>C: 实际操作结果 (或 JSON-RPC 错误)
    deactivate S

    Note over C,S: 4. 清理
    Note over S: ttl 期限后可删除任务
```

#### 任务能力协商

**服务器能力声明**：
```json
{
  "capabilities": {
    "tasks": {
      "list": {},
      "cancel": {},
      "requests": {
        "tools": {
          "call": {}
        }
      }
    }
  }
}
```

**客户端能力声明**：
```json
{
  "capabilities": {
    "tasks": {
      "list": {},
      "cancel": {},
      "requests": {
        "sampling": {
          "createMessage": {}
        }
      }
    }
  }
}
```

#### 关键操作

| 操作 | 说明 |
|------|------|
| `tasks/get` | 轮询任务状态 |
| `tasks/result` | 获取任务结果（会阻塞直到完成） |
| `tasks/list` | 列出所有任务（支持分页） |
| `tasks/cancel` | 取消任务 |
| `notifications/tasks/status` | 任务状态变化通知（可选） |

#### input_required 状态

当任务进入 `input_required` 状态时：
- 服务器需要客户端提供额外信息
- 客户端应调用 `tasks/result` 来接收输入请求
- 所有相关请求必须包含 `io.modelcontextprotocol/related-task` 元数据

---

## 9. 生态系统架构

```mermaid
graph TB
    subgraph "应用层 (Host)"
        IDE[Claude Code/IDE]
        Desktop[Claude Desktop]
        Web[Web 应用]
    end

    subgraph "客户端层 (Client)"
        RustClient[RMCP]
        TSClient[TypeScript SDK]
        PyClient[Python SDK]
    end

    subgraph "传输层"
        Stdio[stdio]
        HTTP[SSE/HTTP]
        WS[WebSocket]
    end

    subgraph "服务器层 (Server)"
        FS[文件系统服务器]
        Git[Git 服务器]
        DB[数据库服务器]
        Search[搜索服务器]
        Custom[自定义服务器]
    end

    subgraph "后端服务"
        Files[本地文件系统]
        Repos[Git 仓库]
        Database[(数据库)]
        WebAPI[外部 API]
    end

    IDE --> RustClient
    Desktop --> TSClient
    Web --> TSClient

    RustClient --> Stdio
    RustClient --> HTTP
    TSClient --> HTTP
    TSClient --> WS
    PyClient --> Stdio

    Stdio --> FS
    Stdio --> Git
    HTTP --> Search
    HTTP --> Custom
    WS --> Custom

    FS --> Files
    Git --> Repos
    DB --> Database
    Search --> WebAPI
```

---

## 相关资源

- [MCP 官方规范](https://modelcontextprotocol.io/specification/2025-11-25/)
  - [Tasks 规范（实验性）](https://modelcontextprotocol.io/specification/2025-11-25/basic/utilities/tasks)
  - [生命周期管理](https://modelcontextprotocol.io/specification/2025-11-25/basic/lifecycle)
  - [传输层](https://modelcontextprotocol.io/specification/2025-11-25/basic/transports)
- [MCP GitHub 仓库](https://github.com/modelcontextprotocol/modelcontextprotocol)
- [Anthropic MCP 文档](https://docs.anthropic.com/en/docs/mcp)
