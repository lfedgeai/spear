#include <spear.h>
// Voice input over user stream sample (WASM-C).
// 基于 user stream 的语音输入示例（WASM-C）。
//
//
// This sample implements a minimal voice pipeline:
// - stream_id=1: text chat in/out (Console chat window)
// - stream_id=2: voice input uplink (press-and-hold)
//
// It enforces "CTRL(OPEN) first" (no backward compatibility).
// For voice:
// - CTRL(kind=utterance_begin) -> Recording
// - DATA(pcm16le) -> rtasr_write
// - COMMIT -> rtasr_flush, wait for transcription, then call cchat and write output to stream_id=1
//
// 本示例实现最小语音链路：
// - stream_id=1：文本对话输入/输出（Console 聊天窗口）
// - stream_id=2：语音输入 uplink（按住说话）
//
// 强制要求每个 stream 必须先发送 CTRL(OPEN)（不做兼容）。
// 对语音：
// - CTRL(kind=utterance_begin) -> Recording
// - DATA(pcm16le) -> rtasr_write
// - COMMIT -> rtasr_flush，等待转写结果，然后调用 cchat 并写回 stream_id=1
//

enum {
    STREAM_TEXT = 1,
    STREAM_VOICE = 2,
};

typedef struct {
    uint32_t stream_id;
    int32_t fd;
    uint64_t out_seq;
    int opened;
} sp_stream_t;

typedef enum {
    VOICE_IDLE = 0,
    VOICE_RECORDING = 1,
    VOICE_COMMITTING = 2,
} sp_voice_state_t;

// Reset local bookkeeping for one logical user stream.
static void sp_stream_init(sp_stream_t *s, uint32_t stream_id) {
    memset(s, 0, sizeof(*s));
    s->stream_id = stream_id;
    s->fd = 0;
    s->out_seq = 1;
    s->opened = 0;
}

static void sp_stream_close(sp_stream_t *s) {
    if (!s) {
        return;
    }
    if (s->fd > 0) {
        (void)sp_user_stream_close(s->fd);
    }
    s->fd = 0;
    s->opened = 0;
    s->out_seq = 1;
}

// Build one SSF DATA frame and send a text chunk to stream_id=1.
static int sp_stream_send_text(sp_stream_t *s, const char *text, const char *meta_json) {
    if (!s || s->fd <= 0 || !text) {
        return 0;
    }
    if (!s->opened) {
        return 0;
    }
    const char *meta = meta_json ? meta_json : "{}";
    uint32_t meta_len = (uint32_t)strlen(meta);
    uint32_t data_len = (uint32_t)strlen(text);
    uint32_t total = SPEAR_SSF_V1_HEADER_LEN + meta_len + data_len;
    uint8_t *buf = (uint8_t *)malloc(total);
    if (!buf) {
        return 0;
    }
    sp_ssf_v1_build_header(buf, s->stream_id, SPEAR_SSF_MSG_TYPE_DATA, 0, s->out_seq++, meta_len, data_len);
    memcpy(buf + SPEAR_SSF_V1_HEADER_LEN, meta, meta_len);
    memcpy(buf + SPEAR_SSF_V1_HEADER_LEN + meta_len, text, data_len);
    int32_t rc = sp_user_stream_write(s->fd, (int32_t)(uintptr_t)buf, (int32_t)total);
    free(buf);
    return rc == 0;
}

// Emit COMMIT to mark the end of the current text response message.
static int sp_stream_send_commit(sp_stream_t *s, const char *meta_json) {
    if (!s || s->fd <= 0) {
        return 0;
    }
    if (!s->opened) {
        return 0;
    }
    const char *meta = meta_json ? meta_json : "{}";
    uint32_t meta_len = (uint32_t)strlen(meta);
    uint32_t data_len = 0;
    uint32_t total = SPEAR_SSF_V1_HEADER_LEN + meta_len + data_len;
    uint8_t *buf = (uint8_t *)malloc(total);
    if (!buf) {
        return 0;
    }
    sp_ssf_v1_build_header(buf, s->stream_id, SPEAR_SSF_MSG_TYPE_COMMIT, 0, s->out_seq++, meta_len, data_len);
    memcpy(buf + SPEAR_SSF_V1_HEADER_LEN, meta, meta_len);
    int32_t rc = sp_user_stream_write(s->fd, (int32_t)(uintptr_t)buf, (int32_t)total);
    free(buf);
    return rc == 0;
}

