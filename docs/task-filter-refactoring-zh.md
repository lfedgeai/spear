# Task 列表过滤重构说明

## 概述

本文档说明 SMS 当前的任务列表过滤行为，包括 HTTP 查询参数、protobuf 请求编码方式以及服务端过滤实现。

核心目标是在保持 protobuf 兼容的前提下支持可选过滤器。实现使用一个整型哨兵值（`-1`）表示“无过滤”。

## API 入口

### HTTP

- 接口：`GET /api/v1/tasks`
- 可选查询参数：
  - `status`：`unknown|registered|created|active|inactive|unregistered`
  - `priority`：`unknown|low|normal|high|urgent`
  - `limit`（默认 100）
  - `offset`（默认 0）

示例：

```text
GET /api/v1/tasks?status=active&priority=high&limit=50&offset=0
```

实现位置：`src/sms/handlers/task.rs:252`。

### gRPC（SMS）

- RPC：`ListTasks(ListTasksRequest) returns (ListTasksResponse)`
- Proto：`proto/sms/task.proto:106`

`status_filter` 与 `priority_filter` 在 proto 中是枚举，但在实现中按整数处理；当值为负数时视为“无过滤”。

## 编码：`FilterState` 与 `NO_FILTER`

`src/sms/types.rs:7` 定义：

- `NO_FILTER: i32 = -1`
- `FilterState::{None, Value(i32)}` 与 `FilterState::to_i32()`

HTTP 侧将可选参数转换成 `FilterState`，再转换为 `i32` 以适配 protobuf：

- 参数缺失或非法：`FilterState::None -> -1`
- 参数存在且合法：`FilterState::Value(enum as i32)`

实现位置：`src/sms/handlers/task.rs:259`。

## 服务端过滤逻辑

### SMS gRPC Service

`src/sms/service.rs:633` 将请求字段转换为可选过滤器：

- `status_filter < 0` => `None`
- `priority_filter < 0` => `None`
- `limit <= 0` => `None`
- `offset < 0` => `None`

随后调用 `TaskService::list_tasks_with_filters(...)`。

### TaskService

`src/sms/services/task_service.rs:57` 应用过滤条件：

- 若指定 `status_filter` 且 `>= 0`，要求 `task.status` 匹配。
- 若指定 `priority_filter` 且 `>= 0`，要求 `task.priority` 匹配。

分页：

- `offset` 默认 0
- `limit` 默认 100
- 当 `offset` 超过过滤后长度时，返回空列表

## 注意事项

- `ListTasksResponse.total_count` 返回的是过滤前的总任务数（用于分页 UI）。见 `src/sms/service.rs:667`。
- Web Admin 也使用 `-1` 表示“无过滤”。见 `src/sms/web_admin.rs:269`。

版本：v1.0（基于 2025-12-16 的代码状态验证）。
