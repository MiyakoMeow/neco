# 需求文档

## 产品信息

- 名称：Neco

- 介绍：

> 原生支持多智能体协作的智能体应用。

## 解决什么问题？如何解决？

### 1. 现有多智能体协作方案不完善

多智能体可以用于：
- 并行执行不同操作，提升效率。
- 整理和过滤任务所需信息，保持主模型上下文干净，降低思考干扰和调用成本。

现有的主流AI Agent应用，如Claude Code等各类编码工具、OpenClaw等，在多智能体协作方面，仅提供了以下功能：
- 创建一个子Agent。
- 子Agent任务完成后，接收输出。

当出现异常情况，如任务执行时间过长/偏航等，无法第一时间纠正。

#### 如何解决？

- 添加上下级智能体之间的沟通工具，上下级模型之间可以直接在会话中传递内容。上级可以要求下级执行汇报。
- 灵感来自现代公司分工。

- 多层智能体树形结构：
  - 最上层智能体直接与用户对话，每个Session只有一个最上层智能体。
  - 每个智能体都可以有多个下级。可以设置例外情况，例如执行智能体只能用于执行。
  - 上层智能体发现任务可以拆分且并行执行时，生成多个下级智能体。
  - 最终会形成一个动态的树形结构。

### 2. 记忆系统

目前记忆系统已经有很多方案，例如Mem0、OpenViking。

#### A. 信息获取

现有的方案的执行方式是，在代码/插件层面，截获上下文内容，然后在外部新建一个模型会话进行整理。
这个方法能够获取到完整的上下文内容，但也会出现多轮对话导致的缓存读额外开销。

- 个人推荐：在多Agent状态下，记忆系统只需要存以下内容：
  - Explore的结果/报告。
  - 每次对话解决的问题，以及在此过程中用户的偏好。

#### B. 信息整理

看了一下现有方案：

- OpenClaw、Mem0等：单个MEMORY.md。容易造成记忆内容膨胀。
- OpenViking：功能完备，三层记忆框架。需要图像处理模型和Embeddings模型。

- 个人推荐：实现一个简单的记忆管理系统。
  - 最多两层就足够了。这两层分别是：
    1. 标题名字 + 摘要
    2. 完整内容
    - 一旦模型决定激活记忆内容，一般来说都会需要完整内容。
  - MEMORY.md只用于记录必要信息，例如用户偏好等。应尽可能精简。
  - 使用类似图书馆/记忆库的形式，管理记忆内容。
    - 按照用户记忆/特定目录记忆分类。如果在特定目录开始会话，只提供全局+该目录对应记忆。

### 3. 多模型配置

个人认为，未来模型会往专用化、细分化的方向发展。

目前因为算力资源有限，大多数国内一线模型厂商只能提供1-2种模型，但各个模型之间的特征差异明显：
- 一部分模型脑子很好，善于思考，灵感似涌泉。
- 一部分模型更擅长执行，执行准确，输出速度快。
- 这两项都很擅长的模型，价格一般不便宜，或者速度不够快。

以及以下细分需求：
- 部分模型有图像/语言识别能力。
- 有多个提供者可选，或有同一提供者的多个API Key，希望循环使用模型，实现负载均衡或避免中断。
  - 当出现异常，尝试3次均失败时，自动尝试下一个可选模型，或当前模型的下一个API Key。

- 参考配置方式：

```toml
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
```

## 预期功能列表

### 一、用户接口

- 按照以下顺序实现：

1. 传入`-m 消息内容`参数，直接执行，输出结果。
  - 输出结束后也输出`--session xxxxxxxx`参考参数，用于接续对话上下文。
2. 实现终端REPL，以及ACP模式。[ACP SDK](https://github.com/agentclientprotocol/agent-client-protocol)
3. 实现后台运行的智能模式。

基本的运行逻辑都一致，只在界面上有区别。

#### 要求

- 使用`ratatui`这个tui库。
- 模型运行和终端输出逻辑分离。

### 二、基础功能

#### A. 模型调用基础

- 基于OpenAI Chat API。
  - Anthropic、OpenAI Responses、OpenRouter、Github Copilot等将在后续支持。
- 流式输出
- 工具调用
  - 尽可能支持并行化工具调用
- Session管理
  - 存储在`~/.local/neco/(session_id)`目录下。目录下存储所有的上下文内容。
  - Session ID使用有自增特性的uuid版本。
  - 可以使用Jujutsu管理Session目录，每一步提交一次，便于检验，也便于后续添加在特定步骤恢复/回溯等功能。
  - 善用Git Workspace或类似功能。

#### B. 扩展

- MCP
- Skills
  - 懒加载基础实现
- 多Agent并行（参见上文）
- 
- （可选）脚本化工具调用
  - 参考文档：[Programmatic Tool Calling](https://platform.claude.com/docs/en/agents-and-tools/tool-use/programmatic-tool-calling)

#### C. 可配置性

- MCP懒加载。
- OpenClaw扩展支持
- 外部上下文管理系统支持

## 要求

- 只使用大语言模型。暂不添加对Embeddings、Rerank、Apply等额外模型的支持。

## 参考

- [ZeroClaw](https://github.com/zeroclaw-labs/zeroclaw)
- OpenAI API调用使用`async-openai`这个crate。
- MCP部分使用`rmcp`。
- Skills部分参考：https://agentskills.io/