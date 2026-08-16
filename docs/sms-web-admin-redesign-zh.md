# SMS Web Admin 重设计文档

本文档分两部分：

- 现状盘点：当前 SMS Web Admin 已具备的能力、接口与局限
- 重设计方案：按照可维护、可扩展、可观测、可演进的 best practices 重新设计前端（并给出需要的后端 BFF/API 契约）

目标读者：SMS/Spearlet 开发者、平台运维、调度/资源侧开发者。

## 1. 背景与目标

### 1.1 背景

当前 SMS Web Admin 采用“静态前端 + 后端聚合接口(BFF)”的形式：

- 后端：Axum 路由，提供 `/admin` 页面与 `/admin/api/*` JSON 接口
- 前端：工程化前端（React + Radix primitives + Tailwind + React Query），以 `hash route` 实现多页面，构建产物以静态资源形式内嵌到 `assets/admin/*`

当前实现可用，但存在以下问题：

- 前端代码为单文件，功能耦合度高，扩展成本高
- UI 信息密度不合理（详情主要以 JSON pre 展示），缺少可视化与可操作性
- 权限模型单一（仅可选 Bearer token），缺少细粒度能力与审计
- 缺少统一的错误处理、空态、加载态、统一的可复用组件体系
- 数据刷新策略混杂（轮询 + SSE），缺少“按域”可配置的刷新策略

### 1.2 目标

- 模块化：按领域拆分（Nodes/Tasks/Files/Executions/Settings…），组件与 API 层分离
- 可扩展：新增页面或模块无需改动现有页面的核心逻辑
- 可观测：前端可清晰定位“节点离线原因/心跳是否异常/上报资源是否异常”
- 更美观：统一布局、主题、密度、详情视图、表格交互；默认支持暗色模式
- 最佳实践：工程化构建（类型、lint、测试）、路由、状态管理、错误边界、权限

非目标（本阶段不做）：

- 全量 RBAC / 多租户
- 复杂图表与指标平台替代（Prometheus/Grafana 仍是主力）

## 2. 现状盘点

### 2.1 页面与功能（当前）

前端（源码位于 `web-admin/`）当前包含 5 个主页面：

- Nodes
  - StatsBar：Total/Online/Offline/Recent(60s) 概览
  - Nodes Table：搜索、排序、分页
  - 节点详情：点击 UUID，弹窗展示 `/admin/api/nodes/{uuid}` 的 JSON
  - 数据刷新：ReactQuery 轮询 +（无 token 时）通过 SSE 触发刷新

- Tasks
  - Tasks Table：搜索、排序、分页
  - Task Detail：弹窗 JSON
  - Create Task：表单创建 Task，支持 executable 配置、从 Files 中选择 smsfile URI

- Files
  - 文件列表、上传（presign + upload）、下载、复制 smsfile URI、删除
  - 支持一次选择多个文件并顺序上传（多请求；后端无需批量接口）
  - 文件 meta 弹窗 JSON

- Settings
  - Dark Mode 开关
  - Admin Token 保存/应用

### 2.2 后端接口（当前 /admin/api）

后端路由在 [src/sms/web_admin.rs](../src/sms/web_admin.rs)。

#### 2.2.1 认证

- 可选环境变量 `SMS_WEB_ADMIN_TOKEN`
  - 若设置：要求请求头 `Authorization: Bearer <token>`
  - 若未设置：无鉴权

#### 2.2.2 Nodes

- `GET /admin/api/nodes`
  - query：`status`, `q`, `sort` 或 `sort_by+order`, `limit`, `offset`
  - 返回：`{ nodes: NodeSummary[], total_count }`

- `GET /admin/api/nodes/{uuid}`
  - 返回：`{ found, node, resource }`
  - resource 目前只返回部分字段：cpu/mem/disk + memory bytes

- `GET /admin/api/nodes/stream`
  - SSE
  - 事件：`snapshot`
  - query：`once=true` 时只返回一次 snapshot

- `GET /admin/api/stats`
  - 返回：`{ total_count, online_count, offline_count, recent_60s_count }`

#### 2.2.3 Tasks

