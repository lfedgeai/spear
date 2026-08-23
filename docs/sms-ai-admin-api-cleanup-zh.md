# SMS AI Admin API 清理说明

## 范围

本次清理针对 SMS Web Admin 中 AI backend 相关管理接口：

- `src/sms/web_admin.rs`
- `src/sms/ai_admin_api.rs`

## 架构调整

在本次清理前，`web_admin.rs` 中 AI backend 这一块仍然同时承担：

- RPC 编排
- 响应结构定义
- 本地 JSON 行级投影
- 嵌套读模型渲染

这使得 AI backend 区域成为 Web Admin 中仍然最像“旧风格”的一大块逻辑。

本次清理新增 `src/sms/ai_admin_api.rs`，作为共享的 typed 响应层，统一承接：

- backend 列表 / 详情 / 变更响应
- placement 列表 / 变更响应
- assignment 列表响应
- node status 列表响应
- model view 列表响应

调整后：

- `web_admin.rs` 主要保留请求校验和控制流
- `ai_admin_api.rs` 统一承接 AI backend 管理端响应模型与映射规则
- `web_admin.rs` 内本地的 AI backend JSON 投影 helper 已被移除

## 收益

- 让 AI backend 管理接口更短、更容易扫读
- 让多层嵌套的 AI backend 响应结构拥有稳定的内部归属点
- 再次减少了 `web_admin.rs` 中大块的临时 `json!` 拼装
- 把 SMS admin 侧的模块化 typed response 模式继续向更大范围推进

## 验证

- `cargo test ai_admin_api --lib`
- `cargo test handlers_test --lib`
- 已对以下文件执行定向诊断：
  - `src/sms/ai_admin_api.rs`
  - `src/sms/web_admin.rs`
