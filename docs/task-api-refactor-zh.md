# Task API 重构文档

## 概述

本文档描述了 SPEAR-Next 项目中 Task API 的全面重构。重构将任务管理操作从复杂的生命周期模型简化为直观的注册模型。

## 变更内容

### 1. Proto 定义简化

**文件**: `proto/sms/task.proto`

**重构前**: 复杂的任务生命周期，包含 submit、stop、kill 操作
**重构后**: 简化的注册模型，包含 register、list、get、unregister 以及协同删除操作

主要变更:
- 移除了 `SubmitTaskRequest`、`StopTaskRequest`、`KillTaskRequest`
- 新增了 `RegisterTaskRequest`、`UnregisterTaskRequest`、`DeleteTaskRequest` 和 `CompleteTaskDeletionRequest`
- 简化任务状态，专注于注册状态
- 新增 `deleting` 生命周期状态用于协调删除
- 更新任务结构，包含 endpoint、version、capabilities、config 字段

### 2. 服务层重构

**文件**: `src/services/task.rs`

**变更内容**:
- 移除了 `submit_task`、`stop_task`、`kill_task` 方法
- 新增了 `register_task`、`unregister_task` 方法
- 简化任务存储模型
- 更新任务验证逻辑
- 保留 `list_tasks` 和 `get_task` 方法并更新逻辑

**核心特性**:
- 任务注册包含端点和能力信息
- 基于优先级的任务管理
- 简化状态管理（已注册/未注册）

### 3. HTTP 处理器更新

**文件**: `src/http/handlers/task.rs`

**变更内容**:
- 更新 `RegisterTaskParams` 结构
- 移除 submit/stop/kill 处理器
- 新增 unregister 处理器和协同删除处理器
- 更新响应结构
- 改进错误处理

**API 端点**:
- `POST /api/v1/tasks` - 注册新任务
- `GET /api/v1/tasks` - 列出任务（支持过滤）
- `GET /api/v1/tasks/:task_id` - 获取任务详情
- `DELETE /api/v1/tasks/:task_id` - 请求协同删除任务

### 3.1 协同删除流程

任务删除路径现在采用两阶段流程：

1. `DeleteTask` 先将任务标记为 `deleting`，并停止 endpoint 解析。
2. SMS 向目标节点发布取消事件。
3. Spearlet 停止并销毁该任务下的实例。
4. Spearlet 在运行态清理完成后调用 `CompleteTaskDeletion`。
5. SMS 完成最终删除，并清理任务主记录与 endpoint 索引。

执行历史不会随着 task 主记录一起立即删除，而是保留在独立的历史链路中。

### 4. 路由配置

**文件**: `src/http/routes.rs`

**变更内容**:
- 更新任务管理路由
- 移除冗余路由定义
- 简化路由结构

### 4.1 Web Admin 支持

- 新增 `DELETE /admin/api/tasks/{task_id}` 的 Web Admin 代理接口。
- 在任务列表页和任务详情页补充了删除入口。
- Web Admin 现在会向协同删除 API 提交 `reason` 与 `force` 参数。

### 4.2 代码组织整理