static int sp_rtasr_clear(int32_t fd) {
    uint32_t len = 0;
    return sp_rtasr_ctl(fd, SPEAR_RTA_CTL_CLEAR, 0, (int32_t)(uintptr_t)&len);
}

static void maybe_emit_protocol_warning(sp_stream_t *text_stream, int *has_pending, char *buf, size_t buf_len) {
    if (!has_pending || !buf) {
        return;
    }
    if (!*has_pending) {
        return;
    }
    if (text_stream && text_stream->opened) {
        (void)sp_stream_send_text(text_stream, buf, "{\"v\":1}");
        *has_pending = 0;
        if (buf_len > 0) {
            buf[0] = 0;
        }
    }
}

static void set_protocol_warning_once(int *has_pending, char *buf, size_t buf_len, const char *msg) {
    if (!has_pending || !buf || buf_len == 0 || !msg) {
        return;
    }
    if (*has_pending) {
        return;
    }
    snprintf(buf, buf_len, "%s", msg);
    *has_pending = 1;
}

static int extract_text_field(const char *json, char *out, size_t out_len) {
    if (!json || !out || out_len == 0) {
        return 0;
    }
    const char *p = strstr(json, "\"text\"");
    if (!p) {
        return 0;
    }
    p = strchr(p, ':');
    if (!p) {
        return 0;
    }
    p++;
    while (*p == ' ' || *p == '\t') {
        p++;
    }
    if (*p != '"') {
        return 0;
    }
    p++;
    const char *end = strchr(p, '"');
    if (!end) {
        return 0;
    }
    size_t n = (size_t)(end - p);
    if (n + 1 > out_len) {
        n = out_len - 1;
    }
    memcpy(out, p, n);
    out[n] = '\0';
    return 1;
}

static int extract_string_field(const char *json, const char *key, char *out, size_t out_len) {
    if (!json || !key || !out || out_len == 0) {
        return 0;
    }
    char needle[64];
    int n = snprintf(needle, sizeof(needle), "\"%s\"", key);
    if (n <= 0 || (size_t)n >= sizeof(needle)) {
        return 0;
    }
    const char *p = strstr(json, needle);
    if (!p) {
        return 0;
    }
    p = strchr(p, ':');
    if (!p) {
        return 0;
    }
    p++;
    while (*p == ' ' || *p == '\t') {
        p++;
    }
    if (*p != '"') {
        return 0;
    }
    p++;
    const char *end = strchr(p, '"');
    if (!end) {
        return 0;
    }
    size_t len = (size_t)(end - p);
    if (len + 1 > out_len) {
        len = out_len - 1;
    }
    memcpy(out, p, len);
    out[len] = '\0';
    return 1;
}

static int is_delta_event(const char *json) {
    if (!json) {
        return 0;
    }
    if (strstr(json, "input_audio_transcription.delta")) {
        return 1;
    }
    if (strstr(json, "conversation.item.input_audio_transcription.delta")) {
        return 1;
    }
    if (strstr(json, "output_audio_transcript.delta")) {
        return 1;
    }
    return 0;
}

static int is_completed_event(const char *json) {
    if (!json) {
        return 0;
    }
    if (strstr(json, "input_audio_transcription.completed")) {
        return 1;
    }
    if (strstr(json, "conversation.item.input_audio_transcription.completed")) {
        return 1;
    }
    if (strstr(json, "output_audio_transcript.done")) {
        return 1;
    }
    return 0;
}

