# AI Runtime 重构路线图

## 目的

本文档用于沉淀 `spearlet` 与 `sms` 中 AI backend 控制面/数据面主链路的重构路线。

目标是：

- 让代码更模块化，更易扩展
- 降低跨模块重复逻辑
- 缩短过长的函数和方法
- 明确配置、动态 registry、路由、上报、Web Admin 之间的边界
- 让每一步都能以较小风险落地并通过测试验证

这份路线图按阶段执行，每个阶段都应当可单独 review、单独回滚。

## 当前问题

### 1. Backend 规范化与实例化逻辑重复

backend 的规范化与运行时实例化目前分散在多个模块：

- `src/spearlet/execution/ai/router/builder.rs`
- `src/spearlet/execution/ai/router/mod.rs`
- `src/spearlet/backend_reporter.rs`
- `src/spearlet/local_models/llamacpp.rs`
- `src/spearlet/ai/remote_backend_policy.rs`

重复内容包括：

- provider 推断
- hosting 归一化
- `credential_ref` 校验
- `BackendSpec` 到 runtime adapter 的转换
- 上报快照映射

这导致新增一个 backend kind 时，需要修改多处代码并手工保持语义一致。

### 2. 动态 backend 仍存在两套心智模型

代码里原本同时存在两种思路：

- effective-config 风格的 remote backend merge
- 基于 dynamic registry 的运行时动态注入

实际运行路径已经主要依赖后者，而前者会显著增加理解成本。

### 3. 路由逻辑过于集中

`Router::route()` 当前同时承担：

- 静态/动态实例合并
- 请求约束处理
- 候选过滤
- 外部 gRPC filter 交互
- 模型绑定限制
- 选择策略
- 无候选时的错误解释

函数职责过重，不利于阅读、测试和扩展。

### 4. gRPC filter stream 代码耦合过深

`grpc_filter_stream.rs` 目前混合了：

- 单例生命周期
- worker 启动
- 协议映射
- inflight 请求关联
- 阻塞等待桥接

这些关注点应当分层。

### 5. SMS service 与 Web Admin 出现 God module

以下文件已经过大且职责混杂：

- `src/sms/service.rs`
- `src/sms/web_admin.rs`

它们需要按能力和子域拆分。

### 6. HTTP backend adapter 存在基础设施重复

`openai_chat_completion` 与 `ollama_chat` 在 HTTP 请求执行、超时、错误映射、响应封装上高度相似，适合抽取公共基座。

## 重构原则

- 每个关注点只保留一个 canonical source of truth
- 优先组合式设计，而不是跨模块特判
- 优先 typed helper，而不是重复的临时 JSON / map 转换
- orchestration 函数尽量短小，只做编排
- 每个阶段默认保持行为不变，除非该阶段明确要改变契约
- 每个阶段都必须补充或更新聚焦测试

## 阶段计划

## Phase 1. 统一 backend 描述与实例化入口

### 目标

引入共享的 backend assembly 层，让静态配置、SMS 下发 backend、local controller backend 走同一条规范化路径。

### 计划工作

- 引入共享 mapper/factory 层，例如：
  - `BackendDescriptor`
  - `BackendAssembler`
  - `AdapterFactory`
- 把共享逻辑从以下模块中抽出：
  - `router/builder.rs`
  - `router/mod.rs`
  - `backend_reporter.rs`
- 集中处理：
  - provider 推断
  - hosting 映射
  - credential 解析策略
  - `BackendSpec` 到 `BackendInstance` 的转换

### 当前进度

当前第一批共享层已经落在 `src/spearlet/ai/backend_assembly.rs`。

在当前阶段，它已经统一了：

- `AiBackendConfig -> BackendSpec`
- typed parts -> `BackendSpec`
- hosting/origin/provider 归一化
- operation 解析
- `BackendSpec -> Capabilities`
- `BackendSpec -> Adapter`
- `AiBackendConfig -> BackendInstance`
- `BackendSpec -> BackendInstance`
- `BackendSpec -> AiBackendConfig`

目前已接入的路径包括：

- router registry builder
- dynamic backend instance conversion
- backend reporter 的静态快照映射
- 本地 `llamacpp` managed backend 构造

### 验收标准

- 新增 backend kind 不再需要修改多条彼此独立的实例化路径
- 静态与动态 backend 共用一致的 adapter 构造规则
- 现有 routing 与 host API 测试保持通过

## Phase 2. 只保留一套动态 backend 生命周期模型

### 目标

消除 effective-config merge 与 dynamic-registry injection 之间的概念重叠。

