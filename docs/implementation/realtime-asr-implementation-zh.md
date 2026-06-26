# Realtime ASR（rtasr_fd）实现文档（面向落地）

本文是对 [realtime-asr-epoll-zh.md](../api/spear-hostcall/realtime-asr-epoll-zh.md) 的工程落地版，目标是在当前仓库中把 Realtime ASR 以 **fd + epoll** 的方式实现为一组 WASM hostcalls，并给出模块拆分、状态机、队列/背压、测试与验收标准。

## 0. 关联文档与代码入口

### 0.1 规范/设计文档

- 通用 fd/epoll 子系统：`docs/api/spear-hostcall/fd-epoll-subsystem-zh.md`
- Realtime ASR 设计（rtasr_fd）：`docs/api/spear-hostcall/realtime-asr-epoll-zh.md`
- streaming/realtime 子系统建议：`docs/backend-adapter/streaming-zh.md`

### 0.2 现有代码入口（已存在）

- fd/epoll 基础类型：`src/spearlet/execution/hostcall/types.rs`
- fd table（epoll watcher、fd_ctl、ep_wait 算法）：`src/spearlet/execution/hostcall/fd_table.rs`
- cchat（基于 fd + epoll 的先例）：`src/spearlet/execution/host_api.rs`
- WASM hostcalls glue（当前已有 cchat + spear_epoll_* + spear_fd_ctl）：`src/spearlet/execution/runtime/wasm_hostcalls.rs`
- legacy Go Realtime ASR（实现参考）：`legacy/spearlet/stream/rt_asr.go`
- Mic 采集实现文档（mic_fd）：`docs/implementation/mic-fd-implementation-zh.md`
- LLM 凭据与 credential_ref：`docs/implementation/llm-credentials-implementation-zh.md`

## 1. 总目标与边界

### 1.1 目标

- 新增 `rtasr_*` hostcalls：`create/ctl/read/write/close`。
- `rtasr_fd` 与 `cchat_fd` 一样进入通用 fd table，并可通过 `spear_epoll_*` 等待就绪。
- 统一错误码：新 family 全部返回 `-errno`（至少覆盖 `-EBADF/-EINVAL/-EFAULT/-EAGAIN/-ENOSPC/-EIO`）。
- 支持背压：发送队列满时 `rtasr_write` 返回 `-EAGAIN`，并在可写时发出 `EPOLLOUT`。
- 支持事件流：接收队列非空时发出 `EPOLLIN`，guest 用 `rtasr_read` 逐条读出。

### 1.2 非目标（本实现文档不要求）

- 不实现完整 WASI sockets/filesystem。
- 不把 guest 强绑定到某种语言/async runtime；guest 侧只依赖 syscall-like API。
- v1 不实现 edge-triggered/oneshot；epoll 固定为 level-triggered。

### 1.3 音频输入来源（必须澄清）

`rtasr_write` 的输入是“音频 bytes”，但这些 bytes 从哪里来是一个独立问题。

legacy Go 的 realtime ASR（见 `legacy/spearlet/stream/rt_asr.go`）并不是从 speARlet 机器上直接采集麦克风：它接收来自外部调用方的音频数据（`OperationTypeAppend` 的 `data`），再把数据 base64 编码后通过 WebSocket 发给上游 ASR。

在 WASM guest 的视角，我们需要支持两类常见场景：

- 外部输入：音频来自网络/文件/上游调用方，guest 只负责转发到 `rtasr_write`。
- 本机采集：音频来自 host 机器的麦克风，guest 需要一个“mic_fd”来读音频，再写入 `rtasr_fd`。

因此，Realtime ASR 的完整 end-to-end demo 需要额外的麦克风输入 hostcalls（`mic_*`），并让 `mic_fd` 也进入通用 fd/epoll 子系统。

## 2. ABI 与对外行为（实现必须对齐）

### 2.1 hostcalls（WASM import 名称）

