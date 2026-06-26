# Spear Canonical IR 设计（多操作/多模态）

本文件定义 Spear adapter layer 的“中间态（Canonical IR）”建议：它是 Hostcall Session（或其它 hostcall 输入）与后端请求之间的统一契约。

## 1. 设计目标

- 支持多操作：Chat、Embeddings、Image Generation、ASR、TTS、Realtime Voice。
- 兼容 OpenAI 生态：尽可能直通 OpenAI-compatible 后端。
- 可扩展：允许不同后端的非标准字段，避免过早把 IR 锁死。
- 可路由：显式携带能力需求与选路偏好。
- 可演进：IR 版本化，支持逐步覆盖更全的 schema。

## 2. 总体形态：统一 Envelope + 按操作分支的 Payload

建议所有 AI 相关 hostcall 都转换为统一的请求封装：

```text
CanonicalRequestEnvelope
  - meta
  - routing
  - requirements
  - policy
  - payload (oneof)
```

### 2.1 CanonicalRequestEnvelope（字段建议）

- `version: u32`
- `request_id: String`（或 u64）
- `task_id: Option<String>`
- `operation: Operation`
- `meta: RequestMeta`
  - `trace_id/span_id`（可选）
  - `created_at_ms`（可选）
- `routing: RoutingHints`
  - `backend: Option<String>`
  - `backend_allowlist/denylist: Option<Vec<String>>`
  - `region_hint/cost_hint/latency_hint`（可选）
- `requirements: Requirements`
  - `required_ops: Set<Operation>`（通常与 `operation` 一致，但允许组合）
  - `required_features: Set<Feature>`
  - `required_transports: Set<Transport>`
  - `preferred_backends: Vec<String>`（可选）
- `policy: Option<SelectionPolicySpec>`（请求级覆盖，受 host 配置约束）
- `timeout_ms: Option<u64>`
- `payload: oneof { ChatCompletionsPayload | EmbeddingsPayload | ImageGenerationPayload | SpeechToTextPayload | TextToSpeechPayload | ... }`
- `extra: Map<String, Value>`（未知字段透传与实验字段）

说明：

- `payload` 放在 Envelope 内部，且与 `operation` 对齐（通常 1:1），各操作的 payload 字段骨架定义见 `operations-zh.md`。
- 大体积二进制不建议直接进入 `payload` 的 JSON 字段；应通过 `MediaRef`（如 `sms_file`）表达数据位置，见本文件第 3 节。

### 2.2 CanonicalResponseEnvelope（字段建议）

建议将响应也统一封装，以便：WASM 消费统一、观测统一、错误统一。

- `version: u32`
- `request_id: String`
- `operation: Operation`
- `backend: BackendInfo`（可选）
  - `name/instance`
  - `latency_ms`
  - `attempts`（重试/hedge）
- `result: oneof { payload, error }`
- `raw: Option<Value>`（可选，调试/回放开关控制）

### 2.3 Error 结构（建议）

- `code: String`（如 `BackendNotEnabled`/`NoCandidateBackend`/`MissingCapabilities`/`Timeout`）
- `message: String`
- `retryable: bool`
- `operation: Operation`
- `required: RequirementsSummary`（可选）
- `candidates_checked: Vec<String>`（可选）
- `rejected_reasons: Map<String, String>`（可选）

## 3. MediaRef：图像/音频等大数据的统一引用

多模态操作会频繁承载图像与音频。建议 IR 统一使用 `MediaRef` 表达“数据在哪里”，避免把大块二进制强行塞进 JSON。

### 3.1 MediaRef 形态建议

- `inline_base64`：适合小数据、调试、兼容性强
  - `{ "kind": "inline_base64", "mime": "audio/wav", "data": "..." }`
- `sms_file`：推荐生产默认（大数据）
  - `{ "kind": "sms_file", "uri": "smsfile://<id>", "mime": "image/png" }`
- `http_url`：仅在明确允许时使用（受 host 安全策略约束）
  - `{ "kind": "http_url", "url": "https://...", "mime": "image/jpeg" }`

