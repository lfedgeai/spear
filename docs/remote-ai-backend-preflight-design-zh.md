# Remote AI Backend 预检设计

## 概览

本文定义 SPEAR 应如何在创建 `remote` AI backend 之前完成访问性与权限预检，重点覆盖 credential 验证、model 可访问性检查，以及 `all_nodes` placement 场景的处理策略。

这个设计基于当前已经存在的 local `llamacpp` 节点侧 preflight 模式演进，而不是再新增一套割裂的临时校验流程。

当前相关实现：

- Web Admin preflight 入口：[`../src/sms/web_admin/ai_backend_admin.rs`](../src/sms/web_admin/ai_backend_admin.rs)
- 当前仅支持 local 的 preflight 路由：`POST /admin/api/ai-backends/preflight`
- 当前节点侧 local preflight 路由：`POST /internal/ai/local-models/preflight`

## 问题定义

目前 remote backend 往往可以先创建成功，即使下面这些问题其实已经存在：

- 目标节点无法访问 provider endpoint
- 实际运行节点上的 TLS、DNS 或 egress 配置有问题
- 选中的 credential 无效
- credential 没有访问目标 model 或 deployment 的权限
- provider 配置在语法上合法，但实际上不可用

这会把失败延后到 reconcile 阶段或第一次真实调用时才暴露，导致用户反馈更慢，定位也更困难。

## 目标

- 对明显错误的 remote backend 定义尽早失败
- 必须从真实执行节点发起验证，而不是从浏览器或 SMS 发起
- 返回结构化、用户可理解的失败原因
- 统一 local 与 remote backend 的 preflight 框架
- 即使 placement 覆盖很多节点，也要保持创建流程足够快
- 保持职责清晰：
  - Web Admin 负责交互
  - SMS 负责编排
  - SPEARlet 负责节点侧真实性

## 非目标

- 第一版不做所有 provider feature 的完整能力认证
- 第一版不做所有 backend 的后台周期性健康检查
- 第一版不保存长期历史 preflight 运行记录
- 第一版不做 credential 自动修复或自动切换
- 第一版不把大规模 `all_nodes` 的全量同步验证作为默认行为

## 设计原则

### 节点侧真实性

验证必须从最终执行请求的节点发起，因为只有该节点才具备真实的网络、代理、TLS、DNS 与 credential 解析环境。

### 轻量探测

preflight 应该通过最小但可靠的请求证明“可用性”。除非没有更便宜的 provider 探测方式，否则不应直接执行高成本推理。

### 结构化结果

系统应该返回稳定的错误码和 phase，而不只是自由文本描述。

### Provider 感知

OpenAI-compatible 和 remote Ollama 这类标准 provider 可以做更强的语义校验；generic HTTP backend 则应使用更弱或可配置的校验。

### 大规模 Placement 两阶段验证

对于 `all_nodes` placement，不应默认在创建前同步验证每一个节点。系统应该采用“创建前抽样严格验证 + 创建后异步全量验证”的策略。

## 高层流程

### 单节点 Placement

1. 用户在 Web Admin 提交 remote backend draft
2. Web Admin 调用 `POST /admin/api/ai-backends/preflight`
3. SMS 校验请求结构并解析目标节点
4. SMS 把节点侧 preflight 请求转发给目标 SPEARlet
5. SPEARlet 解析 credential，探测 provider，并返回结构化结果
6. 只有 preflight 成功时，Web Admin 才继续创建 backend

### All Nodes Placement

1. 用户提交 remote backend draft
2. Web Admin 调用同一个 preflight API
3. SMS 选择一组具有代表性的样本节点
4. SMS 只对样本节点执行同步严格验证
5. 如果样本验证失败，则阻止创建
6. 如果样本验证成功，则 Web Admin 继续创建
7. backend 创建完成后，SMS 再调度异步全量节点验证
8. 节点级验证结果回写到 backend node status 和聚合 summary

## 为什么不在创建前验证所有节点

在创建前对所有节点做同步验证虽然在正确性上最严格，但不适合作为默认行为，因为它会：

- 随着集群规模线性增加创建延迟
- 在 provider 或网络瞬时抖动时让创建流程非常脆弱
- 更容易触发第三方 provider 的 rate limit
- 让大集群的首次上手体验变差
- 把控制面的创建动作和“某一时刻全集群完全健康”强绑定

