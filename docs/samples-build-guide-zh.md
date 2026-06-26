# WASM 示例构建指南（samples）

## 目录结构
- 源码：`samples/wasm-c/hello.c`
- 源码：`samples/wasm-c/chat_completion.c`（Chat Completions 示例）
- 源码：`samples/wasm-c/mic_rtasr.c`（实时麦克风→实时ASR示例）
- 源码：`samples/wasm-c/user_stream_echo.c`（双向 user stream echo 示例）
- 源码：`samples/wasm-c/user_stream_voice_chat.c`（Console 按住说话：语音→转写→对话 示例）
- 源码：`samples/wasm-c/user_stream_live_caption.c`（Console 语音上行→RTASR 增量字幕 示例）
- 源码：`samples/wasm-js/chat_completion/src/main.rs`（Boa JS runner 编译为 WASM；内部执行 `entry.mjs` → Chat Completion）
- 源码：`samples/wasm-js/chat_completion_tool_sum/src/main.rs`（Boa JS runner 编译为 WASM；内部执行 `entry.mjs` → tool calling）
- 源码：`samples/wasm-js/router_filter_keyword/src/main.rs`（Boa JS runner 编译为 WASM；内部执行 `entry.mjs` → Router 关键词过滤）
- 源码：`samples/wasm-js/user_stream_echo/src/main.rs`（Boa JS runner 编译为 WASM；内部执行 `entry.mjs` → 双向 user stream echo）
- 源码：`samples/wasm-js/user_stream_chat_completion/src/main.rs`（Boa JS runner 编译为 WASM；内部执行 `entry.mjs` → user stream 用户输入 → Chat Completion）
- 源码：`samples/wasm-js/user_stream_live_caption/src/main.rs`（Boa JS runner 编译为 WASM；内部执行 `entry.mjs` → 语音上行 → RTASR 实时字幕）
- 源码：`samples/wasm-js/user_stream_voice_chat/src/main.rs`（Boa JS runner 编译为 WASM；内部执行 `entry.mjs` → 语音上行 → RTASR 转写 → Chat Completion）
- 源码：`samples/wasm-rust/user_stream_live_caption/src/main.rs`（Rust guest SDK 编译为 WASM；运行模块化、epoll 驱动的实时字幕状态机）
- 源码：`samples/wasm-rust/user_stream_voice_chat/src/main.rs`（Rust guest SDK 编译为 WASM；运行模块化、epoll 驱动的语音对话状态机）
- 共享 SDK helper crate：`sdk/rust/crates/spear-wasm-helper/src/lib.rs`（承载 Rust-first 示例共享的 user stream 状态、SSF 解析、RTASR 事件解析，以及 RTASR 基础会话骨架）
- 产物：`samples/build/hello.wasm`
  - WASM-JS 产物：`samples/build/js/js-*.wasm`（兼容：`samples/build/rust/*.wasm`）
  - WASM-JS 产物：`samples/build/js/js-*.wasm`
  - WASM-Rust 产物：`samples/build/rust/*.wasm`

## mic_rtasr 示例运行前提

- 需要宿主二进制启用 `mic-device`（否则 `source=device` 会失败）
- 需要宿主配置可用的 realtime ASR backend（例如 `openai_realtime_ws`）以及对应 API Key
- 默认使用 backend 名称 `openai-realtime-asr`，可在构建时通过 `-DSP_RTASR_BACKEND=\"...\"` 覆盖

说明：`mic_rtasr` 示例默认使用 `server_vad` 做分段（按静音切分）。

建议配置方式：

- Spearlet 配置示例：`config/spearlet/config.toml`（包含 `openai_realtime_ws` 的 backend 示例）
- 运行前设置环境变量：`OPENAI_REALTIME_API_KEY`

运行方式：构建 `samples/build/mic_rtasr.wasm` 后，将其作为 WASM 可执行文件上传并创建任务运行（具体上传/创建任务流程见 `docs/api-usage-guide-zh.md`）。