- `GET /admin/api/tasks`
  - query：`q`, `sort`, `sort_by+order`, `limit`, `offset`
  - 返回：`{ tasks: TaskSummary[], total_count }`

- `POST /admin/api/tasks`
  - 创建 task（映射到 gRPC RegisterTask）
  - 返回：`{ success, task_id, message }`
  - 重要语义：
    - task 不再绑定单一 `node_uuid`
    - 请求体表达 workload spec，至少包括 `desired_replicas` 与 `scheduling_strategy`
    - 执行仍由 `POST /admin/api/invocations` 触发，并只落到已有 ready replica 的节点；若 assignment 尚未收敛，则返回 warming up

- `GET /admin/api/tasks/{task_id}`
  - 返回 task 详情（结构化 JSON）

#### 2.2.4 Executions

- `POST /admin/api/invocations`
  - BFF 行为：
    - 若请求体携带 `node_uuid=<uuid>`：直接对该节点执行（不走 placement）
    - 否则：只在当前已有 ready replica 的节点上发起 invoke
    - 若当前没有 ready replica：best-effort 触发 assignment reconcile，然后返回 warming up / no ready replicas
  - 返回：`{ success, ... }`（当前偏“执行结果 JSON”，没有统一 schema）

#### 2.2.5 Files

- `GET /admin/api/files`
  - query：`q`, `limit`, `offset`
  - 返回：`{ files: FileItem[], total_count }`
- `POST /admin/api/files/presign-upload`
- `POST /admin/api/files`（上传）
- `GET /admin/api/files/{id}`（下载）
- `DELETE /admin/api/files/{id}`
- `GET /admin/api/files/{id}/meta`

说明：当前后端上传接口为“单文件/单请求”。多文件上传可以通过前端循环多次调用实现；只有在需要 multipart 批量上传/批量 presign 时才需要扩展后端。

### 2.3 当前痛点（从可用性与扩展性角度）

#### 2.3.1 信息呈现不足

- Nodes 详情没有结构化展示 resource/心跳健康度
- 缺少“离线原因”的可定位信息（例如：最近一次心跳时间、超时阈值、是否资源上报失败）

#### 2.3.2 工程结构不可扩展

- 前端为单文件，缺少：模块边界、组件复用、API 层封装、类型约束、统一错误处理

#### 2.3.3 数据刷新策略不统一

- 表格靠 ReactQuery；stats 又靠轮询 + SSE 触发
- 没有“按模块/按路由”的刷新策略配置

## 3. 重设计方案（前端）

### 3.0 UI 技术栈选择（非 AntD，追求新的感官体验）

由于目标之一是“从感官上有新的体验”，重设计阶段不再沿用 Ant Design。本文档在此处做出技术栈决策：采用 Radix + Tailwind（shadcn/ui）路线，并以此为后续设计与实现的默认前提。

#### 3.0.1 技术栈决策（最终）

- 设计系统/组件：shadcn/ui（基于 Radix UI primitives；组件代码落在仓库内，可二次定制）
- 样式：Tailwind CSS + CSS variables（主题 token），支持暗色模式
- 图标：lucide-react
- 路由：react-router
- 数据请求与缓存：@tanstack/react-query
- 表格：@tanstack/react-table（封装 DataTable 组件，统一筛选/分页/排序/空态/骨架屏）
- 表单与校验：react-hook-form + zod
- 反馈：sonner（toast）
- 图表（Dashboard 可选）：echarts（优先）或 recharts

#### 3.0.2 选择理由

- “新体验”：默认视觉语言与 AntD 明显不同，更接近现代 SaaS 控制台
- “可扩展”：Radix primitives + Tailwind 便于做统一的 Widget 容器与 Dashboard 组合
- “模块化”：更适合在 feature 边界内封装组件，而不是依赖全局大而全组件库

#### 3.0.3 代价与约束

- 需要引入构建流程（Vite/TS），不再适合纯 CDN 单文件方式
- 需要自建一套基础组件封装（DataTable、FormField、PageLayout、Empty/Error、WidgetContainer 等）

#### 3.0.4 组件层级与封装边界