本仓库已统一使用全称 `spear_epoll_*`。

- `rtasr_create() -> i32`
- `rtasr_ctl(fd: i32, cmd: i32, arg_ptr: i32, arg_len_ptr: i32) -> i32`
- `rtasr_write(fd: i32, buf_ptr: i32, buf_len: i32) -> i32`
- `rtasr_read(fd: i32, out_ptr: i32, out_len_ptr: i32) -> i32`
- `rtasr_close(fd: i32) -> i32`

麦克风输入（为了端到端 demo，建议新增）：

- `mic_create() -> i32`
- `mic_ctl(fd: i32, cmd: i32, arg_ptr: i32, arg_len_ptr: i32) -> i32`
- `mic_read(fd: i32, out_ptr: i32, out_len_ptr: i32) -> i32`
- `mic_close(fd: i32) -> i32`

epoll 相关：

- `spear_epoll_create() -> i32`
- `spear_epoll_ctl(epfd: i32, op: i32, fd: i32, events: i32) -> i32`
- `spear_epoll_wait(epfd: i32, out_ptr: i32, out_len_ptr: i32, timeout_ms: i32) -> i32`
- `spear_epoll_close(epfd: i32) -> i32`

### 2.2 事件与缓冲区约定

- `rtasr_read` 一次返回 **一条完整事件**（UTF-8 JSON bytes）。
- `rtasr_read` 输出 buffer 不足：写回需要长度到 `*out_len_ptr`，返回 `-ENOSPC`。
- `rtasr_write` 以 bytes 形式写入（推荐输入为 pcm16 chunk，具体由 `SET_PARAM` 决定 input format）。
- `rtasr_write` 发送队列满：返回 `-EAGAIN`。

### 2.3 readiness（必须可预测）

对 `rtasr_fd`：

- `EPOLLIN`：recv_queue 非空（`rtasr_read` 可读）
- `EPOLLOUT`：send_queue 未满（`rtasr_write` 可写）
- `EPOLLERR`：连接错误（建议配合 `rtasr_ctl(GET_STATUS)`）
- `EPOLLHUP`：对端关闭或本地 close

对 `mic_fd`：

- `EPOLLIN`：采集队列非空（`mic_read` 可读）
- `EPOLLERR`：采集错误
- `EPOLLHUP`：采集停止或 close

## 3. 代码落地设计（建议模块拆分）

### 3.1 最小侵入落地（v0/v1）

允许先把 rtasr 的 host state 放在 `host_api.rs` 内，形式与当前 `cchat_*` 类似。

需要新增：

- `FdKind::RtAsr`
- `FdInner::RtAsr(Box<RtAsrState>)`

其中 `RtAsrState` 负责：状态机、队列、统计、后台任务句柄、最后错误等。

### 3.2 推荐拆分（v1.1+）

将 rtasr 相关逻辑拆出到独立模块，避免 `host_api.rs` 继续膨胀：

- `src/spearlet/execution/stream/rtasr.rs`
  - `RtAsrState`、状态机、队列、背压
  - transport 抽象（stub/ws）
- `src/spearlet/execution/hostcall/memory.rs`
  - 线性内存读写工具（复用 `wasm_hostcalls.rs` 现有 mem_read/mem_write 代码）
- `src/spearlet/execution/hostcall/errno.rs`
  - errno 与 buffer-too-small 统一策略

## 4. RtAsrState：数据结构与状态机

### 4.1 状态机（必须实现）

建议枚举：

- `Init`
- `Configured`
- `Connecting`
- `Connected`
- `Draining`
- `Closed`
- `Error`

状态迁移规则以 [realtime-asr-epoll-zh.md](../api/spear-hostcall/realtime-asr-epoll-zh.md) 4.4 为准。

### 4.2 队列与背压（必须实现）

每个 `rtasr_fd` 维护两条队列：

