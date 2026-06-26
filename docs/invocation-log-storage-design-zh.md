# Invocation 日志持久化与查看设计

## 背景

目前 Web Admin 触发执行（`POST /admin/api/executions`）会返回 `invocation_id` / `execution_id`，但系统缺少“面向用户”的 invocation 日志持久化与查看能力（例如 stdout/stderr、运行时/系统事件）。这会显著增加排障、审计和复现成本。

本文给出一套业界通用（best practice）的设计：在 SMS Web Admin BFF + Web Admin UI 中提供“可持久化、可分页、可追尾、可下载”的 invocation 日志。

相关文档：

- 执行模型：[invocation-execution-model-refactor-zh.md](./invocation-execution-model-refactor-zh.md)
- Web Admin 总览：[web-admin-overview-zh.md](./web-admin-overview-zh.md)

## 目标

- 为每次 invocation 保存可查看的日志（支持 spillback/retry 产生的多次 execution）。
- Web Admin 可快速定位并查看：列表 → 详情 → 跟随（follow）→ 下载。
- 支持按 stream/level 过滤，并支持稳定 cursor 的分页/定位。
- 安全与成本的合理默认值：保留周期、大小限制、背压、脱敏。
- 明确区分：Spearlet 服务日志（控制面） vs 用户 invocation 日志（数据面）。

## 非目标

- 不替代企业级日志平台（Loki/Elastic/Splunk），但保留对接路径。
- 不在 MVP 阶段提供“跨年级别”的全量全文检索（可后续分期）。
- 不在本文覆盖交互式 console（另文档单独设计）。

## 术语

- **Invocation**：一次用户/客户端函数调用请求；`invocation_id` 稳定。
- **Execution**：invocation 的一次具体尝试（落在某个节点/实例）；`execution_id`。
- **Invocation 日志**：面向用户的、附着在 execution 上的日志流，包含：
  - 进程 stdout/stderr
  - 运行时/系统事件（可选）
  - 运行时 API 发出的结构化日志（可选）

## 需求

### 功能性需求

- 以 **execution** 为基本持久化单位；以 **invocation** 聚合多个 execution。
- 支持历史查看（分页）与实时追尾（tail）。
- 支持整份日志下载。
- 每条日志必须包含用于关联与排障的关键字段：
  - `invocation_id`、`execution_id`、`task_id`、`function_name`、`node_uuid`、`instance_id`。

### 非功能性需求

- **保留策略**：默认 TTL 可配置（例如 7–30 天）。
- **成本/体积控制**：单 execution 最大字节数、单行截断、分片 + 压缩。
- **背压**：不能因日志过量导致 Spearlet OOM；需要可控的丢弃策略。
- **安全**：鉴权/授权、多租户隔离（未来）、敏感信息脱敏、下载安全。
- **可靠性**：允许 SMS 短暂不可用；持久化尽力而为（best effort）并在 UI 中可见。

## 总体架构

### 组件

1. **Spearlet Log Collector（按 execution）**
   - 采集 stdout/stderr 与可选系统事件。
   - 有界缓冲（bounded queue），避免内存失控。
   - 按 chunk 刷写到中心存储。

2. **SMS Log Storage（中心存储）**
   - **Blob 存储**保存日志分片（append-only）：生产建议对象存储；开发/默认可复用现有 `smsfile://` 文件存储。
   - **元数据存储**保存索引与 cursor：复用项目已有 KV（RocksDB/Sled）。

3. **SMS Web Admin BFF**
   - 提供 execution/invocation 列表与日志读取接口。
   - 提供轮询友好的分页读取接口（短期不做 SSE）。

4. **Web Admin UI**
   - 增加 Execution 列表与日志查看器。
   - 从任务执行入口提供“查看日志”的深链接。

### 数据流

