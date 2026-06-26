# SPEAR Console 语音输入（按住说话）设计

## 1. 目标

- 在 SPEAR Console 中支持类似微信的“按住说话”交互：按下开始采集语音，松开提交。
- 语音数据通过现有 WebSocket（二进制 SSF frame）发送到 execution，并由执行侧 Agent 完成 ASR（Speech-to-Text）后进入对话流程。
- 保持架构模块化：Console 只做采集与传输；语音识别与对话策略在 Agent 内部实现。

## 2. 非目标

- 不引入 WebRTC。
- 不在 SMS/spearlet 网关层新增“语音专用协议解析/业务逻辑”（仍保持二进制透传与鉴权）。
- 不做向后兼容：本设计要求客户端对每个 stream 在发送 DATA/COMMIT 之前必须先发送 CTRL（OPEN）。

## 3. 关键约定

### 3.1 Stream ID 分配

- `stream_id=1`：文本对话输入/输出（与现有 Console 聊天窗口绑定）。
- `stream_id=2`：语音输入 uplink（Console → Agent）。

说明：
- `stream_id=2` 仅承载“语音输入”，识别结果（文本）以及模型回答统一写回 `stream_id=1`，避免前端额外渲染路径。

### 3.2 SSF 帧与 meta 编码

- on-wire：WebSocket 的每个 binary message 承载且仅承载一个 SSF v1 frame（参见 `docs/api/spear-hostcall/wasm-user-stream-bridge-zh.md`）。
- `meta`：UTF-8 JSON（对象），不得是空字符串；最小值为 `{}`。
- `data`：
  - 文本：UTF-8 bytes（对应 JS/Rust 的 TextEncoder/encodeUtf8）。
  - 音频：PCM16LE bytes（little-endian），当前 Console 实现默认使用 16000Hz、单声道。

## 4. 协议：meta JSON 规范（可直接落地）

### 4.1 顶层通用字段

所有 `meta` JSON（无论 CTRL/DATA/COMMIT）都必须是对象，允许携带以下字段：

- `v`（number，必填）：meta 版本号，固定为 `1`。
- `kind`（string，必填）：`"open"` / `"utterance_begin"` / `"data"` / `"commit"`。
- `modality`（string，必填）：`"text"` / `"audio"`。
- `ts_ms`（number，可选）：客户端时间戳（毫秒）。
- `trace_id`（string，可选）：用于端到端排障的请求标识。

接收方必须忽略未知字段。

### 4.2 文本流（stream_id=1）

#### 4.2.1 CTRL(OPEN)

- `msg_type=CTRL`
- `stream_id=1`
- `meta`：

```json
{
  "v": 1,
  "kind": "open",
  "modality": "text",
  "encoding": "utf-8",
  "ts_ms": 1710000000000
}
```

约束：
- 在该 CTRL 之前不得对 `stream_id=1` 发送 DATA/COMMIT。

#### 4.2.2 DATA（文本分片）

说明：
- DATA/COMMIT 的语义由 SSF `msg_type` 决定；meta 仅用于关联与调试（例如 `message_id`/`utterance_id`/`ts_ms`），不再要求 `kind/modality`。

```json
{
  "v": 1,
  "message_id": "m_01",
  "seq_in_message": 1,
  "ts_ms": 1710000000100
}
```

其中：
- `data`：UTF-8 文本 bytes。
- 允许一个 message 被拆成多条 DATA。

#### 4.2.3 COMMIT（文本提交）

```json
{
  "v": 1,
  "message_id": "m_01",
  "ts_ms": 1710000000200
}
```

语义：
- 标记 `message_id` 的输入结束，Agent 侧可以把累积的文本作为一次用户输入处理。

### 4.3 语音流（stream_id=2）

#### 4.3.1 CTRL(OPEN)

- `msg_type=CTRL`
- `stream_id=2`
- `meta`：

```json
{
  "v": 1,
  "kind": "open",
  "modality": "audio",
  "audio": {
    "format": "pcm_s16le",
    "sample_rate_hz": 16000,
    "channels": 1
  },
  "ts_ms": 1710000000000
}
```

约束：
- 在该 CTRL 之前不得对 `stream_id=2` 发送 DATA/COMMIT。

#### 4.3.2 CTRL(UTTERANCE_BEGIN)（按下开始）

在用户按下“按住说话”后立即发送：

```json
{
  "v": 1,
  "kind": "utterance_begin",
  "modality": "audio",
  "utterance_id": "u_01",
  "ts_ms": 1710000001000
}
```

语义：
- 声明一次新的语音片段（utterance）开始。
- Agent 侧据此创建/复用 ASR 会话并清空上一段缓存。

#### 4.3.3 DATA（音频 chunk）

说明：
- DATA/COMMIT 的语义由 SSF `msg_type` 决定；meta 仅用于关联与调试（例如 `utterance_id`/`chunk_index`/`ts_ms`），不再要求 `kind/modality`。