- `send_queue: VecDeque<RtAsrSendItem>`
  - `Audio(Vec<u8>)`：原始音频 bytes（host 侧会包装为 `input_audio_buffer.append`）
  - `WsText(String)`：控制面 WS text event（例如 `input_audio_buffer.commit/clear`）
  - 以字节数统计 `send_queue_bytes`（对两种 item 都计入）
  - 上限 `max_send_queue_bytes`
- `recv_queue: VecDeque<Vec<u8>>`
  - 以字节数统计 `recv_queue_bytes`
  - 上限 `max_recv_queue_bytes`

溢出策略（建议 v1 固定）：

- send_queue：超过上限时 `rtasr_write -> -EAGAIN`
- recv_queue：超过上限时 `drop_oldest`，并累积 `dropped_events`

### 4.3 与 epoll 的对接（必须实现）

rtasr 的 read/write 以及后台任务在修改队列/状态后必须：

- 更新 fd entry 的 `poll_mask`（至少 IN/OUT/ERR/HUP）
- 调用 `fd_table.notify_watchers(fd)` 唤醒所有监听该 fd 的 epfd

建议实现一个内部函数：

- `recompute_readiness_and_notify(fd)`：根据队列/状态更新 poll_mask，并在变化时 notify

## 5. host_api.rs：内部 API（建议形态）

在 `DefaultHostApi` 中新增方法（与 cchat 风格一致）：

- `rtasr_create() -> i32`
- `rtasr_ctl(fd, cmd, payload_json) -> Result<Option<Vec<u8>>, i32>`
- `rtasr_write(fd, bytes) -> i32`
- `rtasr_read(fd) -> Result<Vec<u8>, i32>`
- `rtasr_close(fd) -> i32`

并通过 `FdTable` 分配/管理 fd entry：

- `FdKind::RtAsr`
- `FdInner::RtAsr(...)`

## 6. wasm_hostcalls.rs：ABI glue（需要新增）

参考 `cchat_*` 的实现方式，为 rtasr 增加：

- 输入 bytes：从 guest memory 读 `buf_ptr/buf_len`
- 输出 bytes：沿用 `out_ptr/out_len_ptr` 约定
- cmd/payload：`arg_ptr/arg_len_ptr` 指向 JSON（UTF-8）

建议把 `mem_read/mem_write/mem_write_with_len` 复用为公共 helper，避免重复代码。

## 7. transport：stub 先行、WebSocket 后补（分阶段）

### 7.1 Phase A：Stub transport（建议先落地）

目的：不新增依赖、不需要真实 key，即可验证 fd/epoll/背压/状态机的工程正确性。

做法：

- `RTASR_CTL_CONNECT` 后启动一个 tokio 任务：
  - 周期性向 `recv_queue` 写入模拟事件（delta/completed/error）
  - 可由 `SET_PARAM` 控制事件频率/大小

验收：

- guest 侧 epoll loop 能稳定收到 `EPOLLIN`
- `rtasr_read` 能读到 JSON 事件
- recv_queue 溢出策略可观测（dropped_events 增长）

### 7.2 Phase B：真实 WebSocket transport（OpenAI realtime transcription）

依赖建议：

- `tokio-tungstenite` + `url`（当前仓库尚未引入）

关键行为：

- 建连：根据 host 配置决定 base_url；当配置了 `credential_ref` 时附加鉴权（api_key）
- 发送：将音频 chunk base64 编码后发出 append 事件（参考 legacy：`legacy/spearlet/stream/rt_asr.go`）
- 接收：将 websocket 事件帧（text/binary）转换为 JSON bytes 写入 `recv_queue`

安全要求：

- key、client_secret、URL allowlist 全部在 host 管理；WASM guest 不能读到真实 secret
- 日志不得输出 secret 与原始认证头

## 8. 配置与路由（与现有 LLM 配置对齐）

