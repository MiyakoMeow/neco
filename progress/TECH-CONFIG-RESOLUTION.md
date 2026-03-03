# TECH-CONFIG-RESOLUTION

## 1. 范围

本文件描述配置解析内部流程：文件发现顺序、合并策略、Key 优先级、工作流局部覆盖与 Agent 查找链路。

## 2. 配置面结构

```mermaid
flowchart TD
    ROOT[~/.config/neco] --> DISC[Config Discovery]
    DISC --> ORD[Ordered Layers]
    ORD --> MERGE[Merge Engine]
    MERGE --> SNAP[Runtime Config Snapshot]
    SNAP --> RES_AGENT[Agent Resolver]
    SNAP --> RES_MODEL[Model/Provider Resolver]
    SNAP --> RES_MCP[MCP Resolver]
```

## 3. 关键数据结构（伪类型）

```text
ConfigLayer {
  source_path
  format            // toml|yaml
  load_order
  payload
}

MergedConfig {
  model_groups
  model_providers
  mcp_servers
  prompts
  agents
  workflows
}

ApiKeyPolicy {
  single_env?
  multi_envs[]
  inline_key?
}
```

## 4. 解析顺序（内部算法）

```mermaid
flowchart LR
    A[Collect candidate files] --> B[Sort by format priority]
    B --> C[Sort by filename priority]
    C --> D[Apply merge layer by layer]
    D --> E[Validate required fields]
    E --> F[Emit snapshot]
```

层顺序规则在引擎内编码，不在调用方重复判断。

## 5. 合并策略模型

```text
for each key:
  if scalar: overwrite by later layer
  if array: replace by later layer
    if item starts with "+":
      append semantic item after removing "+"
  if object: recursive merge
```

## 6. API Key 解析流

```mermaid
flowchart TD
    S[Provider Config] --> E1{api_key_env exists?}
    E1 -- yes --> K1[read single env key]
    E1 -- no --> E2{api_key_envs exists?}
    E2 -- yes --> K2[build key ring for rotation]
    E2 -- no --> E3{api_key exists?}
    E3 -- yes --> K3[use inline key]
    E3 -- no --> FAIL[config error]
```

## 7. 工作流局部配置覆盖

```mermaid
flowchart LR
    WFROOT[workflows/xxx/] --> LCFG[workflow local config/prompts/agents/skills]
    GCFG[global ~/.config/neco] --> RES[Resolver]
    LCFG --> RES
    RES --> OUT[Effective workflow snapshot]
```

Agent 查找优先级：

1. `workflows/xxx/agents/`
2. 全局 `~/.config/neco/agents/`

## 8. 伪代码

```text
function load_effective_config(context):
  layers = discover_and_sort(context)
  cfg = empty
  for layer in layers:
    cfg = merge(cfg, layer.payload)
  validate(cfg)
  return cfg

function resolve_agent(workflow_id, agent_name):
  if exists(workflow_local_agent): return workflow_local_agent
  return global_agent
```

## 9. 生命周期约束

1. 启动时校验失败立即终止。
2. 运行时热加载失败回滚到上一快照，并记录错误日志。