- 将 SMS 侧 task gRPC 编排拆分到 `src/sms/task_rpc.rs`。
- 将 Web Admin 的 task 处理器拆分到 `src/sms/web_admin/task_admin.rs`。
- 将 spearlet 的 task event 游标持久化拆分到 `src/spearlet/task_event_cursor.rs`。
- 将 spearlet 的 SMS 上报逻辑拆分到 `src/spearlet/execution/sms_reporter.rs`。
- 将本地 task 运行态拆除逻辑拆分到 `src/spearlet/execution/task_runtime_cleanup.rs`。
- 将共享的 SMS task 物化逻辑拆分到 `src/spearlet/execution/task_materializer.rs`。
- 将共享的执行完成收尾逻辑拆分到 `src/spearlet/execution/execution_finalize.rs`。
- 新增 `src/spearlet/execution/sms_status_adapter.rs`，将 spearlet 本地 task/instance/runtime 状态到 SMS proto 状态的投影统一收口。
- 在投影到 SMS 时保留终态语义，运行时的 `Cancelled` 与 `Timeout` 不再被压扁成 `Failed`。
- 新增 `src/spearlet/execution/execution_status.rs`，统一收口 execution public status 的解析、终态判定以及到 spearlet proto 状态的投影。
- 移除 manager 与 function service 流程代码里零散的 `"pending"` / `"running"` / `"completed"` 字符串判断，统一改走共享执行状态 helper。
- 将基于实例库存的 task 生命周期同步下沉到 `src/spearlet/execution/task.rs`，现在实例增加/移除会稳定更新本地 `TaskStatus`，不再依赖 manager 侧零散推进。
- 为 task 增加了 draining 与 idle 保留语义 helper，减少 manager 与 cleanup 链路中的状态分支判断。
- 新增 `src/spearlet/execution/task_public_status.rs`，统一收口 `TaskStatus ->` spearlet HTTP 展示状态 的映射。
- 移除 `src/spearlet/http_gateway.rs` 中残留的 task 展示状态本地映射逻辑，使 task 展示语义也遵循与 execution 状态相同的共享语义收口模式。
- 重构 `src/spearlet/execution/sms_reporter.rs`，将 SMS 请求构造与 fire-and-forget RPC 分发拆开，清理重复的 channel/timeout/spawn 模板代码。
- 将 reporter 中 task 状态/结果发布接口改成显式 fire-and-forget 语义，不再暴露没有真实等待网络工作的误导性 `async` 包装。
- 清理 `src/spearlet/execution/manager.rs` 与 `src/spearlet/http_gateway.rs` 中不再提供额外领域语义的纯转发一行 wrapper，减少无意义的中间跳转。
- 继续收缩 manager 内部只转发到 `sms_reporter` 或 `scheduler` 的一行 wrapper，同时保留仍然承载 task/status 领域语义的单行 helper，避免为了“删函数”而损失可读性。
- 在继续扫描更多 Rust 文件后，又清理了一批跨模块的无语义 wrapper，包括 manager 内部重复 getter、伪异步的 execution 状态查询，以及 SMS 侧薄薄一层的状态字符串/endpoint 转换 helper。
- 继续在 `src/sms/service.rs` 与 `src/spearlet/http_gateway.rs` 中删除无语义 wrapper，包括未使用的 service getter、MCP upsert 直通 helper，以及在内联后依然同样清晰的一行时间格式化函数。
- 继续清理 `src/sms/grpc_server.rs` 中的无语义小函数，将 `prepare()` 这种“保留日志后仅返回两个值”的薄封装直接内联到两个启动路径里，减少额外跳转。
- 将 spearlet HTTP 层的 execution 展示状态转换重新收口回 `src/spearlet/execution/execution_status.rs`，让 `http_gateway.rs` 不再单独维护一张 proto 到展示字符串的 `match` 表。
- 按 handler 边界拆分 `src/spearlet/http_gateway.rs`，将 execution 与 task 相关 HTTP handler 下沉到 `src/spearlet/http_gateway/execution_handlers.rs` 和 `src/spearlet/http_gateway/task_handlers.rs`，让根网关文件更聚焦于共享状态、路由装配、websocket、object 与 monitoring 逻辑。
- 继续拆分 `http_gateway`，将 object 与 monitoring 相关 HTTP handler 下沉到 `src/spearlet/http_gateway/object_handlers.rs` 和 `src/spearlet/http_gateway/monitoring_handlers.rs`，让根文件进一步收敛为路由、共享状态、websocket 与文档装配入口。
- 将之前偏 policy-style 的 task visibility 参数化方案，收敛成 `src/sms/task_semantics.rs` 中更语义化的 task 访问 helper：控制面读取继续使用 `get_task(...)`，而 endpoint 路由改为走 `resolve_routable_task_by_endpoint(...)`。
- 简化了 spearlet 侧 task 创建/镜像的两条路径：SMS watch event 预热与 invocation 缺失回补现在都统一走 manager 持有的 SMS 同步入口，不再让 `task_events.rs` 自己单独拉取 task 快照。
- 收紧了 `TaskEventSubscriber` 的处理语义：cursor 现在只会在 task event 真正处理成功后推进，让 create/cancel replay 更接近清晰的 at-least-once 语义，而不再是 fire-and-forget 式处理。
- 新增了 manager 维度的按 instance 维护的活跃 execution 注册表，让 destroy-instance 流程可以基于单一权威映射排空并终止该实例上的所有 execution；同时 scheduler 也补上了对已达并发上限实例的跳过逻辑。
- 对 manager 内部的 execution 终止语义也做了收口：execution 一旦真正开始运行就会 upsert 成带有 instance 绑定的 running 响应，而 terminate/destroy 流程会先同步更新 manager 侧 execution 视图，再发出 runtime termination signal。
- 统一了 manager、host API、runtime 三层的 termination 命名约定：manager 使用 `request_*` 表示控制面终止请求，registry helper 使用 `request/read/clear_*_request` 表示终止请求的登记/读取/清理，而 WASM runtime hook 使用 `read_*` / `abort_*`，显式区分“发出信号”和“消费信号”。
- 继续清理了 `TaskExecutionManager` 内部 helper 的动词语义：`sync_*` 用于 SMS/local 状态投影，`stop_and_unregister_*` 用于 runtime 停止并从 manager/scheduler/task 注销实例，`persist_*_response` 用于写入可查询的 execution response 快照。
- 同时提升了 manager 对外 public API 的语义直观性，并保留兼容 wrapper：新的内部调用点优先使用 `request_execution_termination(...)` 与 `drain_and_destroy_instance(...)`，而旧的 `terminate_execution(...)` / `destroy_instance(...)` 则退化为兼容层薄 wrapper。
- 进一步给 SMS 控制面补上了显式的 instance 删除语义：`InstanceRegistryService` 新增 `DeleteInstance` RPC，`TaskExecutionManager` 会在本地 unregister 完成后调用它，同时 SMS instance index 会保存删除 tombstone，避免旧的异步 upsert 把已删除 instance 重新“复活”。
- 继续对 execution / SMS 相关模块做了函数命名熵减：状态映射函数统一为 `source_to_target`，SMS reporter 统一使用 `report / update / delete / acknowledge`，SMS projector/state-store 统一使用 `upsert_*_record / tombstone_*_record / project_*`，本地 SMS 物化统一使用 `materialize_*`，从而让同类函数名处于同一抽象层并表现出相似行为。
- 将 task / instance 的控制面模型进一步收口到“task 是 workload spec、instance 才绑定 node”的方向：`task.node_uuid` 与 `RegisterTaskRequest.node_uuid` 已从 task 协议面移除，task 新增 `desired_replicas` 与 `scheduling_strategy`（首个实现为 `SPREAD`），task 事件改为面向所有 spearlet 的全局 task stream，而 task 删除的最终完成条件改为 SMS 观察到该 task 的 active instances 已清空，不再依赖单节点 ownership。
- 将 `TaskExecutionManager` 的长流程整理为 resolve、begin、park、finalize 等命名阶段 helper。
- 将 `handle_async_completion()` 整理为 release、record、finalize、store 四个命名阶段。
- 将 `TaskExecutionManager` 的测试从 `manager.rs` 迁移到 `src/spearlet/execution/manager_tests.rs`。
- 将实例获取/创建流程整理为选择、容量检查、配置准备、创建启动、注册副作用等命名阶段。
- 删除 legacy 的 `TaskEventBus` 与 `SubscribeTaskEvents` RPC，spearlet 的 task 同步改为消费 `EventsService.SubscribeEvents` 的统一事件流。
- 将 task/instance/execution 的公共字符串状态与通用状态判定统一收口到 `src/sms/types.rs`。
- 将 `TaskExecutionManager` 的 task/instance 状态上报改为经由统一适配层，而不是在 manager 流程代码里散落选择 proto 枚举。
- 在保持外部 task API 不变的前提下，降低了 `service.rs`、`web_admin.rs` 和 `task_events.rs` 的职责混杂程度。