因此，`all_nodes` 的默认策略应当是：

- 创建前抽样严格验证
- 创建后异步全量验证

## Placement 验证策略

框架层建议支持显式策略，即使第一版 UI 只暴露其中一部分。

### `single_node_strict`

- 用于单节点 placement
- 目标节点必须在创建前通过
- 任意失败都阻止创建

### `sampled_strict`

- 作为 `all_nodes` 的默认策略
- 创建前只有代表性样本节点必须通过
- 只有样本全部通过才允许创建
- 创建后仍然会异步验证所有节点

### `strict_all_nodes`

- 可选高级模式
- 所有目标节点必须在创建前通过
- 任意失败都阻止创建
- 只建议用于小规模集群或明确要求强一致的环境

### `best_effort`

- 可选非阻塞模式
- preflight 返回 warning，但不阻止创建
- 适合探索环境，不建议作为默认模式

## 代表性抽样策略

对于 `sampled_strict`，SMS 应该选择一组小而有代表性的节点，而不是单纯随机抽样。

推荐策略：

1. 如果已有拓扑标签，则每个拓扑域至少选一个节点
2. 优先包含默认路由或调度优先节点
3. 如果没有拓扑信息，则选择 `min(3, total_nodes)` 个节点

未来可以使用的拓扑键包括：

- region
- availability zone
- rack 或 subnet group
- node pool 或 hardware class

如果当前还没有这些拓扑元数据，第一版仍应先实现“有上限、可复现”的抽样策略。

## 第一版 Provider 范围

建议第一版优先支持：

- `openai`
- `openai_compatible`
- `ollama`

后续再扩展：

- generic `http_json` 的强语义验证
- realtime 或 audio 能力验证
- tool-calling 能力验证
- 超出通用路径的 provider 特殊部署模型

## 按 Provider 的验证语义

### OpenAI-Compatible

检查项：

- endpoint 可达
- TLS 与 HTTP 是否成功
- credential 是否有效
- 目标 model 是否可见或可访问

推荐探测顺序：

1. model metadata 或 model listing 接口
2. 如果支持，直接查询目标 model
3. 如果元数据探测不足，再退化为一个最小、低成本请求

### Remote Ollama

检查项：

- base URL 可达
- 基础 API 健康
- 通过 tags 或 model metadata 验证目标 model 是否存在

### Generic HTTP JSON

第一版只做弱验证：

- endpoint 可达
- 配置的 headers 已经被注入
- 返回码不是明显的认证失败

后续如果要加强，应通过显式的 healthcheck 配置来做。

## API 设计

### Web Admin / SMS 公共 API

保留现有路由：

- `POST /admin/api/ai-backends/preflight`

当前请求结构：

```json
{
  "backend": { "...": "..." },
  "node_uuids": ["node-a", "node-b"]
}
```

建议演进后的兼容结构：

```json
{
  "backend": { "...": "..." },
  "node_uuids": ["node-a", "node-b"],
  "verification_policy": "sampled_strict",
  "requested_checks": ["connectivity", "auth", "model_access"]
}
```

### SMS 到 SPEARlet 的内部 API

建议新增统一的节点侧路由：

- `POST /internal/ai/backends/preflight`

建议请求体：

```json
{
  "backend": {
    "provider": "openai_compatible",
    "hosting": "remote",
    "model": "gpt-4o-mini",
    "base_url": "https://api.openai.com/v1",
    "credential_ref": "cred-openai-prod",
    "spec": {
      "operations": ["chat"],
      "features": ["streaming"],
      "transports": ["https"]
    },
    "metadata": {
      "api_style": "openai"
    }
  },
  "requested_checks": ["connectivity", "auth", "model_access"]
}
```

建议响应体：

```json
{
  "success": true,
  "provider": "openai_compatible",
  "phase": "model_access",
  "error_code": null,
  "message": "authentication and model access verified",
  "resolved_endpoint": "https://api.openai.com/v1",
  "http_status": 200,
  "provider_code": null,
  "latency_ms": 183,
  "checks": [
    { "name": "connectivity", "ok": true },
    { "name": "auth", "ok": true },
    { "name": "model_access", "ok": true }
  ],
  "auth_valid": true,
  "model_accessible": true
}
```

## 结果模型

SMS 应该把节点侧结果规范化成一套统一的 Web Admin 响应。

建议节点结果字段：

