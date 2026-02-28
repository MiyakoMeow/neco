# 需求文档

## 产品信息

- 名称：Neco

- 介绍：

> 原生支持多智能体协作的智能体应用。

## 主要解决问题

### 1. 现有多智能体协作方案不完善

多智能体可以用于：

- 并行执行不同操作，提升效率。
- 整理和过滤任务所需信息，保持主模型上下文干净，降低思考干扰和调用成本。

现有的主流AI Agent应用，如Claude Code等各类编码工具、OpenClaw等，在多智能体协作方面，仅提供了以下功能：

- 创建一个子Agent。
- 子Agent任务完成后，接收输出。

当出现异常情况，如任务执行时间过长/偏航等，无法第一时间纠正。

## 其它功能特性

### 一、用户接口

基本的运行逻辑都一致，只在界面上有区别。

#### A. 直接输入输出

传入`-m 消息内容`参数，直接执行，输出结果。

- 输出结束后也输出`--session xxxxxxxx`参考参数，用于接续对话上下文。（Session管理部分见下文）
- 使用`ratatui`渲染。
- 使用按行渲染，不使用tui。

#### B. 终端REPL

- 使用`ratatui`渲染。

#### C. 后台运行模式

- 参考OpenClaw、ZeroClaw等应用的运行模式。
- [ZeroClaw](https://github.com/zeroclaw-labs/zeroclaw)

#### 要求

- 使用`ratatui`这个tui库。
- 模型运行和终端输出逻辑分离。

## 二、多模型配置

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

### 三、模型调用

- 基于OpenAI Chat Completion API。
  - OpenAI API调用使用`async-openai`这个crate。
- 流式输出
- 工具调用
  - 尽可能支持并行化工具调用
- 不需要支持更多功能，因为这些API就可以实现所有功能

- 为后续支持Anthropic、OpenAI Responses、OpenRouter、Github Copilot等预留接口。

### 四、Session管理

- 存储在`~/.local/neco/(session_id)`目录下。目录下存储所有的上下文内容。
- Session ID使用有自增特性的uuid版本。
- 可以使用Jujutsu管理Session目录，每一步提交一次，便于检验，也便于后续添加在特定步骤恢复/回溯等功能。
- 善用Git Workspace或类似功能。

### 五、其它AI Agent基础功能

- MCP：使用`rmcp`这个crate。
- Skills：参考[agentskills.io](https://agentskills.io/)。
  - 懒加载基础实现
- 多Agent并行（参见上文）

#### 可选部分

- 脚本化工具调用
  - 参考文档：[Programmatic Tool Calling](https://platform.claude.com/docs/en/agents-and-tools/tool-use/programmatic-tool-calling)

### 六、记忆系统

目前记忆系统已经有很多方案，例如Mem0、OpenViking。

#### A. 信息获取

现有的方案的执行方式是，在代码/插件层面，截获上下文内容，然后在外部新建一个模型会话进行整理。
这个方法能够获取到完整的上下文内容，但也会出现多轮对话导致的缓存读额外开销。

- 个人推荐：在多Agent状态下，记忆系统只需要存以下内容：
  - Explore的结果/报告。
  - 每次对话解决的问题，以及在此过程中用户的偏好。

#### B. 信息整理

看了一下现有方案：

- Mem0：是个外置数据库，实现未知，需要提供专用API Key。
- OpenClaw：双层存储（`MEMORY.md`/`SOUL.md`等 + `YYYY-MM-DD.md`），单层记忆加载。官方推荐使用字符串查询/Embedding等方式加速搜索。
- OpenViking：功能完备，三层记忆加载框架。必需图像处理模型，Embeddings模型。

- 个人推荐：实现一个简单的记忆管理系统。
  - 记忆加载，最多两层就足够了。这两层分别是：
    1. （始终加载）标题 + 摘要
    2. （模型主动加载）完整内容
    - 一旦模型决定激活记忆内容，一般来说都会需要完整内容。
  - 记忆存储：
    - `MEMORY.md`等全局记忆，只用于记录必要信息，例如用户偏好等。这一点同 OpenClaw。
    - 使用类似图书馆/记忆库的形式，管理记忆内容。
      - 按照用户记忆/特定目录记忆分类。如果在特定目录开始会话，只提供全局+该目录对应记忆。
  - 暂不使用Embeddings模型，但应支持字符串搜索。

### 八、子智能体与上下级智能体见的协作

- 添加上下级智能体之间的沟通工具，上下级模型之间可以直接在会话中传递内容。上级可以要求下级执行汇报。
- 灵感来自现代公司分工。

- 多层智能体树形结构：
  - 最上层智能体直接与用户对话，每个Session只有一个最上层智能体。
  - 每个智能体都可以有多个下级。可以设置例外情况，例如执行智能体只能用于执行。
  - 上层智能体发现任务可以拆分且并行执行时，生成多个下级智能体。
  - 最终会形成一个动态的树形结构。

### 九、自定义工作流程图

- 使用Mermaid图 + 每个节点一个.md文件表示node

- 使用示例：
  - 定义PRD流程
  - 执行/审阅循环流程

### 十、模块化提示词与工具，以及按需加载

- 模块化提示词、工具、MCP、Skills等实例的目的是，支持内容的按需加载。
- 添加一个统一的`Activate`工具，用于加载未加载的内容。

#### 提示词模块化

- `base`：任何时候都加载。包含如何加载未加载的内容的提示。
- `tool:*`：各个工具。默认只加载基础工具（Read/Write等，以及用于加载的
- `multi-agent-parent`：如果这个模型可以生成子模型则加载

## 实现要求

- 只使用大语言模型。暂不添加对Embeddings、Rerank、Apply等额外模型的支持。

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