```json
{
  "v": 1,
  "utterance_id": "u_01",
  "chunk_index": 1,
  "ts_ms": 1710000001020
}
```

其中：
- `data`：PCM16LE bytes（当前 Console 默认 `chunkMs=300`，具体分片大小仍由前端实现决定）。

#### 4.3.4 COMMIT（松开提交）

用户松开按钮后发送：

```json
{
  "v": 1,
  "utterance_id": "u_01",
  "ts_ms": 1710000002200
}
```

语义：
- 标记该 utterance 输入结束，Agent 侧应触发 ASR flush 并等待最终转写结果。

## 5. 前端（Console）模块拆分与接口

### 5.1 新增目录结构（建议）

在 `web-console/src/` 下新增：

- `voice/`
  - `types.ts`：语音配置与事件类型
  - `pcm16.ts`：Float32 → PCM16LE 编码
  - `micCapture.ts`：麦克风采集（WebAudio），对外输出 PCM16 chunk
  - `voiceStreamSender.ts`：将 PCM16 chunk + meta 封装为 SSF 并发送到 `stream_id=2`
  - `PressToTalkButton.tsx`：按住说话按钮组件
  - `index.ts`：统一导出

### 5.2 核心接口（TS 伪代码）

#### 5.2.1 `MicCapture`

```ts
export type MicConfig = {
  sampleRateHz: 16000
  channels: 1
  chunkMs: number
}

export type MicChunk = {
  tsMs: number
  pcm16le: Uint8Array
}

export interface MicCapture {
  start(): Promise<void>
  stop(): Promise<void>
  onChunk(cb: (c: MicChunk) => void): () => void
}
```

约束：
- `start()` 后必须持续回调 `onChunk`，直到 `stop()`。
- 失败必须抛错并向 UI 报告（权限拒绝/设备缺失等）。

#### 5.2.2 `VoiceStreamSender`

```ts
export type VoiceSession = {
  utteranceId: string
  chunkIndex: number
}

export interface VoiceStreamSender {
  ensureStreamOpen(): void
  beginUtterance(nowMs: number): VoiceSession
  sendChunk(session: VoiceSession, chunk: MicChunk): void
  commitUtterance(session: VoiceSession, nowMs: number): void
}
```

约束：
- `ensureStreamOpen()`：确保已对 `stream_id=2` 发送 CTRL(OPEN)。
- `beginUtterance()`：发送 CTRL(UTTERANCE_BEGIN) 并返回 session。

#### 5.2.3 `PressToTalkButton`

```ts
export type PressToTalkButtonProps = {
  disabled: boolean
  onPressStart: () => void
  onPressEnd: () => void
}
```

事件策略：
- `pointerdown` → `onPressStart`
- `pointerup | pointercancel | pointerleave` → `onPressEnd`

### 5.3 具体文件改造清单（Console）

#### 5.3.1 修改：`web-console/src/spearStream.ts`

- 将 `connect()` 的 `autoOpenStreamId?: number` 扩展为 `autoOpenStreamIds?: number[]`，连接建立后依次发送 CTRL(OPEN)。
- 默认在 Console 连接成功时 open `stream_id=1` 与 `stream_id=2`。

#### 5.3.2 修改：`web-console/src/App.tsx`

- 在输入区新增 `PressToTalkButton`。
- 在连接成功后，初始化 `MicCapture` 与 `VoiceStreamSender`，并在 UI 状态中暴露：
  - `micPermissionState`
  - `isRecording`
  - `lastVoiceError`

#### 5.3.3 新增：`web-console/src/voice/*`

- 按 5.1 的模块拆分新增文件。

## 6. Agent 侧状态机与实现路径

### 6.1 输入/输出约定

- Agent 必须打开并监听：
  - `stream_id=1`：文本输入（DATA/COMMIT）与输出（写回 DATA）。
  - `stream_id=2`：语音输入（CTRL/DATA/COMMIT）。
- Agent 可以将“ASR 转写文本”与“LLM 回复文本”都写回 `stream_id=1`（便于复用现有 Console 聊天渲染）。

### 6.2 语音状态机（Idle / Recording / Committing）

#### 6.2.1 状态定义

- `Idle`：未开始一段 utterance；等待 `CTRL(kind=utterance_begin)`。
- `Recording`：已开始 utterance；持续接收音频 DATA 并写入 ASR。
- `Committing`：收到 COMMIT；触发 ASR flush，读取最终 transcript，并将 transcript 作为一次用户输入进入 chat；完成后回到 `Idle`。

#### 6.2.2 状态迁移

- `Idle` + `utterance_begin` → `Recording`
- `Recording` + `DATA(audio)` → `Recording`（持续写入 ASR）
- `Recording` + `COMMIT(audio)` → `Committing`
- `Committing` + `asr_done` → `Idle`
- 任意状态 + `session_closed/error` → 关闭 fd 并退出（或进入错误恢复策略，MVP 可直接退出）

