# SMS Runtime API 清理说明

## 范围

本次清理针对 SMS Web Admin 中 instance/execution 的展示逻辑：

- `src/sms/web_admin.rs`
- `src/sms/runtime_api.rs`
- `src/sms/web_admin/presenter.rs`

## 架构调整

在本次清理前，`web_admin.rs` 中的 instance/execution 管理接口同时承担了：

- RPC 编排
- 分页流程
- 错误处理
- 临时 JSON envelope 拼装
- 行级 / 详情级展示字段投影

这让文件阅读成本偏高，也让 runtime 返回结构散落在 handler 内部，不够显式。

本次清理新增 `src/sms/runtime_api.rs`，作为共享的 runtime 映射层，统一承接：

- task instance 列表行
- instance 详情结构
- instance execution 摘要行
- execution history 列表行
- execution 详情结构
- execution / instance 动作响应结构

调整后：

- `web_admin.rs` 更专注于控制流与 RPC 调用编排
- `runtime_api.rs` 成为 runtime admin 响应模型的统一归属点
- `presenter.rs` 中旧的 runtime JSON presenter 已被移除

## 收益

- 让 runtime admin 相关接口更短、更容易阅读
- 为 instance/execution 响应建立了 typed 的内部归属点
- 减少了局部 `json!` 拼装导致的隐藏响应结构漂移
- 让 runtime 相关接口与 `task_api`、`node_api` 保持一致的模块化模式

## 验证

- `cargo test runtime_api --lib`
- `cargo test handlers_test --lib`
- 已对以下文件执行定向诊断：
  - `src/sms/runtime_api.rs`
  - `src/sms/web_admin.rs`
  - `src/sms/web_admin/presenter.rs`
