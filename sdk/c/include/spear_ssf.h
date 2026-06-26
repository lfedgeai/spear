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

// SSF v1 header view (no allocations).
// SSF v1 header 视图（零拷贝，不分配）。
typedef struct {
    uint16_t msg_type;
    uint16_t flags;
    uint32_t stream_id;
    uint64_t seq;
    uint32_t meta_len;
    uint32_t data_len;
    uint16_t header_len;
} sp_ssf_v1_header_t;

// SSF v1 frame view (no allocations).
// SSF v1 frame 视图（零拷贝，不分配）。
typedef struct {
    sp_ssf_v1_header_t hdr;
    const uint8_t *meta;
    const uint8_t *data;
} sp_ssf_v1_frame_view_t;

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

// Parse SSF v1 header from a frame buffer.
// 从 frame buffer 解析 SSF v1 header。
//
// Return value:
// - 1 on success / 成功返回 1
// - 0 on failure / 失败返回 0
static inline int sp_ssf_v1_parse_header(const uint8_t *frame, uint32_t frame_len, sp_ssf_v1_header_t *out) {
    if (!frame || !out || frame_len < (uint32_t)SPEAR_SSF_V1_HEADER_LEN) {
        return 0;
    }
    if (frame[0] != 'S' || frame[1] != 'P' || frame[2] != 'S' || frame[3] != 'T') {
        return 0;
    }
    uint16_t version = (uint16_t)frame[4] | ((uint16_t)frame[5] << 8);
    if (version != (uint16_t)SPEAR_SSF_V1_VERSION) {
        return 0;
    }
    uint16_t header_len = (uint16_t)frame[6] | ((uint16_t)frame[7] << 8);
    if (header_len < (uint16_t)SPEAR_SSF_V1_HEADER_LEN || frame_len < (uint32_t)header_len) {
        return 0;
    }
    uint16_t msg_type = (uint16_t)frame[8] | ((uint16_t)frame[9] << 8);
    uint16_t flags = (uint16_t)frame[10] | ((uint16_t)frame[11] << 8);
    uint32_t stream_id = (uint32_t)frame[12] | ((uint32_t)frame[13] << 8) | ((uint32_t)frame[14] << 16) |
                         ((uint32_t)frame[15] << 24);
    uint64_t seq = (uint64_t)frame[16] | ((uint64_t)frame[17] << 8) | ((uint64_t)frame[18] << 16) |
                   ((uint64_t)frame[19] << 24) | ((uint64_t)frame[20] << 32) | ((uint64_t)frame[21] << 40) |
                   ((uint64_t)frame[22] << 48) | ((uint64_t)frame[23] << 56);
    uint32_t meta_len = (uint32_t)frame[24] | ((uint32_t)frame[25] << 8) | ((uint32_t)frame[26] << 16) |
                        ((uint32_t)frame[27] << 24);
    uint32_t data_len = (uint32_t)frame[28] | ((uint32_t)frame[29] << 8) | ((uint32_t)frame[30] << 16) |
                        ((uint32_t)frame[31] << 24);
    uint32_t remain = frame_len - (uint32_t)header_len;
    if (meta_len + data_len != remain) {
        return 0;
    }
    out->msg_type = msg_type;
    out->flags = flags;
    out->stream_id = stream_id;
    out->seq = seq;
    out->meta_len = meta_len;
    out->data_len = data_len;
    out->header_len = header_len;
    return 1;
}

// Split an SSF v1 frame into (header, meta, data) without allocations.
// 零拷贝拆分 SSF v1 frame 为 (header, meta, data)。
static inline int sp_ssf_v1_split(const uint8_t *frame, uint32_t frame_len, sp_ssf_v1_frame_view_t *out) {
    if (!out) {
        return 0;
    }
    sp_ssf_v1_header_t hdr;
    if (!sp_ssf_v1_parse_header(frame, frame_len, &hdr)) {
        return 0;
    }
    uint32_t meta_off = (uint32_t)hdr.header_len;
    uint32_t data_off = meta_off + hdr.meta_len;
    if (data_off > frame_len) {
        return 0;
    }
    out->hdr = hdr;
    out->meta = frame + meta_off;
    out->data = frame + data_off;
    return 1;
}

// Check whether meta contains a substring (best-effort helper).
// 判断 meta 是否包含子串（best-effort 辅助函数）。
//
// Note: This is NOT a JSON parser. Prefer a real JSON parser when available.
// 注意：这不是 JSON 解析器。若可用，请优先使用真正的 JSON 解析器。
static inline int sp_ssf_meta_contains(const uint8_t *meta, uint32_t meta_len, const char *needle) {
    if (!meta || meta_len == 0 || !needle) {
        return 0;
    }
    size_t nlen = strlen(needle);
    if (nlen == 0 || nlen > (size_t)meta_len) {
        return 0;
    }
    const char *s = (const char *)meta;
    for (uint32_t i = 0; i + (uint32_t)nlen <= meta_len; i++) {
        if (memcmp(s + i, needle, nlen) == 0) {
            return 1;
        }
    }
    return 0;
}

#endif
