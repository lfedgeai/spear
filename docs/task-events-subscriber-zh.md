# 任务事件订阅器

## 概述

SPEARlet 内置任务事件订阅器，通过连接 SMS 的 `EventsService` 订阅全局 task 统一事件流，并从中筛选 `TaskEvent` payload。订阅器将游标持久化以保证重启后能继续处理（近似一次性处理），并支持可配置的自动重连与退避。

## 组件

- `TaskEventSubscriber`：维护配置与最后处理的事件序号游标。
- 节点 UUID 推导：当 `node_name` 是合法 UUID 时直接使用；否则基于 `grpc.addr`、`grpc.port` 与 `node_name` 通过 UUIDv5 派生稳定的 UUID。
- 游标持久化：将 unified event stream 的 `after_seq` 存储在 `storage.data_dir` 下，文件名为 `task_events_cursor_{node_uuid}.json`。

## 核心行为

- 通过 `sms_grpc_addr` 使用 gRPC 连接 SMS，携带 `ResourceType::Task` selector 与 `after_seq` 调用 `EventsService.SubscribeEvents`。
- 事件流处理：每收到事件时——
  - 更新内存中的 `after_seq` 并写入持久化文件。
  - 仅处理 `ResourceType::Task` 且 payload 为 `sms.TaskEvent` 的统一事件。
  - 对 `Create` 事件，调用 `GetTask` 获取任务详情并在本地物化 task。
  - 对 `Update` 事件，重新从 SMS 获取 task snapshot 并刷新本地物化状态。
  - 对 `Cancel` 事件，触发本地 task runtime 回收与删除确认。
- 不再按 task 级 `node_uuid` 过滤事件；所有 SPEARlet 都能收到 task 控制面事件，并仅清理自己本地持有的 task/instance。
- 在连接或流错误时自动重连，等待时间由 `sms_connect_retry_ms` 控制。

## 配置

相关 `SpearletConfig` 字段：

```toml
[spearlet]
node_name = "spearlet-node"
sms_grpc_addr = "127.0.0.1:50051"
auto_register = true
heartbeat_interval = 30
cleanup_interval = 300
sms_connect_timeout_ms = 15000
sms_connect_retry_ms = 500
reconnect_total_timeout_ms = 300000

[spearlet.grpc]
addr = "0.0.0.0:50052"

[spearlet.http]
cors_enabled = true
swagger_enabled = true

[spearlet.storage]
backend = "memory"
data_dir = "./data/spearlet"
```

支持的环境变量：`SPEARLET_SMS_ADDR`、`SPEARLET_SMS_CONNECT_TIMEOUT_MS`、`SPEARLET_SMS_CONNECT_RETRY_MS`、`SPEARLET_RECONNECT_TOTAL_TIMEOUT_MS`、`SPEARLET_STORAGE_DATA_DIR`。

## 使用方式

在 SPEARlet 初始化阶段启动订阅器：

```rust
use std::sync::Arc;
use spear_next::spearlet::{config::SpearletConfig, task_events::TaskEventSubscriber};

let config = Arc::new(SpearletConfig::default());
let subscriber = TaskEventSubscriber::new(config.clone());
subscriber.start().await; // 后台运行
```

订阅器将游标持久化到 `storage.data_dir` 中，SPEARlet 重启后可以从最后处理的事件续订。

## 错误处理与韧性

- 连接失败时按 `sms_connect_retry_ms` 延迟重试。
- 优雅处理流错误，延迟后重新订阅。
- 不依赖 task 级 `node_uuid` 路由；本地只对自己已 materialize 的 task/instance 执行回收。
- 游标文件目录若不存在会自动创建。

## 测试

- 游标读写回路测试：`src/spearlet/task_events_test.rs` 验证 `store_cursor`/`load_cursor` 行为。
- 建议的集成测试：模拟 SMS 事件流与重连场景。

## 代码引用

- `src/spearlet/task_events.rs:44` — 订阅器启动与重连循环
- `src/spearlet/task_events.rs:76` — 事件处理与 `Create` 事件任务详情获取
- `src/spearlet/config.rs:259` — 默认配置值

---

本文档与任务事件订阅器的最新实现保持一致，便于后续扩展与维护。
