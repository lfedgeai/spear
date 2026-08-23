# SMS Node 与 Resource API 清理说明

## 范围

本次清理针对 SMS 模块中重复的 node/resource 展示逻辑：

- `src/sms/handlers/node.rs`
- `src/sms/handlers/resource.rs`
- `src/sms/web_admin.rs`
- `src/sms/node_api.rs`

## 架构调整

在这次清理前，node 和 resource 的响应结构在多个位置被手工拼装：

- public HTTP handler 使用临时 `json!` 构造返回
- admin 的 node list 自己维护一套列表行投影
- admin 的 node detail 又走另一套本地 presenter 路径

这会让控制器层同时承担展示层知识，也更容易让 public/admin 两条链路的字段逐渐漂移。

本次清理新增 `src/sms/node_api.rs`，作为共享映射层，统一承接：

- public node 响应
- public node resource 响应
- public node-with-resource 响应
- admin node list item
- admin node detail envelope

调整后：

- `handlers/node.rs` 成为更薄的传输层适配器
- `handlers/resource.rs` 成为更薄的传输层适配器
- `web_admin.rs` 复用共享的 typed node view model，而不是重复手工拼 JSON 行
- node/resource 的响应结构有了单一的内部归属点

## 收益

- 减少了 public 与 admin API 之间重复的字段投影逻辑
- 让控制器文件更短、更容易扫读
- 更清晰地界定了 node/resource 展示规则的归属位置
- 为后续继续清理其他 API 入口建立了更干净的模块化模式

## 验证

- `cargo test node_api --lib`
- `cargo test handlers_test --lib`
- 已对以下文件执行定向诊断：
  - `src/sms/handlers/node.rs`
  - `src/sms/handlers/resource.rs`
  - `src/sms/node_api.rs`
  - `src/sms/web_admin.rs`
