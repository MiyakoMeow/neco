# Agent Skills 深度探索报告

> 探索日期：2026-02-27
> 探索目标：彻底了解 Agent Skills 系统的设计理念、技术架构、实现方式

---

## 目录

- [1. 核心概念与设计理念](#1-核心概念与设计理念)
- [2. 技术架构](#2-技术架构)
- [3. 渐进式披露机制](#3-渐进式披露机制)
- [4. Skills 规范详解](#4-skills-规范详解)
- [5. 发现、安装与激活](#5-发现安装与激活)
- [6. 生态系统分析](#6-生态系统分析)
- [7. 安全与验证](#7-安全与验证)
- [8. 典型应用场景](#8-典型应用场景)
- [9. Neco 集成参考](#9-neco-集成参考)

---

## 1. 核心概念与设计理念

### 1.1 什么是 Agent Skills？

**Agent Skills** 是由 Anthropic 开发的开放标准，用于扩展 AI Agent 能力的模块化格式。

> "Skills are folders of instructions, scripts, and resources that agents can discover and use to perform better at specific tasks."

**核心组成：**
- **任务执行指南**（SOP 和背景知识）
- **工具使用说明**（操作方法和命令）
- **模板与资源**（历史案例、格式标准）
- **问题处理方案**（规范和最佳实践）

### 1.2 设计哲学

| 原则 | 说明 |
|-----|------|
| **简单性** | 最小结构只需一个 SKILL.md 文件 |
| **开放性** | 开放标准，多家产品支持 |
| **可移植性** | 跨平台、跨产品复用 |
| **渐进式** | 按需加载，优化上下文使用 |
| **可扩展性** | 从简单文本到复杂脚本 |

### 1.3 与 MCP 的区别

| 维度 | MCP (Model Context Protocol) | Agent Skills |
|-----|------------------------------|--------------|
| **关注点** | 如何调用外部工具、数据与服务 | 如何端到端完成特定工作 |
| **定义内容** | 统一的协议和接口 | 执行方法、工具调用范式 + 相关知识材料 |
| **内容类型** | 接口规范 | 完整的「能力扩展包」 |
| **典型用途** | 连接外部 API 和数据源 | 教会 Agent 完成特定任务 |

**类比：**
- MCP = 统一的充电标准（USB-C）
- Skills = 设备本身（充电宝、耳机、外接显卡）

---

## 2. 技术架构

### 2.1 整体架构

Agent Skills 系统分为三层：

1. **存储层**：文件系统中的 Skills 目录
2. **管理层**：发现、索引、匹配、加载
3. **执行层**：Agent 调用 Skill 完成任务

### 2.2 核心 API 设计

#### 元数据 API

启动时扫描所有 Skills，提取 YAML frontmatter：

```yaml
---
name: skill-name              # 技能标识符（必需）
description: when to use     # 触发描述（必需）
license: Apache-2.0          # 许可证（可选）
metadata:                    # 自定义元数据（可选）
  author: example-org
  version: "1.0"
---
```

**字段约束：**
- `name`: 1-64 字符，仅小写字母、数字、连字符
- `description`: 1-1024 字符，描述功能 + 使用时机

#### 系统提示注入

将 Skills 元数据注入系统提示的 XML 结构：

```xml
<available_skills>
  <skill>
    <name>pdf-processing</name>
    <description>Extracts text and tables from PDF files...</description>
    <location>/path/to/skills/pdf-processing/SKILL.md</location>
  </skill>
</available_skills>
```

### 2.3 两种集成方式

| 方式 | 适用场景 | 访问方法 | 优势 |
|-----|---------|---------|------|
| **文件系统模式** | 有完整计算机环境的 Agent（如 Claude Code） | 直接通过 bash 命令访问文件 | 完整文件访问、直接执行脚本、资源访问无限制 |
| **工具调用模式** | 无独立环境的 Agent（如 Claude.ai） | 通过工具函数访问 Skills | 沙箱化安全、跨平台兼容、易于集成 |

---

## 3. 渐进式披露机制

### 3.1 三层加载体系

**Level 1: 元数据层**（始终加载，~100 tokens）
- **内容**：YAML frontmatter
- **时机**：Agent 启动时
- **作用**：让 Agent 知道有哪些 Skills 可用，何时使用

**Level 2: 指令层**（触发时加载，< 5000 tokens）
- **内容**：SKILL.md 的 Markdown 正文
- **时机**：任务匹配 Skill description 时
- **作用**：提供详细的任务指导

**Level 3: 资源层**（按需加载，无限制）
- **内容**：scripts/、references/、assets/ 文件
- **时机**：指令中明确引用时
- **作用**：提供可执行代码、参考文档、模板资源

### 3.2 上下文窗口变化

```
初始状态：System Prompt + Skill Metadata (all skills)
   ↓ 触发 Skill
加载 Level 2：+ SKILL.md (正文)
   ↓ 需要资源
加载 Level 3：+ REFERENCE.md (按需)
```

### 3.3 设计优势

| 传统方案 | 渐进式披露 |
|---------|-----------|
| 所有指令都在系统提示中，占用大量上下文 | 只加载元数据，上下文占用极小 |
| 无法按需加载详细内容 | 三层加载，按需扩展 |
| 添加新 Skill 会显著增加上下文 | 可安装大量 Skills 而不影响性能 |

---

## 4. Skills 规范详解

### 4.1 目录结构

```
skill-name/
├── SKILL.md          # 必需：核心指令 + 元数据
├── scripts/          # 可选：可执行代码
├── references/       # 可选：参考文档
└── assets/           # 可选：静态资源
```

### 4.2 SKILL.md 格式规范

#### 必需字段

```yaml
---
name: skill-identifier
description: A clear description of what this skill does and when to use it
---
```

**name 字段规则：**
- 1-64 字符
- 仅小写字母（a-z）、数字（0-9）、连字符（-）
- 不能以 `-` 开头或结尾
- 不能包含连续连字符（`--`）
- 必须与父目录名匹配

**description 字段规则：**
- 1-1024 字符
- 应描述**功能**和**使用时机**
- 包含特定关键词以帮助 Agent 识别相关任务

#### 可选字段

- `license`: 许可证名称或文件引用
- `compatibility`: 兼容性说明（产品、工具依赖）
- `metadata`: 自定义元数据（作者、版本、标签等）
- `allowed-tools`: 允许使用的工具列表（实验性）

### 4.3 资源目录规范

**scripts/** - 可执行代码
- 用途：提供可复用的工具脚本
- 要求：自包含、清晰的依赖、友好的错误消息
- 支持语言：Python、Bash、JavaScript（取决于 Agent 实现）

**references/** - 额外文档
- 用途：详细技术参考、模板、结构化数据格式
- 推荐文件：REFERENCE.md、FORMS.md、特定领域文件
- 组织原则：保持文件聚焦，小文件意味着更少上下文使用

**assets/** - 静态资源
- 用途：模板、图片、数据文件
- 典型内容：文档模板、配置模板、图表、查找表、schema

### 4.4 最佳实践

- 保持 SKILL.md < 500 行
- 将详细内容拆分到 references/
- 明确标注何时引用其他文件
- 为大文件（>300 行）添加目录

---

## 5. 发现、安装与激活

### 5.1 技能发现

**扫描路径（优先级从高到低）：**
1. 项目级：`.claude/skills/`
2. 用户级：`~/.claude/skills/`
3. 市场：`~/.claude/plugins/marketplaces/`

**发现流程：**
1. Agent 扫描 skills 目录
2. 解析每个 SKILL.md 的 frontmatter
3. 提取 metadata (name + description)
4. 构建可用技能列表
5. 注入系统提示（available_skills）

### 5.2 技能安装

**方式一：手动安装**
```bash
mkdir -p ~/.claude/skills/my-skill
cp -r /path/to/skill/* ~/.claude/skills/my-skill/
```

**方式二：使用 Skills CLI**
```bash
# 从 GitHub 仓库安装
npx skills add anthropics/skills@pdf

# 从 skills.sh 市场搜索并安装
npx skills find pdf
npx skills add <package>

# 全局安装（推荐）
npx skills add <package> -g
```

**方式三：创建新 Skill**
```bash
# 初始化新 Skill
npx skills init my-skill

# 或使用 skill-creator（Agent 专用）
# Agent 调用 skill-creator 生成 SKILL.md
```

### 5.3 技能激活流程

**匹配逻辑：**
1. Agent 分析用户输入
2. 与所有 Skills 的 `description` 进行语义匹配
3. 选择最相关的 Skill（可能多个）
4. 加载完整的 SKILL.md

**显式调用：**
```
User: /pdf-processing extract text from file.pdf
```

**隐式调用：**
```
User: I need to get the text out of this PDF document
Agent: [检测到 "PDF" + "extract text"，匹配 pdf skill]
```

---

## 6. 生态系统分析

### 6.1 支持的产品

**主要支持：**
- Claude Code（CLI 工具）
- Claude.ai（Web 平台）
- OpenAI Codex、Cursor
- VS Code + Copilot、GitHub

**扩展支持：**
- Factory、Piebald、Firebender、Goose、OpenHands
- Spring AI、Qodo、Junie (JetBrains)
- Gemini CLI、Command Code、Roo Code
- Databricks、Mistral AI Vibe

### 6.2 资源生态

**官方资源：**
- 规范站点：https://agentskills.io
- 官方仓库：https://github.com/agentskills/agentskills
- 示例 Skills：https://github.com/anthropics/skills

**社区资源：**
- 技能聚合：https://skills.sh/
- 中文市场：https://skillsmp.com/zh
- Awesome Skills：https://github.com/ComposioHQ/awesome-claude-skills

### 6.3 工具生态

**开发工具：**
- skills-ref：参考 SDK 和 CLI（验证、生成系统提示）
- Claude Code：内置 skill-creator
- VS Code：Syntax highlighting for SKILL.md

**验证工具：**
- YAML frontmatter 验证
- Markdown linting
- Description 优化

---

## 7. 安全与验证

### 7.1 安全机制

**脚本执行安全：**
- 沙箱化执行环境
- 超时限制
- 资源限制（内存、磁盘、网络）
- 权限控制（allowed-tools）

**输入验证：**
- 路径遍历检查
- 文件大小限制
- 脚本权限验证

### 7.2 Skill 验证标准

**必需验证：**
- YAML frontmatter 格式正确
- name 字段符合规范
- description 字段长度合规
- 目录结构符合标准

**可选验证：**
- Markdown 格式检查
- 脚本语法检查
- 资源文件完整性

---

## 8. 典型应用场景

### 8.1 Skill 复杂度分级

**级别 1：简单 Skill**
- 单个 SKILL.md 文件
- 纯文本指令
- 无脚本或资源
- 示例：brand-guidelines

**级别 2：标准 Skill**
- SKILL.md + references/
- 分层的文档结构
- 无可执行脚本
- 示例：internal-comms

**级别 3：高级 Skill**
- SKILL.md + scripts/ + references/
- 可执行代码
- 复杂的文件结构
- 示例：pdf

**级别 4：专家 Skill**
- 完整的工作流
- 多个脚本和工具
- 详细的文档和测试
- Meta Skill（创建其他 Skills）
- 示例：skill-creator, docx

### 8.2 官方 Skills 分类

**文档类：**
- pdf、docx、pptx、xlsx
- 完整的 Python 脚本支持、详细的 REFERENCE.md、表单处理

**设计类：**
- brand-guidelines、algorithmic-art、canvas-design、frontend-design
- 简单的 SKILL.md、资源文件、模板和示例

**开发类：**
- skill-creator、mcp-builder、webapp-testing
- Meta Skills、复杂的工作流、测试和验证脚本

**通信类：**
- internal-comms、doc-coauthoring、slack-gif-creator
- 企业工作流、集成外部服务、模板驱动

**工具类：**
- theme-factory、web-artifacts-builder

---

## 9. Neco 集成参考

### 9.1 Skills 系统实现

Neco 兼容 Agent Skills 格式的技能系统。完整的 Skills 管理器接口契约参见 [progress/TECH.md § Skills 系统](../progress/TECH.md#skills系统懒加载架构借鉴zeroclaw)。

**关键集成点：**
- SkillMetadata 结构定义
- 两阶段加载策略（Full/Compact 模式）
- Skills 目录扫描和索引
- 提示注入机制

### 9.2 提示注入策略

Neco 支持 Full 和 Compact 两种提示注入模式：

- **Full 模式**：注入完整 Skill 内容到系统提示
- **Compact 模式**：仅注入元数据，Agent 按需读取

### 9.3 安全审计

Neco 实现 Skill 安全审计器，检查路径遍历、文件大小、脚本权限等。接口契约参见 [progress/TECH.md § Skills 系统](../progress/TECH.md#skills系统懒加载架构借鉴zeroclaw)。

### 9.4 返回技术文档

← [返回 Neco 技术设计文档](../TECH.md)

---

## 附录

### A. 参考资料

- 规范站点：https://agentskills.io
- 官方仓库：https://github.com/agentskills/agentskills
- 示例 Skills：https://github.com/anthropics/skills
- 技能市场：https://skills.sh/
- 中文市场：https://skillsmp.com/zh

### B. 工具推荐

```bash
# 使用 Skills CLI（推荐）
npx skills init my-skill           # 创建新 Skill
npx skills find <query>            # 搜索 Skills
npx skills add <package> -g        # 安装 Skill
npx skills list -g                 # 列出已安装 Skills

# 使用 skills-ref（开发工具）
npm install -g skills-ref
skills-ref validate ./my-skill     # 验证 Skill 格式
skills-ref to-prompt ~/.claude/skills/*  # 生成系统提示
```

### C. 核心 API 接口

**元数据解析接口：**
- 用途：解析 SKILL.md 的 YAML frontmatter
- 输入：skill_path
- 输出：metadata（name, description, path）

**系统提示构建接口：**
- 用途：构建 Skills 系统提示 XML
- 输入：skills_metadata
- 输出：XML 格式的系统提示字符串

**渐进式加载接口：**
- Level 1：加载元数据（所有 Skills）
- Level 2：加载指令（触发的 Skill）
- Level 3：加载资源（按需）

**脚本执行接口：**
- 用途：安全执行 Skill 脚本
- 安全特性：沙箱、超时、资源限制

**技能验证接口：**
- 用途：验证 Skill 符合规范
- 检查项：格式、字段、结构

---

**文档版本：** 2.1
**最后更新：** 2026-02-27
**许可：** CC-BY-4.0