static void build_short_timestamp_prefix(char *out, size_t out_len) {
    if (!out || out_len == 0) {
        return;
    }
    int64_t now_ms = sp_time_now_ms();
    if (now_ms < 0) {
        now_ms = 0;
    }
    int64_t total_seconds = now_ms / 1000;
    int hh = (int)((total_seconds / 3600) % 24);
    int mm = (int)((total_seconds / 60) % 60);
    int ss = (int)(total_seconds % 60);
    (void)snprintf(out, out_len, "[%02d:%02d:%02d] ", hh, mm, ss);
}

static int send_chat_completion(sp_stream_t *text_stream, const char *prompt) {
    if (!text_stream || text_stream->fd <= 0 || !prompt) {
        return 0;
    }

    // cchat is used as the downstream text model after voice has been transcribed.
    int32_t fd = sp_cchat_create();
    if (fd < 0) {
        sp_stream_send_text(text_stream, "cchat_create failed\n", "{\"v\":1}");
        return 0;
    }

    int32_t rc = sp_cchat_write_msg_str(fd, "user", prompt);
    if (rc != 0) {
        sp_cchat_close(fd);
        sp_stream_send_text(text_stream, "cchat_write_msg failed\n", "{\"v\":1}");
        return 0;
    }

    rc = sp_cchat_set_param_u32(fd, "timeout_ms", 30000);
    if (rc != 0) {
        sp_cchat_close(fd);
        sp_stream_send_text(text_stream, "set timeout failed\n", "{\"v\":1}");
        return 0;
    }

    int32_t resp_fd = sp_cchat_send(fd, 0);
    if (resp_fd < 0) {
        sp_cchat_close(fd);
        sp_stream_send_text(text_stream, "cchat_send failed\n", "{\"v\":1}");
        return 0;
    }

    uint32_t resp_len = 0;
    uint8_t *resp = sp_cchat_recv_alloc(resp_fd, &resp_len);
    if (!resp) {
        sp_cchat_close(resp_fd);
        sp_cchat_close(fd);
        sp_stream_send_text(text_stream, "cchat_recv failed\n", "{\"v\":1}");
        return 0;
    }

    sp_stream_send_text(text_stream, (const char *)resp, "{\"v\":1}");
    sp_stream_send_text(text_stream, "\n", "{\"v\":1}");

    free(resp);
    sp_cchat_close(resp_fd);
    sp_cchat_close(fd);
    return 1;
}