1. 用户在 Web Admin 触发执行 → SMS BFF 选点并 spillback 调用 Spearlet。
2. Spearlet 开始执行并把日志事件写入 collector。
3. collector 将 chunk + 元数据写入 SMS Log Storage。
4. UI 通过 SMS BFF 分页拉取历史；follow 模式通过短轮询增量拉取（后续可演进为 SSE/WebSocket）。

## 异步执行与日志生命周期（关键）

### 问题背景

异步执行（`execution_mode=async`）的语义是：**提交成功后立即返回 `running`**，真实执行在后台继续进行。此时如果按同步路径在“提交后”就 flush/finalize 日志，会导致：

- flush 时日志尚未产生，读到空；
- finalize 过早关闭写入窗口，后续日志无法落盘。

### 业界 best practice（推荐）

- **日志流以 `execution_id` 为主键**（append-only），并由服务端维护日志状态机：
  - `open`（可写）→ `finalizing`（仅做最后 flush）→ `finalized`（拒绝写入，幂等）
- **只有当 execution 进入终态**（completed/failed/timeout/cancelled）后，才允许写入 “execution_completed/failed” 系统日志并 finalize。
- **异步执行必须有 completion signal**：后台执行完成后向编排层（TaskExecutionManager）发送完成事件，触发最后 flush + finalize。
- **follow**（近实时查看）优先用“增量轮询 + cursor”实现；SSE/WebSocket 可作为后续优化，不影响存储与语义。

## 存储设计

### 日志行（逻辑）结构

建议使用结构化行存储，落盘为 NDJSON：

```json
{
  "ts_ms": 1730000000000,
  "seq": 42,
  "invocation_id": "...",
  "execution_id": "...",
  "task_id": "...",
  "function_name": "__default__",
  "node_uuid": "...",
  "instance_id": "...",
  "stream": "stdout",
  "level": "info",
  "message": "hello",
  "attrs": {"k": "v"}
}
```

说明：

- `seq` 为 execution 内单调递增序号，用于稳定分页。
- `stream` 建议固定枚举：`stdout|stderr|system`。
- `attrs` 可选，用于携带运行时特定字段。

### 分片（chunk）格式

- Chunk 文件路径：`invocations/{invocation_id}/executions/{execution_id}/chunks/{chunk_seq}.ndjson.zst`
- Chunk 大小：压缩后 1–4 MiB（可配置）。
- 压缩：优先 Zstd（或 gzip 兜底）。

### 元数据（KV）

逻辑 key 设计：

- `exec:{execution_id}:meta` → { task_id, invocation_id, node_uuid, instance_id, started_at, completed_at, status, byte_count, first_seq, last_seq }
- `exec:{execution_id}:chunks` → [{ chunk_seq, first_seq, last_seq, uri, byte_count, created_at }]
- `inv:{invocation_id}:executions` → execution_id 列表（attempts）

这样 UI 列表/定位不需要扫描 blob。

### 保留与清理

- SMS 周期性清理：
  - 当 `completed_at + ttl < now` 删除 chunk 与 KV 元数据。
  - 通过 `max_bytes` 限制单 execution 总日志；超过后截断并标记 `truncated=true`。

## API 设计（SMS Web Admin BFF）

UI 仅调用 SMS（避免浏览器直接连 Spearlet）。

### 1）执行创建返回值

建议更新 `POST /admin/api/executions` 返回值，补齐 `invocation_id` 与日志链接：

```json
{
  "success": true,
  "invocation_id": "...",
  "execution_id": "...",
  "node_uuid": "...",
  "message": "ok",
  "log_url": "/admin/executions/{execution_id}"
}
```

### 2）执行列表

`GET /admin/api/executions?task_id=&invocation_id=&limit=&cursor=`

返回包含 `execution_id`、`invocation_id`、`status`、时间戳、`node_uuid` 的 summary 列表。

### 3）执行详情

`GET /admin/api/executions/{execution_id}`

返回 execution 元数据与 `log_state`：