- `src/components/ui/*`：shadcn/ui 生成与轻度定制的基础组件（Button、Card、Dialog、Drawer、Tabs、DropdownMenu…）
- `src/components/*`：与业务无关的“可复用组合组件”（DataTable、JsonViewer、PageHeader、EmptyState、ErrorState、WidgetContainer…）
- `src/features/*`：领域组件（Nodes/Tasks/Files/Executions/Dashboard）

#### 3.0.4.1 代码拆分原则（便于长期扩展）

- 以 feature 为边界：每个模块在 `src/features/<feature>` 内自洽（页面、子组件、hooks、局部类型）
- 以“依赖方向”约束耦合：
  - `features/*` 只能依赖 `components/*`、`components/ui/*`、`api/*`、`lib/*`
  - `components/*` 不依赖 `features/*`
  - `api/*` 不依赖 UI
- 以路由为聚合点：`src/app/*` 只负责 layout、路由、全局 provider，不承载业务细节

建议目录形态（重点关注 app / features）：

```
src/
  app/
    App.tsx
    AppShell.tsx
    routes.tsx
    providers/
  api/
    client.ts
    nodes.ts
    tasks.ts
    files.ts
    stats.ts
    types.ts
  components/
    ui/
    DataTable/
    EmptyState/
    ErrorState/
    WidgetContainer/
  features/
    dashboard/
      DashboardPage.tsx
      widgets/
    nodes/
      NodesPage.tsx
      NodeDetailDialog.tsx
      components/
    tasks/
    files/
    settings/
      SettingsPage.tsx
  lib/
    utils.ts
```

#### 3.0.5 主题与视觉规范（建议）

- Token：基于 CSS variables（`--background/--foreground/--muted/--border/--primary/...`）
- 风格取向：以“企业控制台风”为主（克制、稳定、可读性优先），少渐变/少装饰
- 字体与排版：默认 `text-sm`（信息密集），表格可提供 compact（`text-xs` + 更小行高）；页面标题 `text-lg` 即可
- 圆角与阴影：圆角偏小（例如 6–8px），阴影轻量或几乎不用；用边框与分隔线表达层级
- 状态色：Online/Offline/Degraded 使用 Badge/Chip + 图标，不只靠颜色区分
- 交互：所有危险操作（删除、不可逆变更）使用 Dialog 二次确认
- 布局密度：左侧导航 + 顶部工具栏固定；主体区域用灰底承接卡片/表格，提升信息对比

### 3.1 总体架构

保持后端提供 Admin BFF API 的模式，但前端工程化升级：

- 前端：TypeScript + React + 路由 + 组件库 + 数据层（Query）
- API：统一 `apiClient`（baseURL、auth header、错误归一化、重试策略）
- 状态管理：
  - Server state：TanStack Query
  - UI state：本地 state + URL search params
  - 全局：Theme/Auth/Timezone 等轻量 context

目录结构建议：

```
admin/
  src/
    components/
      ui/
    app/
      App.tsx
      routes.tsx
      layout/
      providers/
    features/
      nodes/
      tasks/
      files/
      executions/
      dashboard/
      settings/
    components/
      DataTable/
      JsonViewer/
      PageHeader/
      HealthBadge/
      EmptyState/
      ErrorState/
      WidgetContainer/
    api/
      client.ts
      types.ts
      nodes.ts
      tasks.ts
      files.ts
      executions.ts
    theme/
    utils/
  index.html
  package.json
```

### 3.2 信息架构（IA）与页面设计

#### 3.2.1 导航结构

- Dashboard（新增，作为默认首页）
  - Cluster 概览：Nodes（online/offline/最近心跳）、Tasks（active/inactive）、Files（count/total bytes）
  - Health 摘要：离线节点 Top N、心跳超时节点、资源上报过期节点
  - 活动摘要：最近注册/离线切换、最近 task 变更、最近上传文件
  - 快捷入口：Create Task、Run Execution、Upload Files

- Nodes
  - 列表页：表格 + 过滤器 + 刷新/自动刷新开关
  - 详情页：结构化分区（Summary / Resources / Metadata / Recent events）

- Tasks
  - 列表页
  - 详情页（可视化 Task spec、executable、最近结果）
  - 创建/编辑（分步表单）

- Executions
  - 执行发起（从 Task 详情进入更自然）
  - 执行历史（需要后端补接口，或先只展示“本次执行结果”）