### 计划工作

- 明确 dynamic registry 是 runtime 的动态事实源
- 如果旧的 effective-config merge 路径已不再需要，则收敛或删除
- 去掉误导性的旧 helper / 无效参数
- 补全最终生命周期文档

### 当前进度

旧的 runtime-facing `effective_config` 路径已经从主代码路径中移除。

已完成：

- 删除过时的 `apply_sms_remote_backends` / `EffectiveSpearletConfig` 运行时模型
- 将 `RemoteBackendMergePolicy` 提取到 `src/spearlet/ai/remote_backend_policy.rs`
- 更新 router builder 文档，明确 dynamic registry 是唯一 runtime 路径
- 删除 `RemoteBackendSyncService` 构造函数中未使用的 `EngineHolder` 依赖

### 验收标准

- runtime backend 变更只有一条主路径
- 不再存在暗示“第二套运行时模型”的历史残留 helper
- 文档与实际运行行为一致

## Phase 3. 将路由编排拆成更小步骤

### 目标

让路由逻辑更易阅读、推理与测试。

### 计划工作

- 将 `Router::route()` 拆成更小的内部 helper，例如：
  - `collect_instances`
  - `collect_candidates`
  - `apply_request_constraints`
  - `apply_external_filter`
  - `restrict_by_model_binding`
  - `select_candidate`
  - `explain_no_candidate`

### 当前进度

Phase 3 的第一步拆分已经落地。

已完成：

- 将 `Router::route()` 拆成更小的 orchestration helper
- 分离实例收集、候选收集、路由约束、外部 filter、模型绑定、最终选择、无候选解释
- 增加针对显式 backend / allowlist / denylist 路由约束的聚焦测试
- 将 router-filter decision 解释逻辑提取到独立的 `filter_decision` helper 模块
- 让 `http_gateway` 复用同一套 filter-decision helper，减少重复的候选决策处理代码
- 将 no-candidate 解释逻辑提取到 `candidate_explainer`
- 将最终选择和候选日志快照逻辑提取到 `selection`

### 验收标准

- `Router::route()` 本身变成短小的 orchestration 函数
- 候选过滤行为不变
- 错误解释仍清晰易懂

## Phase 4. 解耦 gRPC filter stream 层次

### 目标

将 router filter streaming 中的传输、生命周期、协议映射、阻塞桥接拆开。

### 计划工作

- 抽出协议映射 helper
- 抽出请求/响应关联管理
- 隔离 worker 生命周期管理
- 通过一个薄 trait 或 facade 暴露给 router 使用

### 当前进度

Phase 4 的第一步拆分已经落地。

已完成：

- 将协议映射、request 构造、requested-model 查询、trace 构造提取到 `router/filter_protocol.rs`
- 简化 `grpc_filter_stream.rs`，让 hub 更聚焦于 worker 生命周期与同步阻塞桥接
- 将 inflight correlation 的注册、完成、阻塞等待逻辑提取到 `router/filter_inflight.rs`
- 将 worker/client lifecycle 提取到 `router/filter_worker.rs`

### 验收标准

- Router 只依赖一个简洁的 decision interface
- gRPC streaming 细节收敛在专门模块中
- 单测不再强依赖后台 worker 细节

## Phase 5. 按子域拆分 SMS service 与 Web Admin

### 目标

降低 `sms/service.rs` 和 `sms/web_admin.rs` 的体量与职责耦合。

### 计划工作

- 按能力拆分 `sms/service.rs`：
  - admin AI config
  - model deployment
  - placement
  - execution index / projectors
  - registry operations
- 按 handler 分组拆分 `sms/web_admin.rs`：
  - backends
  - models
  - tasks
  - executions
  - node RPC helpers
  - typed request/response DTOs

### 当前进度

Phase 5 的第一步拆分已经落地。

已完成：

- 将 `sms/web_admin.rs` 中重复的节点查询 + lazy channel 构造逻辑提取到 `sms/web_admin/node_rpc.rs`
- 让 execution terminate、instance destroy、direct-node invocation、placement spillback 路径复用同一套 node RPC helper
- 将共用的 invocation request 构造、结果归一化、placement outcome 分类提取到 `sms/web_admin/invocation_flow.rs`
- 将 backend/model snapshot 聚合逻辑提取到 `sms/web_admin/backend_catalog.rs`
- 将 instance/execution index projector 生命周期提取到 `sms/projectors.rs`
- 将 placement penalty state 提取到 `sms/placement/state.rs`
- 将 placement 候选过滤/打分/选择 helper 提取到 `sms/placement/policy.rs`
- 将 placement outcome 解析/分类/request 构造 helper 提取到 `sms/placement/outcome.rs`
- 将 registry state 容器提取到 `sms/registry/state.rs`
- 将 MCP registry record 校验/upsert/delete/list helper 提取到 `sms/registry/mcp.rs`
- 将 model deployment registry 的 list/upsert/delete/status helper 提取到 `sms/registry/model_deployments.rs`
- 将 model deployment watch/filter stream helper 提取到 `sms/registry/model_deployments.rs`