- `ready`：可读历史日志
- `pending`：执行中但尚未刷写
- `unavailable`：存储失败或暂不可用

### 4）读取历史日志（分页）

`GET /admin/api/executions/{execution_id}/logs?limit=500&cursor=...&stream=stdout,stderr&level=info,warn,error`

响应：

```json
{
  "execution_id": "...",
  "lines": [ {"ts_ms":..., "seq":..., "stream":"stdout", "message":"..."} ],
  "next_cursor": "...",
  "truncated": false,
  "completed": false
}
```

cursor 建议：

- 使用不透明 cursor，内部编码 `(seq, chunk_seq, offset)`。
- 支持 `direction=backward|forward` 以满足“向上翻历史”的体验。

### 5）下载

`GET /admin/api/executions/{execution_id}/logs/download`

- Content-Type：`text/plain` 或 `application/x-ndjson`
- 可选 `?format=text|ndjson`

## Spearlet 侧设计

### 采集来源

- **Process runtime**：接管子进程 stdout/stderr 管道并写入 execution log。
- **WASM runtime**：增加 hostcall “log” API，将 WASM 内日志映射到 invocation log。
- **系统事件**：可选记录生命周期里程碑（scheduled/started/completed、资源指标）。

### 背压策略

- 按 execution 设置有界 ring buffer（按 bytes/行数限制）。
- flush 触发条件：
  - 达到大小阈值
  - 达到时间阈值（例如每 1s）
  - 执行结束

### 异步执行（no_wait）策略

- `execution_mode=async|console|stream` 时，Spearlet 返回 `execution_status=running`，但不应在编排层立即 finalize 日志。
- 需要在运行时（WASM worker / Process 子进程 / K8s job）结束后，向编排层上报 completion signal：
  - 由编排层统一做：flush（含 wasm ring）→ 写 `execution_completed/failed` → finalize。
- 为避免 instance 复用导致日志串扰，WASM hostcall 日志应按 `execution_id` 归属（或至少携带 `execution_id` 并在 flush 时过滤）。

丢弃策略（可配置）：

- `drop_oldest`（默认）：保留最新上下文。
- `drop_newest`：保留最早上下文。

### 失败处理

- 当 SMS 日志存储不可达：
  - 保留少量本地缓冲；
  - 将 `log_state=unavailable` 写入元数据；
  - UI 展示“日志暂不可用，可稍后重试”。

## Web Admin 前端改造建议

目标工程：`web-admin/`（React + TanStack Query）。当前 UI 通过 [executions.ts](../web-admin/src/api/executions.ts) 调用 `POST /admin/api/executions`。

### 新增页面与导航

- 增加 “Executions” 页面：
  - 路由：`/executions`
  - 列表：支持过滤（task_id、status、时间范围、node）
  - 点击进入详情：`/executions/:execution_id`

### Execution 详情页

- 顶部信息：status、task_id、node_uuid、invocation_id、时间戳。
- 日志查看器：
  - Follow（自动滚动）开关
  - stdout/stderr/system 筛选
  - level 筛选（如支持）
  - 视窗内搜索（客户端）
  - 下载按钮
  - 选中复制

### API client 增补

- 新增 `src/api/logs.ts`：
  - `getExecution(execution_id)`
  - `getExecutionLogs(execution_id, cursor, limit, filters)`
  - follow 模式通过短轮询实现（短期不接 EventSource）

### 交互细节（best practice）

- 大日志使用虚拟列表渲染。
- 达到 `max_bytes` 时展示 “truncated” 提示条。
- stdout/stderr 视觉区分（颜色或标签）。
- Follow/filters 通过 query 参数持久化，便于分享链接与复现。

## 安全考虑

## 改造计划（函数级）

按“先修语义，再演进能力”的顺序分期，确保架构清晰且可扩展：

### Phase 1：修正异步路径的日志生命周期（不再过早 finalize）