### 3.2 MediaRef 在 IR 中的使用位置（建议）

`MediaRef` 应作为 Canonical IR 里“媒体字段”的统一类型，通常出现于以下位置：

- 请求侧（输入媒体）：
  - `speech_to_text`：`SpeechToTextPayload.audio: MediaRef`
  - `image_generation`：`ImageGenerationPayload.image: Option<MediaRef>`、`mask: Option<MediaRef>`
  - 多模态 `chat_completions`：`Message.content_parts[]` 中的 `image/audio` part 使用 `MediaRef`
- 响应侧（输出媒体）：
  - `image_generation`：返回 `images: Vec<MediaRef>`
  - `text_to_speech`：返回 `audio: MediaRef`

建议避免把大体积二进制直接内嵌在 response JSON 中；优先返回 `sms_file`，并在需要时由调用方再拉取。

### 3.3 规范化建议

- hostcall 输入为字节数组时：优先落盘/对象存储并转为 `sms_file`，避免内存峰值。
- backend adapter 需要 inline 时：由 adapter 决定临时读取并编码，而不是 IR 固定为 inline。

### 3.4 示例

#### 3.4.1 ASR（`speech_to_text`）请求：音频作为 `sms_file`

```json
{
  "version": 1,
  "request_id": "req-123",
  "operation": "speech_to_text",
  "requirements": {
    "required_ops": ["speech_to_text"],
    "required_features": ["supports_audio_input"],
    "required_transports": ["http"]
  },
  "payload": {
    "model": "whisper-1",
    "audio": {
      "kind": "sms_file",
      "uri": "smsfile://01JFK...ABCD",
      "mime": "audio/wav"
    },
    "language": "zh",
    "timestamps": true
  }
}
```

#### 3.4.2 图像生成（`image_generation`）响应：图片作为 `sms_file`

```json
{
  "version": 1,
  "request_id": "req-456",
  "operation": "image_generation",
  "backend": { "name": "openai-us", "instance": "openai-us-1", "latency_ms": 842 },
  "result": {
    "payload": {
      "images": [
        { "kind": "sms_file", "uri": "smsfile://01JFK...WXYZ", "mime": "image/png" }
      ]
    }
  }
}
```

#### 3.4.3 多模态 Chat：消息内容引用图片

```json
{
  "operation": "chat_completions",
  "payload": {
    "model": "gpt-4o",
    "messages": [
      {
        "role": "user",
        "content_parts": [
          { "type": "text", "text": "描述这张图片" },
          {
            "type": "image",
            "image": { "kind": "sms_file", "uri": "smsfile://01J...", "mime": "image/jpeg" }
          }
        ]
      }
    ]
  }
}
```

### 3.5 规范化/解析职责边界（建议）

- Normalize 层（host 侧）职责：
  - 当 hostcall 输入为原始字节时，优先写入对象存储/文件服务并生成 `sms_file` 类型的 `MediaRef`。
  - 对 `http_url` 做安全策略校验（若不允许则拒绝或转存为 `sms_file`）。
- Backend adapter（发送前）职责：
  - 根据后端协议需求把 `MediaRef` 解析为 bytes（拉取 `sms_file`、解码 `inline_base64`），并组装成 multipart/base64/二进制请求体。
  - 若后端返回 base64 图片/音频，优先转存为 `sms_file` 再写入 Canonical Response（除非明确要求 inline 且受大小限制）。
- Router 层职责：
  - 不解析媒体内容本身，但可以基于 `mime`、媒体大小/时长等元信息做能力过滤与限制控制（如 `max_audio_seconds`）。

## 4. Payload 的演进策略

为避免一次性把所有操作字段定死，建议：

- 每个 `*Payload` 的主干字段强类型化；
- 不同后端私有字段一律通过 `extra` 承载；
- 能力差异与降级由 router/adapter 层显式处理，不在 IR 内“静默丢字段”。

各操作 payload 的字段建议与能力映射详见：`operations-zh.md`。