## chat_completion 示例

- 默认使用 `SP_MODEL`（默认 `gpt-4o-mini`）作为请求模型
- 可选：构建时定义 `SP_ROUTE_OLLAMA_GEMMA3`，会将 `model` 切换为 `SP_OLLAMA_GEMMA3_MODEL`（默认 `gemma3:1b`）
- 响应 JSON 会附带 `_spear.backend`（最终路由到的 backend 名称）与 `_spear.model`（请求模型）；示例会打印 `debug_backend=...`

## 构建命令
- 运行：`make samples`
- 编译器优先级：
  - 优先使用 `zig`：`zig cc -target wasm32-wasi`
  - 备选 `clang`：需要设置 `WASI_SYSROOT` 指向 WASI SDK 的 sysroot

WASM-JS 示例：
- 通过 `cargo build --release --target wasm32-wasip1` 构建
- 可通过 Makefile 变量控制：
  - `BUILD_JS_SAMPLES=0` 跳过 WASM-JS 示例构建
  - `JS_SAMPLES="chat_completion chat_completion_tool_sum router_filter_keyword user_stream_echo user_stream_chat_completion user_stream_live_caption user_stream_voice_chat"` 指定要构建的示例列表
  - `JS_WASM_PREFIX="js-"` 配置 JS 产物文件名前缀（默认 `js-`）

WASM-Rust 示例：
- 通过 `cargo build --release --target wasm32-wasip1` 构建
- 可通过 Makefile 变量控制：
  - `BUILD_RUST_SAMPLES=0` 跳过 WASM-Rust 示例构建
  - `RUST_SAMPLES="user_stream_live_caption user_stream_voice_chat"` 指定要构建的 Rust-first 示例列表
  - `SAMPLES_RUST_DIR="samples/wasm-rust"` 指向 Rust-first 示例根目录

## clang 使用说明
- 环境变量：`WASI_SYSROOT=/opt/wasi-sdk/share/wasi-sysroot`（按实际路径）
- 命令会使用：`clang --target=wasm32-wasi --sysroot=$(WASI_SYSROOT)`
- 如未设置或未安装 SDK，会报错并提示安装 `zig` 或设置 `WASI_SYSROOT`

## 重要变更
- `make samples` 会同时构建 WASM-C、WASM-JS 与 WASM-Rust 示例，产物统一写到 `samples/build/` 下

## 语音 user stream 行为
- `stream_id=1` 是文本流，用于字幕输出、提交后的文本输入以及模型回复。
- `stream_id=2` 是语音上行流；运行时要求在发送 DATA/COMMIT 前先发 CTRL(OPEN)。
- 按住说话链路会先发送 `utterance_begin`，持续发送 PCM16LE DATA，松手时发送 voice COMMIT。
- `live_caption` 与 `voice_chat` 示例同时兼容 `input_audio_transcription.*` 和 `conversation.item.input_audio_transcription.*` 两套 RTASR 事件名。
- 当前 Console 默认配置为 `16000Hz`、单声道、`chunkMs=300`；JS 版 `live_caption` 使用 `server_vad` 且 `silence_ms=600`，并会先聚合约 `240ms` 的 PCM，再执行一次 `rtasr.writeAudio()`。
- Rust 版 `live_caption` 在协议行为上与之保持一致，但实现方式改为 Rust SDK `epoll` wrapper + 强类型模块拆分，而不是依赖 Boa JS runner。
- Rust 版 `voice_chat` 在协议行为上与 JS 版保持一致：等待 voice COMMIT，完成 RTASR 转写，再调用下游 Chat Completion，并将结果写回 `stream_id=1`。

## 与运行时集成
- 构建生成的 `hello.wasm` 可通过 SMS 文件服务上传后在任务注册中以 `executable.uri` 引用
- Spearlet WASM 运行时在实例创建阶段将校验模块字节格式，非法内容会报错

## 示例源码
```c
int main() { return 0; }
```
