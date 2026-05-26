#ifndef SPEAR_WASM_SPEAR_SSF_H
#define SPEAR_WASM_SPEAR_SSF_H

#include <stdint.h>
#include <stddef.h>

// SSF (Spear Stream Frame) v1 constants.
// SSF（Spear Stream Frame）v1 常量定义。
#define SPEAR_SSF_V1_MAGIC_SPST 0x54535053u
#define SPEAR_SSF_V1_VERSION 1u
#define SPEAR_SSF_V1_HEADER_LEN 32u

// SSF (Spear Stream Frame) v1 msg_type values.
// SSF（Spear Stream Frame）v1 的 msg_type 定义。
enum {
    // CTRL: control frames (e.g. OPEN/keepalive).
    // CTRL：控制类帧（例如 OPEN/keepalive）。
    SPEAR_SSF_MSG_TYPE_CTRL = 1,
    // DATA: data frames (text/binary payload).
    // DATA：数据帧（文本/二进制 payload）。
    SPEAR_SSF_MSG_TYPE_DATA = 2,
    // COMMIT: marks the end of a user input segment.
    // COMMIT：标记一次用户输入片段的结束。
    SPEAR_SSF_MSG_TYPE_COMMIT = 3,
};

static inline void sp_ssf_v1_write_u16_le(uint8_t *out, size_t off, uint16_t v) {
    out[off] = (uint8_t)(v & 0xffu);
    out[off + 1] = (uint8_t)((v >> 8) & 0xffu);
}

static inline void sp_ssf_v1_write_u32_le(uint8_t *out, size_t off, uint32_t v) {
    out[off] = (uint8_t)(v & 0xffu);
    out[off + 1] = (uint8_t)((v >> 8) & 0xffu);
    out[off + 2] = (uint8_t)((v >> 16) & 0xffu);
    out[off + 3] = (uint8_t)((v >> 24) & 0xffu);
}

static inline void sp_ssf_v1_write_u64_le(uint8_t *out, size_t off, uint64_t v) {
    sp_ssf_v1_write_u32_le(out, off, (uint32_t)(v & 0xffffffffu));
    sp_ssf_v1_write_u32_le(out, off + 4, (uint32_t)((v >> 32) & 0xffffffffu));
}

// Build SSF v1 header into `out` (must be at least 32 bytes).
// 构造 SSF v1 header 写入 `out`（至少 32 字节）。
static inline void sp_ssf_v1_build_header(uint8_t *out, uint32_t stream_id, uint16_t msg_type, uint16_t flags,
                                          uint64_t seq, uint32_t meta_len, uint32_t data_len) {
    out[0] = 'S';
    out[1] = 'P';
    out[2] = 'S';
    out[3] = 'T';
    sp_ssf_v1_write_u16_le(out, 4, (uint16_t)SPEAR_SSF_V1_VERSION);
    sp_ssf_v1_write_u16_le(out, 6, (uint16_t)SPEAR_SSF_V1_HEADER_LEN);
    sp_ssf_v1_write_u16_le(out, 8, msg_type);
    sp_ssf_v1_write_u16_le(out, 10, flags);
    sp_ssf_v1_write_u32_le(out, 12, stream_id);
    sp_ssf_v1_write_u64_le(out, 16, seq);
    sp_ssf_v1_write_u32_le(out, 24, meta_len);
    sp_ssf_v1_write_u32_le(out, 28, data_len);
}

#endif

