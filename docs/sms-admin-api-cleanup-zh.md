# SMS Admin API 清理说明

## 范围

本次清理针对 SMS Web Admin 中 credential 与 MCP 相关接口：

- `src/sms/web_admin.rs`
- `src/sms/admin_api.rs`
- `src/sms/web_admin/presenter.rs`

## 架构调整

在本次清理前，`web_admin.rs` 中 credential 与 MCP 相关 handler 仍然主要依赖本地 `json!` envelope 和 presenter helper。这会让响应结构隐含在 handler 里，也让这些接口与已经清理过的 `task_api`、`node_api`、`runtime_api` 风格不一致。

本次清理新增 `src/sms/admin_api.rs`，作为共享的 admin 响应层，统一承接：

- credential 列表行
- credential 变更响应
- MCP server 列表 / 详情响应
- 仅 revision 的变更响应

调整后：

- `web_admin.rs` 在 credential 与 MCP 相关接口上使用 typed admin response
- `admin_api.rs` 成为 credential/MCP 响应模型与投影 helper 的统一归属点
- `presenter.rs` 中旧的 credential/MCP helper 已被移除

## 收益

- 让 credential/MCP 管理接口更容易阅读
- 减少 `web_admin.rs` 内部局部 `json!` 拼装
- 让 admin 响应结构拥有稳定的内部归属点
- 把 SMS Web Admin 的模块化 API-mapper 模式继续向前推进

## 验证

- `cargo test admin_api --lib`
- `cargo test handlers_test --lib`
- 已对以下文件执行定向诊断：
  - `src/sms/admin_api.rs`
  - `src/sms/web_admin.rs`
  - `src/sms/web_admin/presenter.rs`