- [TaskExecutionManager::execute_existing_task_invocation](../src/spearlet/execution/manager.rs#L658-L882)
  - 当 `runtime_response.execution_status == Running`（异步提交成功）：
    - 不调用 `append_wasm_logs_to_sms`
    - 不写 `execution_completed`
    - 不调用 `finalize_execution_logs_to_sms`
    - 可选：写一条 `system` 日志 `execution_dispatched mode=async`

### Phase 2：为异步执行补齐 completion signal（完成后再 flush/finalize）

- [WasmWorkerRequest](../src/spearlet/execution/runtime/wasm.rs)
  - 扩展 `Invoke` payload：携带 `execution_id`，并增加一个完成回传通道（tokio mpsc/oneshot）。
- [WasmRuntime::execute](../src/spearlet/execution/runtime/wasm.rs#L790-L927)
  - no_wait 分支发送 `Invoke(execution_id, ...)` 给 worker，并注册 completion handler。
- [TaskExecutionManager](../src/spearlet/execution/manager.rs)
  - 新增一个后台 listener（或复用现有 work loop）消费 completion events：
    - `append_wasm_logs_to_sms(execution_id, ...)`
    - `append_execution_logs_to_sms(... execution_completed/failed ...)`
    - `finalize_execution_logs_to_sms(execution_id)`
    - 更新 execution index 状态与时间戳（Completed/Failed）

### Phase 3：WASM hostcall 日志按 execution_id 归属（避免串扰，支持并发/重试）

- [DefaultHostApi::wasm_log_write](../src/spearlet/execution/host_api/core.rs#L151-L213)
  - 引入“当前 execution_id”的上下文（由 worker 在 invoke 开始/结束设置/清理），写入 log entry 时带上 `execution_id`。
- [get_wasm_logs / clear_wasm_logs](../src/spearlet/execution/host_api/core.rs#L99-L126)
  - 增加 `get_wasm_logs_by_execution(execution_id, cursor, limit)`，供 flush 使用。
- [append_wasm_logs_to_sms](../src/spearlet/execution/manager.rs#L952-L1000)
  - 从“按 instance_id 全量读取”改为“按 execution_id 增量读取 + cursor”。

### Phase 4：follow 模式体验增强（可选）

- 编排层对 running execution 周期性 flush（例如 500ms–1s），UI 通过 `/logs?cursor=` 短轮询近实时刷新。

- 不在 UI 中暴露敏感信息：
  - 在 SMS 侧对常见模式进行脱敏（API key、Bearer token 等）。
  - 默认不存储环境变量。
- 鉴权：
  - 复用 `SMS_WEB_ADMIN_TOKEN` 的 Bearer token 机制。
  - 未来：支持租户 RBAC 与审计日志。
- 下载安全：
  - 使用 Content-Disposition 附件下载；限制最大体积；仅用 ID 路由避免路径穿越。

## 可观测性

- 每行带关联字段，保证端到端定位。
- 指标：
  - 写入字节数、丢弃行数、flush 延迟、存储失败率。
- flush 操作加入 tracing span，便于定位性能瓶颈。

## 分期落地

1. **Phase 0（MVP）**
   - 持久化 stdout/stderr（按 execution）。
   - 实现历史读取 + 下载。
   - UI：Execution 详情页 + 日志查看器。

2. **Phase 1**
   - SSE 追尾。
   - Execution 列表页。

3. **Phase 2**
   - 丰富的结构化 level 与运行时事件。
   - 可选全文检索（配置开关）。

## 备选方案

- **对接外部日志平台**：通过 OTEL/Fluent Bit 输出日志，并在 Grafana/Kibana 查询。
  - 优点：可扩展检索、长周期保留。
  - 缺点：依赖外部基础设施，不够自包含。

该方案强调先做“产品内可用的最小闭环”，同时保留与企业日志平台的对接路径。
