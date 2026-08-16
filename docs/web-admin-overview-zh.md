# 管理页面概览

本文概述 `spear-next` 新增的 Web 管理页面。

## 能力

- 独立端口（默认 `127.0.0.1:8081`），Axum 路由提供接口
- 节点列表（搜索、排序、分页）
- AI Backends 控制页（backend 定义、placement 与 credentials）
- AI Models 列表（只读聚合视图；区分 Local/Remote，并支持详情页）
- 统计卡片（总数、在线、离线、最近 60s 心跳）
- SSE 流 `GET /admin/api/nodes/stream`
  - 测试友好：`?once=true` 返回单次快照事件后结束
- 主题切换（暗/亮）
- 可选鉴权：`SMS_WEB_ADMIN_TOKEN`（Bearer Token）

## 配置

- 启用：`--enable-web-admin`
- 地址：`--web-admin-addr 0.0.0.0:8081`
- 环境变量：`SMS_ENABLE_WEB_ADMIN`、`SMS_WEB_ADMIN_ADDR`

## 实现说明

- 前端通过内嵌静态资源提供（`index.html`、`main.js`、`main.css`）
- 前端源码位于 `web-admin/`，构建后覆盖输出到 `assets/admin/*`
- UI 采用 Radix primitives + Tailwind（shadcn/ui 风格），以企业控制台风为主
- SSE 通过 `CancellationToken` 支持优雅关闭

## 接口

- `GET /admin/api/nodes` → 返回 `uuid`、`name`、`ip_address`、`port`、`status`、`last_heartbeat`、`registered_at`
- `GET /admin/api/nodes/:uuid` → 返回节点与（可选）资源信息
- `GET /admin/api/stats` → 统计总数/在线/离线/最近 60s
- `GET /admin/api/nodes/stream[?once=true]` → SSE 快照事件
- `GET /admin/api/ai-model-views` → 返回 unified read model 视角的 AI Models 聚合列表（按 provider/model/hosting 汇总，并包含 placement/runtime 视图）
- `GET /admin/api/ai-backends` → 返回 canonical AI backend 列表（控制面写入口对应资源）
- `GET /admin/api/ai-backend-placements` → 返回 AI backend placement 列表
- `GET /admin/api/ai-backend-statuses` → 返回节点侧 runtime 状态列表
- `GET /admin/api/ai/credentials` → 返回 AI backends 复用的 credential 注册表

### 历史接口（已下线）

- 以下旧 AI Models / model deployments / remote backends Web Admin 入口已下线：
- `/admin/api/ai-models`
- `/admin/api/nodes/:node_uuid/ai-models*`
- `/admin/api/ai/remote-backends*`

### 任务接口

- `GET /admin/api/tasks` → 返回任务列表，包含字段：
  - `task_id`、`name`、`description`、`status`、`priority`、`desired_replicas`、`scheduling_strategy`、`endpoint`、`version`

#### 创建与执行（两条链路）

Web Admin 将“创建任务（注册 Task）”与“执行任务（调度 + 运行）”拆为两步：

- 第一步：创建/注册任务
  - `POST /admin/api/tasks`
  - 请求体声明 task spec，而不是 task 所属节点
  - 当前最小调度相关字段为：
    - `desired_replicas`
    - `scheduling_strategy`（当前支持 `spread`）
- 第二步：触发执行（可选）
  - `POST /admin/api/invocations`
  - 只会在已有 ready replica 的 node 上发起执行；如果副本仍在收敛，会返回 warming up / no ready replicas

两种模式的差异：

- task 不再有 pinned node / owner node 语义：
  - `task` 表达 workload spec
  - `instance` 才绑定具体 `node_uuid`
  - 是否执行由 `POST /admin/api/invocations` 决定；UI 的 `Run after create` 会在创建成功后调用该接口；若 assignment 对应副本尚未 ready，则返回 warming up，由控制面/节点收敛后再重试

## Secret/Key 管理建议

如果后续在 Web Admin 增加“API key 配置”相关组件，建议将其设计为“secret 引用管理”，而不是在 UI 中录入与存储明文 key。

- UI/控制面管理：backend instance 与 `credential_ref`（或 `credential_refs`）的映射
- secret 值的落地：交由部署系统注入（Kubernetes Secret / Vault Agent / systemd drop-in）
- 可观测性：仅展示“是否存在/可用”（例如由 spearlet 心跳上报 `HAS_ENV:<ENV_NAME>=true`），不展示值
  - `executable_type`、`executable_uri`、`executable_name`
  - `registered_at`、`last_heartbeat`、`metadata`、`config`
  - `result_uris`、`last_result_uri`、`last_result_status`、`last_completed_at`、`last_result_metadata`
- `GET /admin/api/tasks/{task_id}` → 返回任务详情（字段同上）
- `POST /admin/api/tasks` → 创建任务
  - 请求体包含 `name`、`description`、`priority`、`desired_replicas`、`scheduling_strategy`、`endpoint`、`version`、`capabilities`、`metadata`、`config`、可选 `executable`

## 测试

- SSE 集成测试使用 `?once=true` 避免阻塞
- 前端已包含 Playwright UI 测试（`make test-ui`）
