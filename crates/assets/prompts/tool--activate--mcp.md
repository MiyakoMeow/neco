# MCP 激活工具提示词

## 功能说明

`activate::mcp` 工具用于激活 MCP（Model Context Protocol）服务器提供的外部工具和能力。

## 参数说明

- **name**：必填，要激活的 MCP 服务器名称
- **command**：可选，MCP 服务器的启动命令
- **args**：可选，启动参数列表

## MCP 简介

MCP（Model Context Protocol）是一种开放协议，允许 AI 助手连接外部工具和服务。通过 MCP，你可以：

- 访问外部 API 和服务
- 使用专门的开发工具
- 连接数据库和存储系统
- 集成第三方平台

## 使用场景

- 需要使用外部 API 获取信息时
- 需要连接数据库进行查询时
- 需要使用专门的开发工具时
- 需要集成第三方平台功能时

## 使用示例

```json
{
  "name": "filesystem",
  "command": "npx",
  "args": ["-y", "@modelcontextprotocol/server-filesystem", "/path/to/dir"]
}
```

## 注意事项

1. **配置要求**：MCP 服务器需要在配置中正确设置
2. **认证信息**：某些 MCP 服务需要提供 API 密钥或认证信息
3. **调用限制**：注意各 MCP 服务的调用频率限制
4. **可用服务**：查看已配置的 MCP 服务器列表了解可用服务