### 6.2.3 当前 sample 兼容性说明

- `user_stream_live_caption` 与 `user_stream_voice_chat` 同时兼容 `input_audio_transcription.*` 和 `conversation.item.input_audio_transcription.*` 两套 RTASR 事件名。
- `user_stream_live_caption` 还会把 `speech_started` 与 `input_audio_buffer.speech_started` 都视为一次新 utterance 的重置事件。
- 按住说话松手时，会在 `stream_id=2` 上发送 voice COMMIT；执行侧 sample 会以这个 COMMIT 边界触发 RTASR flush。

### 6.3 推荐落地位置（最小可工作：WASM C Agent）

原因：
- `rtasr_*` hostcall 已存在于 C SDK 与 wasm-sys ABI，可直接使用，不依赖额外语言绑定。

当前 sample 落地点：

- `samples/wasm-c/user_stream_voice_chat.c`：实现完整的状态机与 rtasr/cchat 调用
- `samples/wasm-c/user_stream_live_caption.c`：实现语音到字幕的链路，不进入下游对话
- `samples/wasm-js/user_stream_voice_chat/src/entry.mjs`：JS-first 版本的 voice chat 流程
- `samples/wasm-js/user_stream_live_caption/src/entry.mjs`：JS-first 版本的 live caption 流程
- `samples/wasm-rust/user_stream_live_caption/src/main.rs`：Rust-first 版本的 live caption 流程，基于 Rust SDK `epoll` wrapper 与强类型模块拆分实现
- `samples/wasm-rust/user_stream_voice_chat/src/main.rs`：Rust-first 版本的 voice chat 流程，基于 Rust SDK `epoll` wrapper、RTASR helper 与下游 Chat Completion 适配层实现

实现要点（MVP）：
- 仅验证语音 → 文本链路：收到语音 COMMIT 后，把最终 transcript 写回 `stream_id=1`（不调用 LLM）。
- 下一步再接入 `cchat_*`：把 transcript 作为 user message，拿到模型回复后写回 `stream_id=1`。

## 7. 最小可工作的实现路径（MVP）

### 阶段 A：打通传输与 UI（无需 ASR）

- 前端完成：
  - `stream_id=2` 的 CTRL(OPEN)、CTRL(UTTERANCE_BEGIN)、DATA(audio)、COMMIT(audio) 发送。
  - UI 显示录音中状态与权限错误。
- Agent 暂时只做“收到 audio DATA/COMMIT 的计数与回显”，用于确认帧到达与分段正确。

### 阶段 B：引入 ASR（rtasr_fd）

- Agent 在 `Recording` 状态将每个音频 chunk 写入 `rtasr_write`。
- `Committing` 触发 flush，并读取最终 transcript。
- 对 JS 版 `live_caption`，会先聚合多个 PCM chunk，再执行一次 `rtasr.writeAudio()`，以减少 JS/WASM bridge 开销。
- 将 transcript 写回 `stream_id=1`。

### 阶段 C：转写文本进入对话

- Agent 将 transcript 作为一次文本输入走现有 chat completion 逻辑，并将模型输出写回 `stream_id=1`。

## 8. 测试与验收

- 手工验收（MVP）：
  - 浏览器授权麦克风后，按住说话 1~3 秒，松开后在聊天窗口看到转写文本（阶段 B）或模型回复（阶段 C）。
  - 拒绝麦克风权限时 UI 有明确错误提示且不会发送音频帧。
- 协议一致性：
  - 若未发送 CTRL(OPEN) 就发送 DATA/COMMIT，Agent 直接报错并关闭 stream（不兼容策略）。

## 9. SDK 辅助（SSF + CTRL meta）

为便于 WASM guest 侧处理 CTRL/DATA/COMMIT，本仓库在 SDK 中提供了“零拷贝拆分 frame”与“解析 meta JSON”的辅助：

- C SDK：
  - SSF v1 frame 解析与拆分：`sdk/c/include/spear_ssf.h`（`sp_ssf_v1_parse_header` / `sp_ssf_v1_split`）
  - meta 子串匹配：`sp_ssf_meta_contains`（best-effort，建议优先使用 JSON 解析器）
- Rust SDK（WASM guest）：
  - meta v1 解析：`spear_wasm::ssf_meta::{parse_ctrl_meta_v1, parse_data_meta_v1}`
  - 严格模式：`parse_data_meta_v1` 会拒绝携带 `kind/modality/audio` 的 DATA/COMMIT meta
- Boa JS runtime（WASM-JS sample）：
  - meta v1 解析：`import * as ssf from "spear/ssf"; ssf.parseCtrlMetaV1(...) / ssf.parseDataMetaV1(...)`

## 10. 安全与隐私

- Console 不在前端日志中输出音频 bytes。
- SMS 的 stream session token 已是短期 token；不要在页面上展示或复制该 token。
- 语音输入默认走与文本一致的鉴权边界：Console → SMS（鉴权）→ spearlet（内网）。