### 5. 集成测试更新

**文件**: `tests/task_integration_tests.rs`

**变更内容**:
- 更新测试数据生成
- 修改测试场景以使用新 API
- 修复优先级值映射
- 更新错误处理测试
- 所有测试现在都能成功通过

## API 使用示例

### 注册任务

```bash
curl -X POST http://localhost:8080/api/v1/tasks \
  -H "Content-Type: application/json" \
  -d '{
    "description": "测试任务",
    "priority": "normal",
    "endpoint": "http://worker:8080/execute",
    "version": "1.0.0",
    "capabilities": ["compute", "storage"],
    "config": {
      "timeout": 300,
      "retries": 3
    },
    "executable": {
      "type": "wasm",
      "uri": "smsfile://<file_id>",
      "name": "hello.wasm",
      "args": [],
      "env": {}
    }
  }'
```

### 列出任务

```bash
curl -X GET "http://localhost:8080/api/v1/tasks?status=registered&priority=normal"
```

### 获取任务详情

```bash
curl -X GET http://localhost:8080/api/v1/tasks/{task_id}
```

### 删除任务

```bash
curl -X DELETE http://localhost:8080/api/v1/tasks/{task_id} \
  -H "Content-Type: application/json" \
  -d '{
    "reason": "operator cleanup",
    "force": true
  }'
```

## 优先级级别

系统支持以下优先级级别:
- `low` - 低优先级任务
- `normal` - 普通优先级任务（默认）
- `high` - 高优先级任务
- `urgent` - 紧急优先级任务

## 重构的好处

1. **简化 API**: 从生命周期管理模型简化为注册模型，降低复杂性
2. **更好的性能**: 移除不必要的状态转换
3. **更清晰的语义**: 基于注册的模型更加直观
4. **更容易测试**: 简化测试场景，提高测试覆盖率
5. **可维护性**: 更清洁的代码结构，降低复杂性

## 可执行描述（Executable）

- `type`: 可执行类型，支持 `binary|script|container|wasm|process`
- `uri`: 规范化 URI（如 `smsfile://<id>`、`http://...`、`docker://image:tag`）
- `name`: 可选本地别名
- `checksum_sha256`: 可选完整性校验
- `args` 与 `env`: 运行时默认参数与环境变量

注意：对于 `type=wasm`，Spearlet 运行时在实例化阶段需要合法的 WASM 二进制；若下载或传入的模块字节不是合法 WASM，将在实例创建时报错。

## 迁移说明

对于使用旧 API 的现有客户端:
- 将 `submit_task` 调用替换为 `register_task`
- 将 `stop_task` 和 `kill_task` 调用替换为 `unregister_task`
- 更新任务数据结构以包含新字段（endpoint、version、capabilities、config）
- 更新优先级值以使用小写字符串（normal、high 等）

## 测试

所有集成测试已更新并通过:
- `test_task_lifecycle` - 测试完整的任务注册和注销
- `test_task_list_with_filters` - 测试带各种过滤器的任务列表
- `test_task_error_handling` - 测试错误场景
- `test_task_sequential_operations` - 测试多个任务操作
- `test_task_content_types` - 测试不同内容类型

运行测试:
```bash
cargo test --test task_integration_tests
```
