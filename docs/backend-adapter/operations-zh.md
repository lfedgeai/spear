# Spear 多操作与多模态能力设计（Operations & Capabilities）

本文件补充 `Operation` 维度的建议：每个操作的典型请求字段、关键能力（Feature/Limits/Transport）、以及常见降级策略。

## 1. Operation 总表（建议）

| Operation | 输入模态 | 输出模态 | 典型 Transport | 常见 Feature |
|---|---|---|---|---|
| `chat_completions` | text（可扩 image/audio） | text/tool_calls | http | `supports_stream` `supports_tools` `supports_json_schema` |
| `embeddings` | text | vector | http | `supports_batch` `supports_dimensions` |
| `image_generation` | text（可扩 image/mask） | image | http | `supports_image_input` `supports_mask` `supports_seed` |
| `speech_to_text`（ASR） | audio | text | http（可扩 ws） | `supports_timestamps` `supports_language` `supports_diarization` |
| `text_to_speech`（TTS） | text | audio | http | `supports_voices` `supports_formats` `supports_ssml` |
| `realtime_voice`（规划） | audio/text events | audio/text events | websocket/grpc | `supports_bidi_stream` `supports_audio_in/out` |

说明：表中是“能力建模的最小核心”，不要求一次实现全部字段。`realtime_voice` 当前仅作为历史设计/规划项保留，不代表现有运行时支持。

## 2. 每个操作的 Payload 字段建议（骨架）

本节只给“强类型主干字段”的骨架。未覆盖字段或后端私有字段统一进入 `extra`。

### 2.1 `ChatCompletionsPayload`

- `model: String`
- `messages: Vec<Message>`（role + content parts）
- `tools: Option<Vec<ToolSpec>>`
- `tool_choice: Option<ToolChoice>`
- `generation: GenerationParams`（temperature/top_p/max_tokens/stop/seed/stream...）
- `response_format: Option<ResponseFormat>`

关键能力：

- `supports_tools`
- `supports_json_schema`
- `supports_stream`

常见降级：

- 无 `supports_tools`：可选“tool prompt 注入”降级或直接报错（由 `degradation_policy` 决定）
- 无 `supports_system_role`：将 system 合并到第一条 user（显式标记）

### 2.2 `EmbeddingsPayload`

- `model: String`
- `input: Vec<String> | String`（建议统一到列表）
- `dimensions: Option<u32>`（部分后端支持）
- `encoding_format: Option<String>`（如 `float`/`base64`）

关键能力：

- `supports_batch`
- `supports_dimensions`

常见降级：

- 不支持 batch：router/adapter 拆分为多次单条请求（注意限流与成本）

### 2.3 `ImageGenerationPayload`

- `model: String`
- `prompt: String`
- `image: Option<MediaRef>`（img2img/edit 可用）
- `mask: Option<MediaRef>`（edit 可用）
- `n: Option<u32>`
- `size: Option<String>`（如 `1024x1024`）
- `quality/style/seed: Option<...>`
- `response_format: Option<String>`（建议输出 `MediaRef`，而不是强制 base64）

关键能力：

- `supports_image_input`（img2img）
- `supports_mask`
- `max_image_size` `max_images_per_request`

常见降级：

- 不支持 mask/edit：降级为纯 text2img 或报错

### 2.4 `SpeechToTextPayload`（ASR）

- `model: String`
- `audio: MediaRef`
- `language: Option<String>`
- `prompt: Option<String>`
- `timestamps: Option<bool>`
- `diarization: Option<bool>`
- `temperature: Option<f32>`

关键能力：

- `supports_timestamps`
- `supports_language`
- `supports_diarization`
- `max_audio_seconds`

常见降级：

- 不支持 timestamps：关闭 timestamps
- 过长音频：分片（需要明确边界与拼接策略）或拒绝

### 2.5 `TextToSpeechPayload`（TTS）

- `model: String`
- `input: String`
- `voice: Option<String>`
- `format: Option<String>`（wav/mp3/pcm）
- `speed: Option<f32>`
- `ssml: Option<bool>`

关键能力：

- `supports_voices`
- `supported_formats`
- `supports_ssml`
- `max_chars`

常见降级：

- voice 不支持：回退到默认 voice
- ssml 不支持：拒绝或剥离标签（需显式）

### 2.6 `RealtimeVoicePayload`（历史规划，当前未实现）

Realtime 是流式会话，不建议用“单 request single response”抽象。建议 payload 只描述：

- `session_config`（模型、语音、turn-taking、vad、语言等）
- `input_stream: StreamRef`（抽象）
- `output_events: EventSinkRef`（抽象）

关键能力：

- `supports_bidi_stream`
- `transport=websocket|grpc`
- `max_session_seconds` `max_frame_bytes`

常见降级：

- 无 realtime：按策略降级为 `speech_to_text`（非实时）或拒绝

## 3. 能力声明与路由建议（按操作默认）

### 3.1 `default_policy_by_operation`（建议）

- `chat_completions`：`ewma_latency` 或 `least_inflight`（兼顾 P95/P99）
- `embeddings`：`weighted_round_robin`（吞吐优先）
- `image_generation`：`priority + fallback`（成本高，默认不 hedged）
- `speech_to_text`：`least_inflight` 或 `weighted_rr`
- `text_to_speech`：`weighted_rr`
- 规划项：`realtime_voice` 可优先考虑 `least_inflight`（当前运行时未实现）

### 3.2 hedged/mirror 的适用性

- image 默认不 hedged；可以 mirror 做离线评估
- realtime/ASR 对 tail latency 敏感但成本也高，建议只在预算明确时启用轻量 hedged

## 4. Hostcall family 扩展建议

你当前有 `cchat_*`。当增加其它能力时有两条路线：

命名约定建议：

- family 前缀表达操作语义，不要求统一 `c` 前缀。
- `cchat` 代表 completion chat，因此保持 `cchat_*` 合理；其它能力建议使用语义前缀。

### 路线 A：按操作拆分 hostcall（语义清晰）

- `cchat_*`：completion chat
- `emb_*`：embeddings
- `img_*`：image（image generation/edit/variation 等）
- `asr_*`：speech-to-text（ASR）
- `tts_*`：text-to-speech（TTS）
- `rt_*`：realtime（或复用 stream 子系统）

### 路线 B：统一 `ai_call/transform` hostcall（扩展快）

用一个 hostcall 承载 `operation + payload`，host 侧按 operation 分发。

选择建议：

- 如果你追求 ABI 稳定且易调试：优先路线 A
- 如果你更像 legacy Go 的 `TransformRegistry` 想快速扩展：路线 B 更贴近

无论选哪条路线，secret 与 URL 不应由 WASM 提供。
