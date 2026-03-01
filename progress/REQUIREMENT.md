# 需求文档

## 产品信息

- 名称：Neco

- 介绍：

> 原生支持多智能体协作的智能体应用。

---

## 主要解决问题

### 现有多智能体协作方案不完善

多智能体可以用于：

- 并行执行不同操作，提升效率。
- 整理和过滤任务所需信息，保持主模型上下文干净，降低思考干扰和调用成本。

现有的主流AI Agent应用，如Claude Code等各类编码工具、OpenClaw等，在多智能体协作方面，仅提供了以下功能：

- 创建一个子Agent。
- 子Agent任务完成后，接收输出。

当出现异常情况，如任务执行时间过长/偏航等，无法第一时间纠正。

### 工作流固定问题

当前许多开发者、企业等，已经开发出了Agent独有的工作流，例如：

- PRD（需求文档、技术文档、实施计划）
- TDD（测试驱动开发）
- 让模型A开发，模型B检查

但是这个工作流仍然需要手动推进每一步，仍然有自动化空间。

并且我希望这个工作流是可以共享的。

---

## 功能特性

### 多模型配置

个人认为，未来模型会往专用化、细分化的方向发展。

目前因为算力资源有限，大多数国内一线模型厂商只能提供1-2种模型，但各个模型之间的特征差异明显：

- 一部分模型脑子很好，善于思考，灵感似涌泉。
- 一部分模型更擅长执行，执行准确，输出速度快。
- 这两项都很擅长的模型，价格一般不便宜，或者速度不够快。

以及以下细分需求：

- 部分模型有图像/语言识别能力。
- 有多个提供者可选，或有同一提供者的多个API Key，希望循环使用模型，实现负载均衡或避免中断。
  - 当出现异常，尝试3次均失败时，自动尝试下一个可选模型，或当前模型的下一个API Key。

### 模型调用

- 基于OpenAI Chat Completion API。
  - OpenAI API调用使用`async-openai`这个crate。
- 流式输出
- 工具调用
  - 尽可能支持并行化工具调用
- 不需要支持更多功能，因为这些API就可以实现所有功能

- 为后续支持Anthropic、OpenAI Responses、OpenRouter、Github Copilot等预留接口。

### Session管理

- 存储在`~/.local/neco/(session_id)`目录下。目录下存储所有的上下文内容。
- Session ID使用有自增特性的uuid版本。
- 可以使用Jujutsu管理Session目录，每一步提交一次，便于检验，也便于后续添加在特定步骤恢复/回溯等功能。
- 善用Git Workspace或类似功能。

### MCP

- 使用`rmcp`这个crate。
- 同时支持`local`和`http`两种形式。

### Skills