仓库已有 `SpearletConfig.ai.backends`（见 `src/spearlet/config.rs`），包含 `ops/features/transports`。建议对 rtasr：

- `ops`：增加一个约定值（例如 `realtime_asr` 或 `realtime_transcription`）
- `transports`：使用 `websocket`

配置注意事项：

- `[[spearlet.ai.backends]] hosting` 为必填，只允许 `local` 或 `remote`。
- `credential_ref` 为可选：未配置时视为“无需鉴权”。

实现层面可选两条路：

1) rtasr 独立配置块（最直接、改动小）
2) 复用 router/registry：将 rtasr 作为一个 streaming operation 接入统一能力路由（长期推荐）

无论采用哪条路，都必须支持：

- 显式指定 backend（通过 `SET_PARAM.backend` 或 `RTASR_CTL_SET_PARAM` JSON）
- host 侧强制 allowlist/denylist

## 9. 控制面：rtasr_ctl 命令清单（v1 推荐）

推荐命令（以当前实现为准）：

- `RTASR_CTL_SET_PARAM = 1`：写入 key/value（JSON）
- `RTASR_CTL_CONNECT = 2`：建连/启动后台任务
- `RTASR_CTL_GET_STATUS = 3`：返回状态 JSON（含 state、last_error、queue_bytes、dropped 等）
- `RTASR_CTL_SEND_EVENT = 4`：发送 raw WS JSON text event（高级用法/逃生口）
- `RTASR_CTL_FLUSH = 5`：语义化 flush（触发一次“段落结束”；具体 backend 如何实现由 host 决定）
- `RTASR_CTL_CLEAR = 6`：语义化 clear（清空当前未完成的输入缓冲）
- `RTASR_CTL_SET_SEGMENTATION = 7`：设置分段策略（JSON）
- `RTASR_CTL_GET_SEGMENTATION = 8`：读取分段策略（JSON）

## 10. 测试与验收（必须做）

### 10.1 Rust 单测（纯内存）

- `rtasr_write`：
  - 队列未满写入成功
  - 队列满返回 `-EAGAIN`，并保持 `EPOLLOUT` 语义正确
- `rtasr_read`：
  - 队列为空返回 `-EAGAIN`
  - 队列非空读出一条并更新 `EPOLLIN`
- `rtasr_close`：
  - 幂等
  - 触发 `EPOLLHUP` 并唤醒 `spear_epoll_wait`

- `mic_read`（如果引入 mic_fd）：
  - 队列为空返回 `-EAGAIN`
  - 队列非空读出 bytes 并更新 `EPOLLIN`
  - close 触发 `EPOLLHUP` 并唤醒 `spear_epoll_wait`

### 10.2 链接测试（WAT import）

- 在 `src/spearlet/execution/runtime/wasm.rs` 添加 WAT：
  - import `rtasr_*`
  - import `spear_epoll_*` 与 `spear_fd_ctl`

### 10.3 E2E（推荐）

- stub 模式：默认跑，确保 CI 可稳定通过
- 真实 WS：在具备 key 的环境下运行（用环境变量注入），并将其标记为可选/ignored

## 11. 实施步骤（建议按阶段合并 PR）

### Phase 1：类型与 fd 接入

- 扩展 `FdKind/FdInner` 支持 `RtAsr`
- 落地 `RtAsrState`（状态机 + 队列 + readiness）
- 在 `host_api.rs` 增加 rtasr 的内部方法

验收：Rust 单测覆盖基本 read/write/close + epoll 唤醒。

### Phase 2：WASM hostcalls glue

- 在 `wasm_hostcalls.rs` 注册 `rtasr_*`
- 复用内存读写 helper

验收：WAT link test 通过；guest 能编译导入。

### Phase 3：Stub transport

- `CONNECT` 后启动 stub 事件产生任务

验收：epoll loop 可读到事件；背压与溢出策略可观测。

### Phase 4：真实 WebSocket transport

