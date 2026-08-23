# SMS Web Admin Credential 与 MCP 拆分说明

## 范围

本次清理进行了一次文件级模块化拆分，涉及：

- `src/sms/web_admin.rs`
- `src/sms/web_admin/credential_mcp_admin.rs`

## 架构调整

在上一轮 AI backend 拆分之后，`web_admin.rs` 中仍然还保留着另一块边界清晰、可独立演进的功能区：

- credential 的请求体与 handler
- MCP 的请求体与 handler
- MCP 的请求到 proto 构造 helper

本次清理把这整块功能移动到了：

- `web_admin/credential_mcp_admin.rs`

新模块现在统一承接：

- credential 请求结构
- MCP 请求结构
- credential handler
- MCP handler
- MCP 本地请求到 proto 的构造逻辑

而 `web_admin.rs` 改为通过 `pub(crate) use` 暴露 router 所需的模块项，而不再直接持有整套实现。

## 收益

- 继续降低了 `web_admin.rs` 的体积和职责面
- 让 credential/MCP 管理逻辑更容易独立定位和演进
- 让请求类型、handler 与转换 helper 按功能聚合在一起
- 进一步强化了代码从“一个超大 admin 文件”向“主入口 + feature 模块”演进的方向

## 验证

- `cargo test web_admin --lib`
- `cargo test handlers_test --lib`
- 已对以下文件执行定向诊断：
  - `src/sms/web_admin.rs`
  - `src/sms/web_admin/credential_mcp_admin.rs`
