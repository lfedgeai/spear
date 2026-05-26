# WASM 示例构建指南（samples）

## 目录结构
- 源码：`samples/wasm-c/hello.c`
- 源码：`samples/wasm-c/chat_completion.c`（Chat Completions 示例）
- 源码：`samples/wasm-c/mic_rtasr.c`（实时麦克风→实时ASR示例）
- 源码：`samples/wasm-js/chat_completion/src/main.rs`（Boa JS runner 编译为 WASM；内部执行 `entry.mjs` → Chat Completion）
- 源码：`samples/wasm-js/chat_completion_tool_sum/src/main.rs`（Boa JS runner 编译为 WASM；内部执行 `entry.mjs` → tool calling）
- 源码：`samples/wasm-js/router_filter_keyword/src/main.rs`（Boa JS runner 编译为 WASM；内部执行 `entry.mjs` → Router 关键词过滤）
- 源码：`samples/wasm-js/user_stream_echo/src/main.rs`（Boa JS runner 编译为 WASM；内部执行 `entry.mjs` → 双向 user stream echo）
- 源码：`samples/wasm-js/user_stream_chat_completion/src/main.rs`（Boa JS runner 编译为 WASM；内部执行 `entry.mjs` → user stream 用户输入 → Chat Completion）
- 产物：`samples/build/hello.wasm`
  - WASM-JS 产物：`samples/build/js/js-*.wasm`（兼容：`samples/build/rust/*.wasm`）

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
  - `BUILD_JS_SAMPLES=0` 跳过 WASM-JS 示例构建（兼容：`BUILD_RUST_SAMPLES=0`）
  - `JS_SAMPLES="chat_completion chat_completion_tool_sum router_filter_keyword user_stream_echo user_stream_chat_completion"` 指定要构建的示例列表（兼容：`RUST_SAMPLES=...`）
  - `JS_WASM_PREFIX="js-"` 配置 JS 产物文件名前缀（默认 `js-`）

## clang 使用说明
- 环境变量：`WASI_SYSROOT=/opt/wasi-sdk/share/wasi-sysroot`（按实际路径）
- 命令会使用：`clang --target=wasm32-wasi --sysroot=$(WASI_SYSROOT)`
- 如未设置或未安装 SDK，会报错并提示安装 `zig` 或设置 `WASI_SYSROOT`

## 重要变更
- `make samples` 会同时构建 WASM-C 与 WASM-Rust 示例，产物统一写到 `samples/build/` 下

## 与运行时集成
- 构建生成的 `hello.wasm` 可通过 SMS 文件服务上传后在任务注册中以 `executable.uri` 引用
- Spearlet WASM 运行时在实例创建阶段将校验模块字节格式，非法内容会报错

## 示例源码
```c
int main() { return 0; }
```