### 当前进度补充

`ModelDeploymentRegistryServiceTrait` 现在在 `service.rs` 中主要只保留 gRPC 门面编排。
list/upsert/delete/status/watch 的数据面 helper 已集中收口到 `sms/registry/model_deployments.rs`。

### 验收标准

- 文件体量和函数长度明显下降
- 从模块名即可判断职责归属
- 子域相关测试更容易定位

## Phase 6. 提取公共 HTTP backend 执行基座

### 目标

去掉 JSON-over-HTTP backend adapter 中重复的基础设施逻辑。

### 计划工作

- 引入公共 HTTP 执行工具，统一处理：
  - 请求序列化
  - 超时
  - 状态码/错误映射
  - 响应 envelope 转换
- provider-specific 模块只保留：
  - 请求体构造
  - 成功响应解释
  - provider 错误解释

### 验收标准

- backend adapter 文件更短
- 共享传输逻辑只实现一次
- provider 模块更易 review

## 推荐执行顺序

建议顺序如下：

1. Phase 1：统一 backend assembly
2. Phase 2：只保留单一动态 backend 生命周期
3. Phase 3：拆 Router
4. Phase 4：拆 gRPC filter stream
5. Phase 5：拆 SMS service / Web Admin
6. Phase 6：抽 HTTP backend 基座

这个顺序能先澄清 runtime 边界，再逐步拆更大的服务模块，整体风险最低。

## 执行规则

- 每个阶段都应小到足以进行一次聚焦 review
- 除非刻意要改行为，否则避免把行为变化和重构混在同一个 commit
- 每个阶段完成后都同步更新中英文文档
- 实施过程中运行聚焦测试，阶段完成后跑全量测试

## 相关文件

- `src/spearlet/execution/ai/router/builder.rs`
- `src/spearlet/execution/ai/router/mod.rs`
- `src/spearlet/ai/remote_backend_policy.rs`
- `src/spearlet/ai/remote_backend_sync.rs`
- `src/spearlet/backend_reporter.rs`
- `src/spearlet/execution/ai/router/grpc_filter_stream.rs`
- `src/spearlet/execution/ai/backends/openai_chat_completion.rs`
- `src/spearlet/execution/ai/backends/ollama_chat.rs`

### 当前进度

Phase 5 已基本收尾，Phase 6 目前也已达到阶段性完成状态。

已完成：

- 新增 `src/spearlet/execution/ai/backends/http_json.rs`，作为小型共享 JSON-over-HTTP 执行 helper
- 让 `openai_chat_completion` 与 `ollama_chat` 复用同一套 blocking async/runtime + HTTP POST 执行路径
- 将共享的 JSON 响应解析、upstream-status 错误映射、canonical payload envelope 封装继续收口到同一层 HTTP helper
- 将 URL join、chat operation guard、非空字段校验这类请求前置 helper 继续收口到同一层 HTTP helper
- 将共享的 chat params 过滤与可选 tools 注入继续收口到同一层 HTTP helper
- 继续保留各自的 request body 构造、backend-specific 错误解释、成功响应语义转换，避免过早过度抽象

### 当前边界

Phase 6 目前停在一个比较健康的共享层边界：

- 共享层负责：
  - 请求前置校验
  - JSON-over-HTTP 执行
  - 响应 JSON 解析
  - upstream status 错误映射
  - canonical payload envelope 封装
- provider-specific 模块继续负责：
  - request body 的具体 shape
  - provider-specific 错误消息提取
  - provider-specific 成功响应语义解释

### 暂不建议继续抽象

除非后续再引入第三个以上的同类 HTTP backend，否则目前不建议继续：

- 把 OpenAI/Ollama 的完整 request builder 强行统一
- 引入更大的 adapter trait / template method 框架
- 为了“对称”继续拆出更细碎的 helper 文件

当前状态已经能明显减少重复，同时还能保持 provider 差异清晰可读，适合作为 Phase 6 的阶段性收尾点。
