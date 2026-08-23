# SMS Task API 映射层清理说明

## 范围

本次清理针对 SMS 模块中面向 task 的 HTTP 与管理端入口：

- `src/sms/handlers/task.rs`
- `src/sms/web_admin/task_admin.rs`
- `src/sms/task_api.rs`

目标是把传输层处理与 task 请求构造、task 展示映射分离开，让整体架构更容易阅读。

## 架构调整

在本次清理前，两套 task 入口都各自承担了下面这些职责：

- 将对外的 executable type 字符串转换为 proto 枚举
- 将 scheduling strategy 字符串转换为 proto 枚举
- 构造 `RegisterTaskRequest`
- 将 proto `Task` 记录映射为对外或管理端响应

这些逻辑分别散落在两个文件里，导致重复较多，也让入口层同时承担了协议翻译细节。

本次清理新增了共享映射模块 `src/sms/task_api.rs`，统一承接：

- task 注册请求构造
- executable type 归一化
- scheduling strategy 归一化
- 公共 task 响应投影
- 管理端 task 摘要 / 详情 / 删除结果投影

调整后：

- `handlers/task.rs` 主要负责 HTTP 参数解析、错误处理和 gRPC 调用
- `web_admin/task_admin.rs` 主要负责管理端查询流程以及分页/过滤编排
- `task_api.rs` 成为 task API 翻译规则的单一归属点

## 收益

- 减少了 HTTP 与 admin 两条链路中的重复 task 映射逻辑
- 让面向 task 的控制器文件更短、更容易扫读
- 更清晰地界定了 SMS 架构中协议翻译逻辑的归属位置
- 为后续清理其他重复 API 适配层提供了可复用模式

## 验证

- `cargo test handlers_test task_api --lib`
- 已对以下文件执行定向诊断：
  - `src/sms/handlers/task.rs`
  - `src/sms/web_admin/task_admin.rs`
  - `src/sms/task_api.rs`