- 引入依赖并接入 WS
- 严格执行安全策略与日志脱敏

验收：在真实环境下可跑通 transcription delta/completed。

## 12. wasm-c（C 代码）参考用法

本节回答“将来 C 写的 WASM guest 应该如何调用 realtime ASR”。它展示一个典型的 **单线程事件循环**：用 `spear_epoll_*` 同时等待 `mic_fd` 与 `rtasr_fd` 的就绪事件，从而“同时推进采集麦克风 + 写音频 + 读 ASR 事件”。

注意：当前仓库的 `sdk/c/include/spear.h` 已包含 `sp_epoll_*` 与 `cchat_*`。当 `rtasr_*` 与 `mic_*` 落地后，建议同样在 `spear.h` 中新增 `sp_rtasr_*`、`sp_mic_*` 的 import 声明与 helper（风格可参考 `sp_cchat_recv_alloc`）。

### 12.1 示例：realtime_asr.c（伪代码级，可直接改成 sample）

```c
#include <spear.h>

#ifndef RTASR_MODEL
#define RTASR_MODEL "gpt-4o-mini-transcribe"
#endif

enum {
    RTASR_CTL_SET_PARAM = 1,
    RTASR_CTL_CONNECT = 2,
    RTASR_CTL_GET_STATUS = 3,
    RTASR_CTL_SEND_EVENT = 4,
    RTASR_CTL_FLUSH = 5,
    RTASR_CTL_CLEAR = 6,
    RTASR_CTL_SET_SEGMENTATION = 7,
    RTASR_CTL_GET_SEGMENTATION = 8,
};

enum {
    MIC_CTL_SET_PARAM = 1,
};

static inline int32_t sp_rtasr_set_param_string(int32_t fd, const char *key,
                                                const char *value) {
    char buf[512];
    int n = snprintf(buf, sizeof(buf), "{\"key\":\"%s\",\"value\":\"%s\"}", key, value);
    if (n <= 0 || (size_t)n >= sizeof(buf)) {
        return -EIO;
    }
    uint32_t len = (uint32_t)n;
    return sp_rtasr_ctl(fd, RTASR_CTL_SET_PARAM, (int32_t)(uintptr_t)buf,
                        (int32_t)(uintptr_t)&len);
}

static inline int32_t sp_rtasr_set_segmentation_json(int32_t fd, const char *json, uint32_t json_len) {
    uint32_t len = json_len;
    return sp_rtasr_ctl(fd, RTASR_CTL_SET_SEGMENTATION, (int32_t)(uintptr_t)json,
                        (int32_t)(uintptr_t)&len);
}

static inline int32_t sp_mic_set_param_json(int32_t fd, const char *json, uint32_t json_len) {
    uint32_t len = json_len;
    return sp_mic_ctl(fd, MIC_CTL_SET_PARAM, (int32_t)(uintptr_t)json,
                      (int32_t)(uintptr_t)&len);
}

static inline uint8_t *sp_rtasr_read_alloc(int32_t fd, uint32_t *out_len) {
    uint32_t cap = 64 * 1024;
    uint8_t *buf = (uint8_t *)malloc(cap + 1);
    if (!buf) {
        return NULL;
    }
    for (int attempt = 0; attempt < 3; attempt++) {
        uint32_t len = cap;
        int32_t rc = sp_rtasr_read(fd, (int32_t)(uintptr_t)buf, (int32_t)(uintptr_t)&len);
        if (rc >= 0) {
            buf[len] = 0;
            *out_len = len;
            return buf;
        }
        if (rc != -ENOSPC) {
            free(buf);
            return NULL;
        }
        cap = len;
        uint8_t *b2 = (uint8_t *)realloc(buf, cap + 1);
        if (!b2) {
            free(buf);
            return NULL;
        }
        buf = b2;
    }
    free(buf);
    return NULL;
}

int main() {
    int32_t epfd = sp_epoll_create();
    if (epfd < 0) {
        printf("epoll_create failed: %d\n", epfd);
        return 1;
    }

    int32_t mic_fd = sp_mic_create();
    if (mic_fd < 0) {
        printf("mic_create failed: %d\n", mic_fd);
        sp_epoll_close(epfd);
        return 1;
    }

    const char *mic_cfg = "{\"sample_rate_hz\":24000,\"channels\":1,\"format\":\"pcm16\",\"frame_ms\":20}";
    int32_t rc = sp_mic_set_param_json(mic_fd, mic_cfg, (uint32_t)strlen(mic_cfg));
    if (rc != 0) {
        printf("mic set param failed: %d\n", rc);
        sp_mic_close(mic_fd);
        sp_epoll_close(epfd);
        return 1;
    }

    int32_t asr_fd = sp_rtasr_create();
    if (asr_fd < 0) {
        printf("rtasr_create failed: %d\n", asr_fd);
        sp_mic_close(mic_fd);
        sp_epoll_close(epfd);
        return 1;
    }

    rc = sp_rtasr_set_param_string(asr_fd, "transport", "websocket");
    if (rc != 0) {
        printf("rtasr set transport failed: %d\n", rc);
        sp_rtasr_close(asr_fd);
        sp_mic_close(mic_fd);
        sp_epoll_close(epfd);
        return 1;
    }

    // backend 名称来自 spearlet_config 的 ai.backends（例如 openai-realtime）。
    rc = sp_rtasr_set_param_string(asr_fd, "backend", "openai-realtime");
    if (rc != 0) {
        printf("rtasr set backend failed: %d\n", rc);
        sp_rtasr_close(asr_fd);
        sp_mic_close(mic_fd);
        sp_epoll_close(epfd);
        return 1;
    }

    rc = sp_rtasr_set_param_string(asr_fd, "model", RTASR_MODEL);
    if (rc != 0) {
        printf("rtasr set model failed: %d\n", rc);
        sp_rtasr_close(asr_fd);
        sp_mic_close(mic_fd);
        sp_epoll_close(epfd);
        return 1;
    }

    rc = sp_rtasr_set_param_string(asr_fd, "input_audio_format", "pcm16");
    if (rc != 0) {
        printf("rtasr set input_audio_format failed: %d\n", rc);
        sp_rtasr_close(asr_fd);
        sp_mic_close(mic_fd);
        sp_epoll_close(epfd);
        return 1;
    }

    // 默认分段策略是 server VAD（无需设置即可自动断句）。
    // 如需调整 VAD 行为，可显式设置分段策略：
    const char *seg_cfg = "{\"strategy\":\"server_vad\",\"vad\":{\"silence_ms\":300}}";
    rc = sp_rtasr_set_segmentation_json(asr_fd, seg_cfg, (uint32_t)strlen(seg_cfg));
    if (rc != 0) {
        printf("rtasr set segmentation failed: %d\n", rc);
        sp_rtasr_close(asr_fd);
        sp_mic_close(mic_fd);
        sp_epoll_close(epfd);
        return 1;
    }

    uint32_t empty = 0;
    rc = sp_rtasr_ctl(asr_fd, RTASR_CTL_CONNECT, 0, (int32_t)(uintptr_t)&empty);
    if (rc != 0) {
        printf("rtasr connect failed: %d\n", rc);
        sp_rtasr_close(asr_fd);
        sp_mic_close(mic_fd);
        sp_epoll_close(epfd);
        return 1;
    }

    rc = sp_epoll_ctl(epfd, SPEAR_EPOLL_CTL_ADD, mic_fd,
                      SPEAR_EPOLLIN | SPEAR_EPOLLERR | SPEAR_EPOLLHUP);
    if (rc != 0) {
        printf("epoll_ctl add mic_fd failed: %d\n", rc);
        sp_rtasr_close(asr_fd);
        sp_mic_close(mic_fd);
        sp_epoll_close(epfd);
        return 1;
    }

    rc = sp_epoll_ctl(epfd, SPEAR_EPOLL_CTL_ADD, asr_fd,
                      SPEAR_EPOLLIN | SPEAR_EPOLLOUT | SPEAR_EPOLLERR | SPEAR_EPOLLHUP);
    if (rc != 0) {
        printf("epoll_ctl add failed: %d\n", rc);
        sp_rtasr_close(asr_fd);
        sp_mic_close(mic_fd);
        sp_epoll_close(epfd);
        return 1;
    }

    uint8_t evbuf[8 * 16];
    uint32_t evlen = sizeof(evbuf);

    uint8_t pending[3200];
    int pending_len = 0;

    for (;;) {
        evlen = sizeof(evbuf);
        int32_t n = sp_epoll_wait(epfd, (int32_t)(uintptr_t)evbuf, (int32_t)(uintptr_t)&evlen, 50);
        if (n < 0) {
            continue;
        }
        for (int i = 0; i < n; i++) {
            int32_t fd = *(int32_t *)(evbuf + i * 8 + 0);
            int32_t revents = *(int32_t *)(evbuf + i * 8 + 4);

            if (fd == mic_fd && (revents & SPEAR_EPOLLIN)) {
                uint8_t pcm_chunk[3200];
                uint32_t cap = sizeof(pcm_chunk);
                int32_t rr = sp_mic_read(mic_fd, (int32_t)(uintptr_t)pcm_chunk,
                                         (int32_t)(uintptr_t)&cap);
                if (rr > 0) {
                    if (pending_len == 0) {
                        int32_t w = sp_rtasr_write(asr_fd, (int32_t)(uintptr_t)pcm_chunk, rr);
                        if (w == -EAGAIN) {
                            memcpy(pending, pcm_chunk, (size_t)rr);
                            pending_len = rr;
                        } else if (w < 0) {
                            printf("rtasr_write failed: %d\n", w);
                        }
                    }
                }
            }

            if (fd == asr_fd && (revents & SPEAR_EPOLLIN)) {
                for (;;) {
                    uint32_t out_len = 0;
                    uint8_t *msg = sp_rtasr_read_alloc(asr_fd, &out_len);
                    if (!msg) {
                        break;
                    }
                    printf("asr_event_len=%u\n", out_len);
                    printf("asr_event_json=%s\n", (char *)msg);
                    free(msg);
                }
            }

            if (fd == asr_fd && (revents & SPEAR_EPOLLOUT)) {
                if (pending_len > 0) {
                    int32_t w = sp_rtasr_write(asr_fd, (int32_t)(uintptr_t)pending, pending_len);
                    if (w >= 0) {
                        pending_len = 0;
                    } else if (w < 0 && w != -EAGAIN) {
                        printf("rtasr_write failed: %d\n", w);
                    }
                }
            }

            if (revents & (SPEAR_EPOLLERR | SPEAR_EPOLLHUP)) {
                goto done;
            }
        }
    }

done:
    sp_rtasr_close(asr_fd);
    sp_mic_close(mic_fd);
    sp_epoll_close(epfd);
    return 0;
}
```

### 12.2 关键点说明

- `sp_epoll_wait` 返回的 events buffer 是紧凑数组，每条 8 bytes：`fd(i32)` + `events(i32)`。
- `mic_read` 建议读到 `-EAGAIN` 为止；若音频生产速率高于网络发送速率，必须在 guest 或 host 侧做缓冲与背压。
- `rtasr_read` 建议一次读一条完整事件，guest 侧用循环 drain 到 `-EAGAIN`（或直到本次无数据）。
- `rtasr_write` 建议在 `EPOLLOUT` 时写入；若返回 `-EAGAIN`，guest 侧缓存音频并等待下一次 `EPOLLOUT`。
