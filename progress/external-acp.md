# Agent Client Protocol (ACP) 参考指南

> **协议版本**: v1 (最新版本 v0.10.8，2026年2月)  
> **官方仓库**: https://github.com/agentclientprotocol/agent-client-protocol  
> **官方网站**: https://agentclientprotocol.com/  
> **重要更新**: Session Config Options 已成为推荐方式，Session Modes 将在未来版本中移除

---

## 目录

1. [协议概述](#1-协议概述)
2. [核心概念](#2-核心概念)
3. [协议方法详解](#3-协议方法详解)
4. [SDK与集成](#4-sdk与集成)
5. [协议对比](#5-协议对比)

---

## 1. 协议概述

### 1.1 什么是 ACP

Agent Client Protocol (ACP) 是一个开放标准，用于标准化代码编辑器（IDE、文本编辑器等）与 AI 编码代理之间的通信。由 Zed Industries 主导开发。

**核心定位**: 类似于 LSP 标准化语言服务器集成，ACP 标准化了 AI 代理与编辑器的集成，实现"一次实现，到处运行"。

### 1.2 设计原则

- **可信优先**: 编辑器保留对代理工具调用的控制权
- **用户体验优先**: 解决与 AI 代理交互的 UX 挑战
- **MCP 友好**: 基于 JSON-RPC 2.0，重用 MCP 类型定义

### 1.3 通信模型

ACP 遵循 JSON-RPC 2.0 规范，支持两种消息类型：

| 类型 | 描述 | 用途 |
|------|------|------|
| **Methods** | 请求-响应对 | 需要返回结果或错误的操作 |
| **Notifications** | 单向消息 | 不需要响应的事件通知 |

### 1.4 协议生命周期

```
初始化阶段 → 会话建立阶段 → 提示词交互阶段
```

1. **初始化**: 客户端调用 `initialize` 协商版本和能力
2. **会话建立**: 调用 `session/new` 创建新会话或 `session/load` 加载已有会话
3. **提示词交互**: 通过 `session/prompt` 发送用户消息，接收 `session/update` 流式更新

---

## 2. 核心概念

### 2.1 架构组件

```
┌─────────────┐     JSON-RPC      ┌─────────────┐
│   Client    │ ←──────────────→  │   Agent     │
│ (编辑器)     │   Stdio/HTTP/WS   │ (AI代理)     │
└─────────────┘                   └─────────────┘
      │                                  │
      ├─ 文件系统                         ├─ LLM
      ├─ 终端                            ├─ 工具
      └─ 用户界面                        └─ MCP服务器
```

#### 客户端 (Client)
- 管理环境（工作目录、文件系统）
- 控制资源访问权限
- 处理用户交互
- 启动和管理代理进程

#### 代理 (Agent)
- 处理客户端请求
- 使用语言模型和工具执行任务
- 管理会话状态和上下文
- 发送流式更新

### 2.2 传输层

ACP 支持多种传输机制：

- **Stdio**: 标准输入输出（本地进程）
- **HTTP**: HTTP 请求（远程代理）
- **WebSocket**: 实时双向通信

### 2.3 内容块 (Content Blocks)

ACP 使用与 MCP 相同的内容块结构：

| 类型 | 描述 | 能力要求 |
|------|------|----------|
| `text` | 纯文本内容 | 必需 |
| `image` | 图像数据 | `image` |
| `audio` | 音频数据 | `audio` |
| `resource` | 嵌入资源 | `embeddedContext` |
| `resource_link` | 资源链接 | 必需 |

### 2.4 配置管理：Session Config Options vs Session Modes

**重要变更**: Session Config Options 现在是推荐的方式，Session Modes 将在未来版本中被移除。

| 特性 | Session Config Options | Session Modes |
|------|----------------------|---------------|
| **状态** | ✅ 推荐 | ⚠️ 即将弃用 |
| **灵活性** | 高（支持多种配置类型） | 低（仅模式切换） |
| **扩展性** | 可自定义配置类别 | 固定模式集 |
| **向后兼容** | - | 过渡期内可用 |

在过渡期间，代理应同时提供两者以保持向后兼容性。

---

## 3. 协议方法详解

### 3.1 代理端方法 (Agent Methods)

#### 3.1.1 `initialize` ⭐ 必需

**用途**: 建立连接并协商协议版本和能力

**请求参数**:
- `protocolVersion` (number, 必需): 客户端支持的协议版本
- `clientCapabilities` (ClientCapabilities): 客户端能力声明
- `clientInfo` (ClientInfo, 可选): 客户端名称和版本

**响应结果**:
- `protocolVersion` (number): 协商的协议版本
- `agentCapabilities` (AgentCapabilities): 代理能力声明
- `agentInfo` (AgentInfo, 可选): 代理名称和版本
- `authMethods` (AuthMethod[], 可选): 支持的认证方法

#### 3.1.2 `authenticate`

**用途**: 使用指定方法进行身份验证

**请求参数**:
- `methodId` (string, 必需): 认证方法 ID（来自 initialize 响应）

**响应结果**: 空对象表示成功

#### 3.1.3 `session/new` ⭐ 必需

**用途**: 创建新的对话会话

**请求参数**:
- `cwd` (string, 必需): 工作目录（绝对路径）
- `mcpServers` (McpServer[], 必需): MCP 服务器列表

**响应结果**:
- `sessionId` (string, 必需): 会话唯一标识符
- `configOptions` (SessionConfigOptions, 可选): 初始配置选项
- `modes` (SessionModes, 可选): 初始模式状态

**错误**: 可能返回 `auth_required` 错误，表示需要先认证

#### 3.1.4 `session/load`

**用途**: 加载已有会话（需要 `loadSession` 能力）

**请求参数**:
- `sessionId` (string, 必需): 要加载的会话 ID
- `cwd` (string, 必需): 工作目录
- `mcpServers` (McpServer[], 必需): MCP 服务器列表

**响应结果**:
- `configOptions` (SessionConfigOptions, 可选): 配置选项
- `modes` (SessionModes, 可选): 模式状态

**行为**: 代理会通过 `session/update` 通知流式发送完整对话历史

#### 3.1.5 `session/prompt` ⭐ 必需

**用途**: 发送用户提示词到代理

**请求参数**:
- `sessionId` (string, 必需): 目标会话 ID
- `prompt` (ContentBlock[], 必需): 用户消息内容块数组

**响应结果**:
- `stopReason` (StopReason, 必需): 停止原因
  - `end_turn`: 代理完成响应
  - `cancelled`: 客户端取消
  - `max_tokens`: 达到 token 限制
  - `error`: 发生错误

**行为**: 
1. 代理通过 `session/update` 通知发送流式更新
2. 可能调用客户端方法（读取文件、创建终端等）
3. 可能通过 `session/request_permission` 请求权限
4. 最终返回 `stopReason`

#### 3.1.6 `session/set_mode` ⚠️ 即将弃用

**用途**: 切换会话模式（如 "ask"、"architect"、"code"）
> **注意**: 此方法将在未来版本中被移除，推荐使用 `session/set_config_option` 代替

**请求参数**:
- `sessionId` (string, 必需)
- `modeId` (string, 必需): 模式 ID（必须是 availableModes 之一）

**响应结果**: 空对象

#### 3.1.7 `session/set_config_option` ⭐ 推荐

**用途**: 设置会话配置选项
> **推荐**: 这是配置会话的首选方式，替代了即将弃用的 Session Modes API

**请求参数**:
- `sessionId` (string, 必需)
- `configId` (string, 必需): 配置选项 ID
- `value` (string, 必需): 配置值 ID

**响应结果**:
- `configOptions` (SessionConfigOptions, 必需): 更新后的完整配置

**说明**:
- Config Options 支持模型选择、推理级别等多种配置
- 代理应始终提供默认值以确保兼容性
- 响应包含完整配置状态，可反映依赖性变化

#### 3.1.8 `session/cancel` (通知)

**用途**: 取消正在进行的操作

**参数**:
- `sessionId` (string, 必需)

**行为**: 代理应停止所有 LLM 请求和工具调用，发送待处理的 `session/update` 通知，然后返回 `stopReason: "cancelled"`

---

### 3.2 客户端方法 (Client Methods)

#### 3.2.1 `session/request_permission` ⭐ 必需

**用途**: 请求用户授权执行工具调用

**请求参数**:
- `sessionId` (string, 必需)
- `toolCall` (ToolCall, 必需): 工具调用详情
- `options` (PermissionOption[], 必需): 用户可选的权限选项

**响应结果**:
- `outcome` (RequestPermissionOutcome, 必需): 用户决定
  - `granted`: 授权
  - `denied`: 拒绝
  - `cancelled`: 取消提示

#### 3.2.2 `fs/read_text_file`

**用途**: 读取文本文件内容（需要 `fs.readTextFile` 能力）

**请求参数**:
- `sessionId` (string, 必需)
- `path` (string, 必需): 文件绝对路径
- `line` (number, 可选): 起始行号（1-based）
- `limit` (number, 可选): 最大读取行数

**响应结果**:
- `content` (string, 必需): 文件内容

#### 3.2.3 `fs/write_text_file`

**用途**: 写入文本文件（需要 `fs.writeTextFile` 能力）

**请求参数**:
- `sessionId` (string, 必需)
- `path` (string, 必需): 文件绝对路径
- `content` (string, 必需): 写入内容

**响应结果**: 空对象

#### 3.2.4 `terminal/create`

**用途**: 创建新终端并执行命令（需要 `terminal` 能力）

**请求参数**:
- `sessionId` (string, 必需)
- `command` (string, 必需): 要执行的命令
- `args` (string[], 可选): 命令参数
- `cwd` (string, 可选): 工作目录
- `env` (object, 可选): 环境变量
- `outputByteLimit` (number, 可选): 输出字节限制

**响应结果**:
- `terminalId` (string, 必需): 终端唯一标识符

#### 3.2.5 `terminal/output`

**用途**: 获取终端输出和退出状态

**请求参数**:
- `sessionId` (string, 必需)
- `terminalId` (string, 必需)

**响应结果**:
- `output` (string, 必需): 终端输出
- `exitStatus` (number, 可选): 退出状态码
- `truncated` (boolean, 必需): 是否被截断

#### 3.2.6 `terminal/wait_for_exit`

**用途**: 等待终端命令退出

**请求参数**:
- `sessionId` (string, 必需)
- `terminalId` (string, 必需)

**响应结果**:
- `exitCode` (number, 可选): 进程退出码
- `signal` (string, 可选): 终止信号

#### 3.2.7 `terminal/kill`

**用途**: 终止终端命令但不释放终端

**请求参数**:
- `sessionId` (string, 必需)
- `terminalId` (string, 必需)

**响应结果**: 空对象

#### 3.2.8 `terminal/release`

**用途**: 释放终端资源（会终止命令）

**请求参数**:
- `sessionId` (string, 必需)
- `terminalId` (string, 必需)

**响应结果**: 空对象

---

### 3.3 会话更新 (Session Update Notifications)

#### 3.3.1 `session/update` (通知) ⭐ 核心

**用途**: 代理向客户端发送流式更新

**参数**:
- `sessionId` (string, 必需)
- `update` (SessionUpdate, 必需): 更新内容

**更新类型**:

| 类型 | 描述 |
|------|------|
| `agent_message_chunk` | 代理文本响应片段 |
| `user_message_chunk` | 用户消息片段 |
| `thought_message_chunk` | 代理推理过程片段 |
| `tool_call` | 新工具调用 |
| `tool_call_update` | 工具调用进度/结果 |
| `plan` | 代理执行计划 |
| `available_commands` | 可用命令列表 |
| `current_mode_update` | 当前模式更新 |
| `config_option_update` | 配置选项更新（推荐方式） |
| `config_options_update` | 完整配置选项状态更新 |

---

## 4. SDK与集成

### 4.1 官方 SDK

| 语言 | SDK 包名 | 仓库 | 版本 |
|------|---------|------|------|
| **Rust** | `agent-client-protocol` | 官方参考实现 | - |
| **Python** | `agent-client-protocol` | python-sdk | - |
| **TypeScript** | `@agentclientprotocol/sdk` | typescript-sdk | - |
| **Kotlin** | `acp-kotlin` | kotlin-sdk | 0.1.0-SNAPSHOT |

### 4.2 社区 SDK

| 语言 | SDK 包名 | 说明 |
|------|---------|------|
| Dart | acp_dart | 社区实现 |
| Emacs Lisp | acp.el | Emacs 集成 |
| Go | acp-go-sdk | 社区实现 |
| React | use-acp | React Hook |
| Swift | swift-acp / swift-sdk | 社区实现（多个） |

### 4.3 编辑器支持

| 编辑器 | 状态 |
|--------|------|
| **Zed** | 原生支持，主要推动者 |
| **JetBrains** | 正在集成中 |
| **VS Code** | 社区插件支持 |

### 4.4 代理支持

| 代理 | 状态 |
|------|------|
| **Claude Code** | 原生支持 |
| **Gemini CLI** | 实验性支持 (`--experimental-acp`) |
| **OpenCode** | 完整支持 |

---

## 5. 协议对比

### 5.1 ACP vs MCP vs A2A

| 特性 | ACP | MCP | A2A |
|------|-----|-----|-----|
| **目标** | 编辑器 ↔ 代理 | 代理 ↔ 工具/数据源 | 代理 ↔ 代理 |
| **协议基础** | JSON-RPC 2.0 | JSON-RPC 2.0 | JSON-RPC 2.0 |
| **传输层** | Stdio/HTTP/WS | Stdio/HTTP/SSE | 待定 |
| **复杂度** | 中等 | 较低 | 较高 |
| **典型场景** | AI 编程助手 | 工具调用 | 多代理协作 |

### 5.2 协议选择

```
需要连接编辑器 → 使用 ACP
需要访问工具/数据 → 使用 MCP
需要代理间通信 → 使用 A2A
```

### 5.3 ACP 与 MCP 的协同

ACP 代理可以同时作为 MCP 客户端：

```
编辑器 → ACP → AI代理 → MCP → 工具/数据源
```

这种设计允许代理通过 MCP 访问丰富的工具生态系统。

---

## 附录

### A. 能力声明 (Capabilities)

#### 客户端能力 (ClientCapabilities)
```typescript
{
  fs: {
    readTextFile: boolean,
    writeTextFile: boolean
  },
  terminal: boolean
}
```

#### 代理能力 (AgentCapabilities)
```typescript
{
  loadSession: boolean,
  promptCapabilities: {
    image: boolean,
    audio: boolean,
    embeddedContext: boolean
  },
  mcpCapabilities: {
    http: boolean,
    sse: boolean
  }
}
```

### B. 停止原因 (StopReason)

- `end_turn`: 代理正常完成响应
- `cancelled`: 客户端取消操作
- `max_tokens`: 达到 token 限制
- `error`: 发生错误

### C. 快速开始

1. **阅读官方文档**: https://agentclientprotocol.com/
2. **选择 SDK**: Python/TypeScript 最易上手
3. **运行示例**: 
   ```bash
   # Python
   pip install agent-client-protocol
   python examples/echo_agent.py
   ```
4. **配置编辑器**: 在 Zed settings.json 中添加 agent_servers 配置

---

*文档更新时间: 2026年2月27日*  
*协议版本: v1 (v0.10.8，2026年2月4日)*  
*重要: Session Config Options 已成为推荐配置方式，Session Modes 将在未来版本中移除*