- Files
  - 列表/上传/下载/删除
  - Meta 结构化展示

- Settings
  - Theme / TZ / Token
  - “连接状态/版本信息”

#### 3.2.0 Dashboard（可插拔扩展设计）

Dashboard 的定位：提供“可行动”的全局总览，而不是替代监控系统。由于具体内容未完全定稿，Dashboard 需要以“可插拔 widget”的方式设计，允许后续快速扩展/替换模块，而无需改动整体布局与基础能力。

- 核心原则：
  - Widget 化：Dashboard 由多个独立 widget 组成（卡片、列表、图表、事件流等都属于 widget）
  - 低耦合：widget 只能通过统一 API client 获取数据，不直接依赖其他 widget 的内部状态
  - 可配置：布局、显隐、刷新间隔、权限门禁全部由配置驱动
  - 可降级：任意 widget 失败不影响页面其它 widget；支持 skeleton/empty/error 三态

- 页面骨架（固定，但内容可变）：
  - 顶部：全局时间范围/时区、刷新按钮、自动刷新开关
  - 主体：响应式栅格布局（按断点决定列数），每个栅格单元渲染一个 widget

- Widget 注册与扩展点：
  - 前端维护 `WidgetRegistry`：`id -> ReactComponent + defaultConfig + dataDependencies`
  - Dashboard 仅负责：读取配置 → 选择 widget → 渲染容器（不关心具体内容）
  - 每个 widget 自带：
    - `title/description`
    - `requiredPermissions`
    - `queryKeys` 与 `refetchPolicy`

- 配置模型（建议）：
  - `DashboardLayout`：
    - `widgets[]`: `{ id, enabled, size, order, props, refreshIntervalSeconds }`
    - `breakpoints`: `{ xs, md, lg }`（每行列数/间距）
  - 配置来源：
    - P0：前端内置默认配置 + localStorage 覆盖
    - P1：后端提供 `GET /admin/api/dashboard/layout` 返回布局（便于不同环境定制）

- 数据来源建议：
  - P0：复用现有 `GET /admin/api/stats`、`GET /admin/api/nodes?limit=...`、`GET /admin/api/tasks?limit=...`、`GET /admin/api/files`
  - P1：增加 `GET /admin/api/dashboard`（聚合数据，减少瀑布请求；允许按 widget 选择性返回字段）

- 可观测性与可维护性：
  - widget 内部统一上报：load 耗时、错误类型（network/auth/server）、最后更新时间
  - 统一 ErrorBoundary：隔离渲染错误


#### 3.2.2 Nodes 列表页（设计）

- 顶部：
  - 搜索框（uuid/ip/name/meta）
  - 状态过滤（online/offline/all）
  - 刷新策略：手动刷新 + 自动刷新（默认 15s）
  - 密度切换（comfortable/compact）

- 表格列建议：
  - Name（优先展示 metadata.name，缺失则“-”）
  - UUID（可复制）
  - Status（Online/Offline/Degraded）
  - Last heartbeat（相对时间 + hover 绝对时间）
  - Resource（CPU/Mem/Disk 小型条形/Badge）
  - Address（ip:port）
  - Actions（View / Copy / …）

#### 3.2.3 Node 详情页（设计）

采用右侧 Drawer 或独立路由（建议独立路由 `#/nodes/:uuid`，便于深链）。

- Summary
  - status、ip:port、registered_at、last_heartbeat
  - Health：基于 `heartbeat_timeout` 计算 “是否超时/超时多久”

- Resources
  - CPU/Mem/Disk：进度条 + 数值
  - Memory bytes：used/total
  - Disk bytes：used/total
  - updated_at（资源更新时间）

- Metadata
  - KV 表格

- Actions
  - Copy node UUID
  - 快捷创建 Task（预填 node_uuid）

#### 3.2.4 Tasks

- 列表：状态/优先级/目标节点/最近一次结果
- 详情：
  - Spec（endpoint/version/capabilities）
  - Executable（type/uri/name/args/env）
  - Last result（last_result_uri/status/completed_at）
  - 操作：执行一次（sync/async/stream）、下线 task

#### 3.2.5 Files