int main() {
    int32_t epfd = sp_ep_create();
    if (epfd < 0) {
        printf("ep_create failed: %d\n", epfd);
        return 1;
    }

    int32_t ctl_fd = sp_user_stream_ctl_open();
    if (ctl_fd < 0) {
        printf("user_stream_ctl_open failed: %d\n", ctl_fd);
        sp_ep_close(epfd);
        return 1;
    }

    int32_t rc = sp_ep_ctl(epfd, SPEAR_EP_CTL_ADD, ctl_fd, SPEAR_EPOLLIN | SPEAR_EPOLLERR | SPEAR_EPOLLHUP);
    if (rc != 0) {
        printf("ep_ctl add ctl_fd failed: %d\n", rc);
        sp_user_stream_close(ctl_fd);
        sp_ep_close(epfd);
        return 1;
    }

    sp_stream_t text_stream;
    sp_stream_t voice_stream;
    sp_stream_init(&text_stream, STREAM_TEXT);
    sp_stream_init(&voice_stream, STREAM_VOICE);

    sp_voice_state_t voice_state = VOICE_IDLE;
    int32_t rtasr_fd = 0;
    char last_text[4096] = {0};
    uint32_t voice_bytes = 0;
    int has_pending_warn = 0;
    char pending_warn[256] = {0};

    uint8_t ready_buf[8 * 64];
    printf("user_stream_voice_chat started\n");

    while (1) {
        uint32_t ready_len = sizeof(ready_buf);
        int32_t nready = sp_ep_wait(epfd, (int32_t)(uintptr_t)ready_buf, (int32_t)(uintptr_t)&ready_len, 2000);
        if (nready < 0) {
            printf("ep_wait failed: %d\n", nready);
            break;
        }

        maybe_emit_protocol_warning(&text_stream, &has_pending_warn, pending_warn, sizeof(pending_warn));
        // The loop multiplexes control events, user-stream SSF frames, and RTASR callbacks
        // so the sample can stay single-threaded.
        for (int i = 0; i < nready; i++) {
            int32_t fd = 0;
            int32_t ev = 0;
            memcpy(&fd, ready_buf + (i * 8), 4);
            memcpy(&ev, ready_buf + (i * 8) + 4, 4);

            if (ev & SPEAR_EPOLLHUP) {
                if (fd == ctl_fd) {
                    printf("ctl hup\n");
                    goto out;
                }
                if (fd == text_stream.fd) {
                    sp_stream_close(&text_stream);
                }
                if (fd == voice_stream.fd) {
                    sp_stream_close(&voice_stream);
                }
                if (fd == rtasr_fd) {
                    (void)sp_rtasr_close(rtasr_fd);
                    rtasr_fd = 0;
                }
                continue;
            }

            if (ev & SPEAR_EPOLLERR) {
                if (fd == ctl_fd) {
                    printf("ctl err\n");
                    goto out;
                }
                continue;
            }

            if (fd == ctl_fd && (ev & SPEAR_EPOLLIN)) {
                while (1) {
                    sp_user_stream_ctl_event_t evt;
                    int32_t rr = sp_user_stream_ctl_read_event(ctl_fd, &evt);
                    if (rr == -SPEAR_EAGAIN) {
                        break;
                    }
                    if (rr < 0) {
                        printf("user_stream_ctl_read failed: %d\n", rr);
                        goto out;
                    }
                    if (evt.kind == SPEAR_USER_STREAM_CTL_EVENT_SESSION_CLOSED) {
                        printf("session closed\n");
                        goto out;
                    }
                    if (evt.kind != SPEAR_USER_STREAM_CTL_EVENT_STREAM_CONNECTED) {
                        continue;
                    }

                    if (evt.stream_id != STREAM_TEXT && evt.stream_id != STREAM_VOICE) {
                        continue;
                    }

                    // When the runtime announces a connectable logical stream, open its data fd
                    // and start polling it like a normal endpoint.
                    sp_stream_t *s = evt.stream_id == STREAM_TEXT ? &text_stream : &voice_stream;
                    if (s->fd > 0) {
                        continue;
                    }
                    int32_t sfd = sp_user_stream_open((int32_t)evt.stream_id, SPEAR_USER_STREAM_DIR_BIDIRECTIONAL);
                    if (sfd < 0) {
                        printf("user_stream_open failed: %d\n", sfd);
                        continue;
                    }
                    s->fd = sfd;
                    s->out_seq = 1;
                    s->opened = 0;
                    int32_t add_rc = sp_ep_ctl(epfd, SPEAR_EP_CTL_ADD, sfd,
                                              SPEAR_EPOLLIN | SPEAR_EPOLLOUT | SPEAR_EPOLLERR | SPEAR_EPOLLHUP);
                    if (add_rc != 0) {
                        printf("ep_ctl add stream fd failed: %d\n", add_rc);
                        sp_stream_close(s);
                        continue;
                    }
                }
                continue;
            }

            if ((ev & SPEAR_EPOLLIN) == 0) {
                continue;
            }

            if (fd == rtasr_fd) {
                while (1) {
                    uint32_t out_len = 0;
                    uint8_t *evt_bytes = sp_rtasr_read_alloc(rtasr_fd, &out_len);
                    if (!evt_bytes) {
                        break;
                    }
                    if (out_len > 0) {
                        evt_bytes[out_len] = 0;
                        const char *json = (const char *)evt_bytes;
                        // RTASR emits incremental deltas first, then one completed event.
                        // We keep the latest transcript in last_text and only ask cchat after
                        // the utterance has been committed and completed.
                        if (is_delta_event(json)) {
                            char tmp[512] = {0};
                            if (extract_string_field(json, "delta", tmp, sizeof(tmp))) {
                                size_t cur = strlen(last_text);
                                size_t cap = sizeof(last_text) - 1;
                                if (cur < cap) {
                                    strncat(last_text, tmp, cap - cur);
                                }
                            }
                        } else if (is_completed_event(json)) {
                            char tmp[4096] = {0};
                            if (extract_string_field(json, "transcript", tmp, sizeof(tmp))) {
                                strncpy(last_text, tmp, sizeof(last_text) - 1);
                                last_text[sizeof(last_text) - 1] = 0;
                            } else if (extract_text_field(json, tmp, sizeof(tmp))) {
                                strncpy(last_text, tmp, sizeof(last_text) - 1);
                                last_text[sizeof(last_text) - 1] = 0;
                            }
                            if (voice_state == VOICE_COMMITTING && last_text[0]) {
                                char prefix[16] = {0};
                                build_short_timestamp_prefix(prefix, sizeof(prefix));
                                sp_stream_send_text(&text_stream, prefix, "{\"v\":1}");
                                sp_stream_send_text(&text_stream, last_text, "{\"v\":1}");
                                sp_stream_send_text(&text_stream, "\n", "{\"v\":1}");
                                send_chat_completion(&text_stream, last_text);
                                voice_state = VOICE_IDLE;
                                last_text[0] = 0;
                            }
                        }
                    }
                    free(evt_bytes);
                }
                continue;
            }

            sp_stream_t *s = NULL;
            if (fd == text_stream.fd) {
                s = &text_stream;
            } else if (fd == voice_stream.fd) {
                s = &voice_stream;
            } else {
                continue;
            }

            uint32_t frame_len = 0;
            uint8_t *frame = sp_user_stream_read_alloc(s->fd, &frame_len);
            if (!frame) {
                continue;
            }

            sp_ssf_v1_frame_view_t v;
            if (!sp_ssf_v1_split(frame, frame_len, &v)) {
                free(frame);
                continue;
            }

            if (!s->opened) {
                int ok = 0;
                if (v.hdr.msg_type == SPEAR_SSF_MSG_TYPE_CTRL &&
                    sp_ssf_meta_contains(v.meta, v.hdr.meta_len, "\"kind\":\"open\"")) {
                    if (s->stream_id == STREAM_TEXT) {
                        ok = sp_ssf_meta_contains(v.meta, v.hdr.meta_len, "\"modality\":\"text\"");
                    } else {
                        ok = sp_ssf_meta_contains(v.meta, v.hdr.meta_len, "\"modality\":\"audio\"");
                    }
                }
                // This sample requires an explicit CTRL(open) handshake before accepting any
                // data frames on the stream.
                if (ok) {
                    s->opened = 1;
                    free(frame);
                    maybe_emit_protocol_warning(&text_stream, &has_pending_warn, pending_warn, sizeof(pending_warn));
                    continue;
                }
                set_protocol_warning_once(&has_pending_warn, pending_warn, sizeof(pending_warn),
                                         "protocol: stream opened without CTRL(open), dropping frames until open\n");
                free(frame);
                continue;
            }

            if (s->stream_id == STREAM_VOICE) {
                if (v.hdr.msg_type == SPEAR_SSF_MSG_TYPE_CTRL) {
                    if (sp_ssf_meta_contains(v.meta, v.hdr.meta_len, "\"kind\":\"utterance_begin\"")) {
                        // A new utterance resets local transcript state and prepares RTASR for
                        // the next chunked audio upload.
                        voice_state = VOICE_RECORDING;
                        last_text[0] = 0;
                        voice_bytes = 0;

                        if (rtasr_fd <= 0) {
                            rtasr_fd = sp_rtasr_create();
                            if (rtasr_fd > 0) {
                                (void)sp_rtasr_set_param_string(rtasr_fd, "transport", "websocket");
                                int32_t c_rc = sp_rtasr_connect(rtasr_fd);
                                if (c_rc != 0) {
                                    sp_stream_send_text(&text_stream, "rtasr_connect failed: no speech_to_text websocket backend\n", "{\"v\":1}");
                                    (void)sp_rtasr_close(rtasr_fd);
                                    rtasr_fd = 0;
                                    voice_state = VOICE_IDLE;
                                    free(frame);
                                    continue;
                                }
                                (void)sp_ep_ctl(epfd, SPEAR_EP_CTL_ADD, rtasr_fd,
                                                SPEAR_EPOLLIN | SPEAR_EPOLLOUT | SPEAR_EPOLLERR | SPEAR_EPOLLHUP);
                            }
                        } else {
                            (void)sp_rtasr_clear(rtasr_fd);
                        }
                    }
                } else if (v.hdr.msg_type == SPEAR_SSF_MSG_TYPE_DATA && voice_state == VOICE_RECORDING) {
                    if ((!sp_ssf_meta_contains(v.meta, v.hdr.meta_len, "\"v\":1,") &&
                         !sp_ssf_meta_contains(v.meta, v.hdr.meta_len, "\"v\":1}")) ||
                        sp_ssf_meta_contains(v.meta, v.hdr.meta_len, "\"kind\"") ||
                        sp_ssf_meta_contains(v.meta, v.hdr.meta_len, "\"modality\"") ||
                        sp_ssf_meta_contains(v.meta, v.hdr.meta_len, "\"audio\"")) {
                        set_protocol_warning_once(&has_pending_warn, pending_warn, sizeof(pending_warn),
                                                 "protocol: voice DATA meta invalid, dropping frame\n");
                        free(frame);
                        continue;
                    }
                    // Voice DATA is forwarded as-is to RTASR. The sample only counts bytes so
                    // COMMIT can ignore extremely short / accidental pushes.
                    if (rtasr_fd > 0 && v.data && v.hdr.data_len > 0) {
                        voice_bytes += (uint32_t)v.hdr.data_len;
                        (void)sp_rtasr_write(rtasr_fd, (int32_t)(uintptr_t)v.data, (int32_t)v.hdr.data_len);
                    }
                } else if (v.hdr.msg_type == SPEAR_SSF_MSG_TYPE_COMMIT && voice_state == VOICE_RECORDING) {
                    if ((!sp_ssf_meta_contains(v.meta, v.hdr.meta_len, "\"v\":1,") &&
                         !sp_ssf_meta_contains(v.meta, v.hdr.meta_len, "\"v\":1}")) ||
                        sp_ssf_meta_contains(v.meta, v.hdr.meta_len, "\"kind\"") ||
                        sp_ssf_meta_contains(v.meta, v.hdr.meta_len, "\"modality\"") ||
                        sp_ssf_meta_contains(v.meta, v.hdr.meta_len, "\"audio\"")) {
                        set_protocol_warning_once(&has_pending_warn, pending_warn, sizeof(pending_warn),
                                                 "protocol: voice COMMIT meta invalid, dropping frame\n");
                        free(frame);
                        continue;
                    }
                    // COMMIT flushes the utterance into RTASR only after we have buffered a
                    // minimum amount of audio, otherwise the sample just returns to idle.
                    if (rtasr_fd > 0) {
                        if (voice_bytes >= 4800) {
                            (void)sp_rtasr_flush(rtasr_fd);
                            voice_state = VOICE_COMMITTING;
                        } else {
                            voice_state = VOICE_IDLE;
                        }
                    } else {
                        voice_state = VOICE_IDLE;
                    }
                }
            }

            free(frame);
        }
    }

out:
    if (rtasr_fd > 0) {
        (void)sp_rtasr_close(rtasr_fd);
        rtasr_fd = 0;
    }
    sp_stream_close(&text_stream);
    sp_stream_close(&voice_stream);
    (void)sp_user_stream_close(ctl_fd);
    (void)sp_ep_close(epfd);
    return 0;
}