- 参考：[agentskills.io](https://agentskills.io/)。
- 按需加载

### 上下级智能体之间的协作

- 基于`SubAgent`模式。

- 添加上下级智能体之间的沟通工具，上下级模型之间可以直接在会话中传递内容。
- 上级可以要求下级执行汇报。
- 灵感来自现代公司分工。

- 多层智能体树形结构：
  - 最上层智能体直接与用户对话，每个Session只有一个最上层智能体。
  - 每个智能体都可以有多个下级。可以设置例外情况，例如执行智能体只能用于执行。
  - 上层智能体发现任务可以拆分且并行执行时，生成多个下级智能体。
  - 最终会形成一个动态的树形结构。

### 自定义工作流程图

- 使用Mermaid图 + 每个节点一个.md文件表示node

- 使用示例：
  - 定义PRD流程
  - 执行/审阅循环流程

### 模块化提示词与工具，以及按需加载

- 模块化提示词、工具、MCP、Skills等实例的目的是，支持内容的按需加载。
- 添加一个统一的`Activate`工具，用于加载未加载的内容。

---

## 实现要求

- 只使用大语言模型。暂不添加对Embeddings、Rerank、Apply等额外模型的支持。

---

## 工具注意事项

### 1、Read（读取文件）

- 实现 Hashline 技术。Agent 读到的每一行代码，末尾都会打上一个强绑定的内容哈希值，格式类似下文的`1#VK`，称为“行哈希”。

```text
1#VK| function hello() {
2#XJ|   return "world";
3#MB| }
```

- 每一行的哈希值，来源于当前行内容与上一行的哈希值。
- 以上示例仅供格式参考，实际生成的哈希值不一定要与此相同。

### 2、Edit（编辑文件）

传入开始行哈希和结束行哈希（都是闭区间），以及修改后的内容。

---

## 参考配置方式

- 配置目录：`~/.config/neco`
- 本节的所有“配置路径”，都是相对于配置目录的路径。

### 基本配置文件

- 配置路径（按照以下优先级）：
  - 基本（优先级最高）：`neco.toml`
  - 追加：`neco.xxx.toml`，其中的`xxx`可以是任何合法文件名字符串，按照文件名顺序应用。

- 除了TOML格式外，也支持YAML格式，数据定义、配置形式等都一致。所有TOML格式配置文件比所有YAML格式的优先级更高。

- 格式如下：

```toml

# 模型组定义
[model_groups.think]
models = ["zhipuai/glm-4.7"]

[model_groups.balanced]
models = ["zhipuai/glm-4.7", "minimax-cn/MiniMax-M2.5"]

[model_groups.act]
models = ["zhipuai/glm-4.7-flashx"]

[model_groups.image]
models = ["zhipuai/glm-4.6v"]

# 以下设置应内置于代码中
[model_providers.zhipuai]
type = "openai" # 使用OpenAI Chat接口
name = "ZhipuAI"
base = "https://open.bigmodel.cn/api/paas/v4"
env_key = "ZHIPU_API_KEY"

# 以下设置应内置于代码中
[model_providers.zhipuai-coding-plan]
type = "openai" # 使用OpenAI Chat接口
name = "ZhipuAI Coding Plan"
base = "https://open.bigmodel.cn/api/coding/paas/v4"
env_key = "ZHIPU_API_KEY"

# MiniMax参考配置
[model_providers.minimax-cn]
type = "openai" # 使用OpenAI Chat接口
name = "MiniMax (CN)"
base = "https://api.minimaxi.com/v1"
env_keys = ["MINIMAX_API_KEY", "MINIMAX_API_KEY_2"]

# MCP参考：本地形式
[mcp_servers.context7]
command = "npx"
args = ["-y", "@upstash/context7-mcp"]

[mcp_servers.context7.env]
MY_ENV_VAR = "MY_ENV_VALUE"

# MCP参考：HTTP形式
[mcp_servers.figma]
url = "https://mcp.figma.com/mcp"
bearer_token_env_var = "FIGMA_OAUTH_TOKEN"
http_headers = { "X-Figma-Region" = "us-east-1" }
```

### 提示词组件定义

- 路径：`prompts/xxx.md`
- 单个Markdown文件即为一个提示词组件，用于插入提示词。
- 该Markdown文件的内容即为该组件的提示词。
- 无头部信息。`xxx`即为这个提示词组件的`name`。

#### 内置提示词组件

- `base`：任何时候都加载。包含如何加载未加载的内容的提示。
- `multi-agent`：如果这个Agent可以生成子Agent，则加载。
- `multi-agent-child`：如果这个模型有父Agent，则加载。

#### 工具提示词组件

- 在工具定义处，随工具加载。

### Agent定义

- 路径：`agents/xxx.md`
- 单个Markdown文件即为一个Agent定义。
- 该Markdown文件的内容即为该Agent的提示词。

#### Agent头部信息

```yaml
# （可选）激活的提示词组件。按顺序激活。
# 默认：只有base
prompts:
  - base
  - multi-agent 
```

---

## 用户接口

基本的运行逻辑都一致，只在界面上有区别。

### A. 直接输入输出

传入`-m 消息内容`参数，直接执行，输出结果。

- 输出结束后也输出`--session xxxxxxxx`参考参数，用于接续对话上下文。（Session管理部分见下文）
- 使用`ratatui`渲染。
- 使用ratatui的`Viewport::Inline`模式，非全屏TUI。
  - 按行渲染输出，不切换到alternate screen（全屏）。
  - 保留终端历史记录，TUI在当前光标下方显示。

### B. 终端REPL

- 使用ratatui的`Viewport::Inline`模式，非全屏TUI。
  - REPL界面不占用全屏，保留终端历史记录。

### C. 后台运行模式

- 参考OpenClaw、ZeroClaw等应用的运行模式。
- 要求类型设计参考该项目：[ZeroClaw](https://github.com/zeroclaw-labs/zeroclaw)

### 要求

- 模式A和B都使用`ratatui`的`Viewport::Inline`模式（非全屏TUI），不使用alternate screen（全屏模式）。
- 模式A和B共享渲染逻辑。
- 以下逻辑要求分离至不同crate：
  - 核心执行逻辑
  - 终端输出逻辑
  - 后台Agent与外部接口