- 列表：Name/Size/Modified/ID
- 详情：Meta + Copy URI
- 上传：
  - 支持拖拽与多文件选择
  - 上传队列（默认顺序上传；可配置并发=2/3）
  - 单个文件失败不影响队列继续；结束后给出成功/失败汇总
  - 上传后自动刷新

### 3.3 UI/UX best practices

- 路由与深链：所有详情页都可被 URL 直接打开
- 可访问性：表单控件、键盘导航、颜色对比度
- 一致的空态/错误态：统一组件 `EmptyState`/`ErrorState`
- 一致的时间显示：统一 `TZ` 设置 + 相对时间
- 表格体验：列可配置、列宽、复制按钮、批量操作预留
- 主题系统：light/dark + 统一 token

## 4. 重设计方案（后端 API/BFF 契约建议）

当前 `/admin/api` 已覆盖核心功能，但为了前端更可维护，建议统一 schema 与补齐必要字段。

### 4.1 统一响应结构

建议所有接口遵循：

```
type ApiResponse<T> =
  | { ok: true; data: T }
  | { ok: false; error: { code: string; message: string; details?: any } }
```

这样前端只需一个错误处理层。

### 4.2 Nodes API 建议

- `GET /admin/api/nodes`
  - 建议返回：
    - `heartbeat_timeout_seconds`（用于前端计算 health）
    - `now_ts`（避免前端和服务端时钟偏差造成误判）

- `GET /admin/api/nodes/{uuid}`
  - 建议 resource 返回完整字段（包括 `updated_at`、disk bytes、load averages）
  - 建议返回 `computed` 字段：
    - `is_heartbeat_stale`
    - `heartbeat_age_seconds`

### 4.3 Executions API 建议

当前 `/admin/api/invocations` 是“发起一次 invoke 并返回结果/状态”的 BFF。

建议补齐：

- `POST /admin/api/invocations`
  - request：
    - task_id
    - mode(sync/async/stream)
    - labels/metadata
  - response：
    - request_id / execution_id / node_uuid
    - warming_up / no_ready_replicas（当 assignment 尚未收敛）
    - final_result（成功时）

后续可扩展：

- `GET /admin/api/executions?task_id=&limit=&offset=`（需要服务端有存储）

### 4.4 Auth 与审计建议

- 维持现有 Bearer token，但建议：
  - 支持多 token（read-only / admin）
  - 返回 `x-sms-admin-user` 或 `whoami` 接口用于前端展示
  - 对危险操作（delete file / create task / execute）记录审计日志

## 5. 工程化与交付方式

### 5.1 构建与静态资源交付

推荐方式：

- 使用 Vite 构建产物（JS/CSS 分包、hash 文件名、gzip/brotli）
- SMS 后端继续以 `include_bytes!` 或静态目录方式提供

与当前 CDN 单文件实现不同，Radix + Tailwind（shadcn/ui）路线要求将前端作为独立工程构建后再交付静态资源。

两种交付模式：

1) Embed 模式（保持当前风格）
   - `assets/admin/dist/*` 通过 `include_bytes!` 编译进二进制
   - 优点：部署简单
   - 缺点：编译时间变长、二进制体积变大

2) Static Dir 模式（推荐）
   - SMS 读取 `--web-admin-static-dir` 或 config 指定目录
   - 优点：前后端解耦
   - 缺点：需要部署时额外文件

### 5.2 测试策略

- 前端：
  - 单元测试（utils、query param 解析、formatters、api client）
  - 组件测试（关键表格/筛选/详情弹窗/危险操作确认）
  - e2e（可选）：本地启动 SMS + Playwright 走真实 `/admin/api`（或 MSW mock）

前端测试需要的改造（建议在 M1 同步做）：

- 可测试的 API 层：
  - `api/client.ts` 提供可注入的 `baseUrl`/`fetch`（测试中用 mock fetch 或 MSW）
  - `api/*` 仅做请求与类型映射，不直接依赖 UI
- 可测试的 UI 结构：
  - feature 内部组件拆分（例如 `NodeDetailDialog`），避免页面组件过大难测
  - 把“格式化/筛选/排序”等纯逻辑抽到 `lib/*`，用单元测试覆盖