- `node_uuid`
- `provider`
- `success`
- `phase`
- `error_code`
- `message`
- `resolved_endpoint`
- `http_status`
- `provider_code`
- `latency_ms`
- `auth_valid`
- `model_accessible`
- `checks`

## 错误模型

建议使用稳定错误码：

- `invalid_input`
- `unsupported_provider`
- `unsupported_feature`
- `node_unreachable`
- `dns_resolve_failed`
- `connect_timeout`
- `tls_handshake_failed`
- `http_404`
- `http_5xx`
- `auth_missing`
- `auth_invalid`
- `permission_denied`
- `model_not_found`
- `model_not_accessible`
- `provider_rate_limited`
- `provider_unavailable`

建议 phase：

- `input`
- `connectivity`
- `auth`
- `model_access`
- `capability`

## 创建后的状态模型

系统应该把 backend 级的抽样验证状态和节点级的全量验证状态分开建模。

### Backend 级 Summary

建议状态：

- `pending`
- `sampled_verified`
- `sampled_failed`
- `fully_verified`
- `partially_verified`
- `failed`

### 节点级验证状态

建议状态：

- `pending`
- `ready`
- `auth_failed`
- `network_failed`
- `model_not_accessible`
- `timeout`

这些状态应通过现有 backend node status 视图和 backend summary 视图暴露出来。

## UI 行为

### Create Remote Backend

默认行为：

1. 用户点击 `Create`
2. Web Admin 自动调用 preflight
3. 如果失败，创建流程中断，并在对话框中展示结构化错误
4. 如果成功，则自动继续创建

### All Nodes Placement

创建前：

- 展示抽样验证结果
- 明确提示“创建后仍会继续执行全量验证”

创建后：

- backend summary 展示类似 `18/20 nodes verified`
- backend detail 页面展示节点级失败原因

## 安全考虑

- Web Admin 绝不能从 preflight 返回中拿到明文 secret
- SMS 应避免处理超过必要范围的 secret 材料
- SPEARlet 应复用运行时已有的 credential 解析链路
- preflight 日志必须对敏感 header 和 token 做脱敏
- 失败结果里不能回显 secret

## 推荐模块拆分

建议实现落点：

- SMS 编排层：
  - [`../src/sms/web_admin/ai_backend_admin.rs`](../src/sms/web_admin/ai_backend_admin.rs)
  - 可再抽出 `backend_preflight.rs` 等 helper
- SPEARlet 节点侧 handler：
  - 在 HTTP gateway 下新增统一 internal route
- provider adapter：
  - `remote_preflight/mod.rs`
  - `remote_preflight/common.rs`
  - `remote_preflight/openai_compatible.rs`
  - `remote_preflight/ollama.rs`
  - `remote_preflight/http_json.rs`

## 落地阶段

### Phase 1

- 统一 local 与 remote preflight 编排
- 增加节点侧 remote preflight 路由
- 支持 `single_node_strict`
- 支持 OpenAI-compatible 与 Ollama

### Phase 2

- 为 `all_nodes` 增加 `sampled_strict`
- 创建后增加异步全量验证
- 在 Web Admin 展示聚合验证进度

### Phase 3

- 增加可选的 `strict_all_nodes`
- 增加更丰富的 provider-specific capability checks
- 为 generic HTTP backend 增加可配置 healthcheck

## 测试计划

### 单元测试

- provider request 构造
- credential 解析失败
- error code 映射
- sampling policy 选择

### 集成测试

- Web Admin preflight 编排
- mock OpenAI-compatible provider 成功和失败
- mock Ollama provider 成功和失败
- `all_nodes` 下 sampled strict 行为

### UI 测试

- preflight 失败时阻止创建
- preflight 成功时继续创建
- sampled/full verification 的提示正确渲染

## 开放问题

- 第一版生产环境中，哪些拓扑标签应参与代表性抽样
- `generic http_json` 是否在第一版就支持自定义验证契约
- backend 验证 summary 应放入现有 node status，还是新增专门的 verification sub-status

## 最终建议

推荐默认设计是：

- 使用统一 backend preflight 框架
- 始终从目标 SPEARlet 节点发起验证
- 对单节点 remote backend 使用严格验证
- 对 `all_nodes` 在创建前采用抽样严格验证
- 创建后再执行异步全量节点验证
- 在 Web Admin 同时暴露 sampled 与 full verification 状态
