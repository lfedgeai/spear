# Capabilities 路由与选路策略

本文件定义能力建模、候选过滤、以及当一个能力有多个后端可用时的选路策略（LB/Fallback/Hedge/Mirror）。

## 1. 能力建模（Capabilities）

建议将能力拆为四类：

- `Operation`：高层操作（跨 hostcall 的统一语义）
- `Feature`：细项能力（工具调用、JSON schema、timestamps、mask 等）
- `Limits`：约束（token 上限、音频长度、并发）
- `Transport`：http/websocket/grpc

核心原则：能力属于 **backend instance**（同一 backend kind 的不同实例可能模型/限额不同）。

参考 legacy：

- `OpenAIFunctionType` 的类别划分与 `APIEndpointMap` 的多候选端点（`legacy/spearlet/core/models.go`）
- transform 子集选择（`legacy/spearlet/hostcalls/transform.go`）

## 2. 候选过滤（Hard Constraints）

路由第一步是过滤：

- 必须满足 `required_ops`（通常等于请求的 `operation`）
- 必须满足 `required_features` 与 `required_transports`
- 必须通过 allowlist/denylist
- 必须健康（或满足软健康策略）
- 必须通过并发/限流/配额

过滤输出是候选实例集合 `candidates[]`。

## 3. 打分与选择（Soft Preferences）

过滤后按偏好打分并选择：

- `preferred_backends` 加分
- 可选：region/cost/latency hints 加分
- 可选：模型匹配（同一操作多个 model）加分

## 4. 策略（Selection Policies）

### 4.1 单选（最常用）

- `round_robin`
- `weighted_round_robin`
- `weighted_random`
- `least_inflight`
- `ewma_latency`
- `consistent_hash(key=session_id|task_id)`

### 4.2 主备与降级（Fallback）

- `priority + fallback`：先选高优先级池，失败/超时/429 时切到下一优先级
- 建议结合熔断/剔除：连续失败或高错误率的实例临时降权或移出候选

### 4.3 并发多发（Hedged / Fan-out）

适用于 tail latency 敏感且预算允许的场景：

- `hedged(k=2, delay_ms=50)`：主请求未返回则补发
- `mirror_to`：主链路返回 primary，同时镜像到 secondary 做离线评估

## 5. 按 Operation 的默认策略建议

- `chat_completions`：`ewma_latency` 或 `least_inflight`
- `embeddings`：`weighted_round_robin`
- `image_generation`：`priority + fallback`（默认不 hedged）
- `speech_to_text`：`least_inflight` 或 `weighted_rr`
- `text_to_speech`：`weighted_rr`
- 规划项：若未来恢复 `realtime_voice`，优先考虑 `least_inflight`（当前运行时未实现）

## 6. 输出与可解释性

路由器应能输出结构化“解释信息”以便诊断：

- `candidates_checked`
- `rejected_reasons`
- `selected_instance(s)`
- `policy` 与关键打分信号