- Query 层可控：
  - 测试中使用独立 QueryClient（关闭重试/轮询），避免 flaky
  - 对 polling/SSE 逻辑做开关（production 开，test 关）

推荐工具链（与 Radix + Tailwind / shadcn 路线匹配）：

- 单测/组件测：Vitest + Testing Library + jsdom
- Mock：MSW（建议）或 fetch mock（轻量）
- e2e：Playwright（可选，优先跑 smoke）

- 后端：
  - 现有 integration tests 扩展：nodes/tasks/files/executions 的 schema

### 5.3 兼容与迁移

## 8. 关键交互细节（补充）

### 8.1 统一搜索与过滤模型

所有列表页统一使用 URL search params 表达筛选条件，确保可分享与可回放：

- `q`：自由文本
- `status`：枚举
- `sort_by` + `order`
- `limit` + `offset`（或 `page` + `page_size`）

前端实现建议：

- `useSearchParams` 管理条件
- 条件变更时仅更新 URL，不直接触发 fetch；由 Query key 驱动自动刷新

### 8.2 刷新策略

建议统一为“可配置自动刷新 + 可见性优化”：

- 列表页默认 15s 自动刷新（Nodes/Tasks），Files 默认手动
- 页面不可见时暂停轮询（利用 `refetchOnWindowFocus` + visibility API）
- SSE 仅作为“变更提示”，由前端节流后触发 invalidate，而不是实时推全量数据

### 8.3 时间显示

- 全站只保留一种时间基准：后端返回 `now_ts` 与数据的 `*_at`（unix seconds）
- 前端仅负责：
  - 相对时间（例如 12s ago）
  - hover 展示绝对时间（按用户 TZ）

### 8.4 详情页信息组织

- Drawer（列表内快看）+ 独立路由（深链详情）同时支持
- Drawer 内容与详情页复用同一 `NodeDetailPanel` 组件

### 8.5 错误处理与可观测

- api client 归一化错误：
  - 网络错误（断网/超时）
  - 401/403（token 无效）
  - 429/5xx（退避重试）
- 列表页错误：展示 ErrorState（带重试按钮）
- 操作类错误：toast + 详情（可展开）

## 9. 需要后端补齐的字段/接口清单（建议优先级）

P0（立即收益）：

- Nodes 列表/详情：增加 `resource.updated_at` 和 `node.metadata.name` 的显式字段映射
- Nodes：返回 `now_ts` 与 `heartbeat_timeout_seconds`
- Executions：统一返回 schema（attempts + final）

P1（增强体验）：

- Nodes：增加 `resource.load_average_*`、disk bytes、network bytes
- Tasks：增加“最近一次执行状态”的结构化字段（当前已有 last_result_*，但前端需要更明确含义）

P2（可观测）：

- SSE：增加事件类型（node_updated/task_updated/file_updated）并带最小 payload
- Executions：增加历史查询接口

- 第一阶段：保持 `/admin/api/*` 兼容，先重写前端
- 第二阶段：逐步引入统一 schema/补字段，同时保持旧字段兼容（增加而非破坏）
- 第三阶段：引入 executions history 等新能力

## 6. 里程碑（建议）

- M1：前端工程初始化（Vite/TS/Tailwind/shadcn）+ Layout/Theme/Router + Dashboard 骨架 + Nodes/Tasks/Files 基础页面
- M2：结构化详情页（Node/Task/File）+ 统一错误处理 + 可配置刷新
- M3：Executions 页面与 spillback 可视化 + 审计日志 + read-only token

## 7. 附录：代码入口

- 后端 Router：`create_admin_router`（[web_admin.rs](../src/sms/web_admin.rs#L128-L209)）
- Nodes API：`list_nodes`/`get_node_detail`（[web_admin.rs](../src/sms/web_admin.rs#L398-L473)）
- Stats API：`get_stats`（[web_admin.rs](../src/sms/web_admin.rs#L839-L870)）
- 前端源码入口：`web-admin/src/main.tsx`（[main.tsx](../web-admin/src/main.tsx)）
- 前端构建产物：`assets/admin/index.html`、`assets/admin/main.js`、`assets/admin/main.css`
