#include <spear.h>

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

// Build one SSF DATA frame and write it to an already-open user stream.
static int sp_stream_send_text(sp_stream_t *s, const char *text, const char *meta_json) {
    if (!s || s->fd <= 0 || !text) {
        return 0;
    }
    if (!s->opened) {
        return 0;
    }
    const char *meta = meta_json ? meta_json : "{\"v\":1}";
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

// COMMIT terminates the current logical text message on the target stream.
static int sp_stream_send_commit(sp_stream_t *s, const char *meta_json) {
    if (!s || s->fd <= 0) {
        return 0;
    }
    if (!s->opened) {
        return 0;
    }
    const char *meta = meta_json ? meta_json : "{\"v\":1}";
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

static int json_type_is(const char *json, const char *substr) {
    if (!json || !substr) {
        return 0;
    }
    return strstr(json, substr) != NULL;
}

static int is_delta_event(const char *json) {
    return json_type_is(json, "input_audio_transcription.delta") ||
           json_type_is(json, "conversation.item.input_audio_transcription.delta");
}

static int is_completed_event(const char *json) {
    return json_type_is(json, "input_audio_transcription.completed") ||
           json_type_is(json, "conversation.item.input_audio_transcription.completed");
}

static int is_speech_started_event(const char *json) {
    return json_type_is(json, "speech_started") ||
           json_type_is(json, "input_audio_buffer.speech_started");
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

static int ensure_rtasr_connected(int32_t *rtasr_fd, int32_t epfd) {
    if (*rtasr_fd > 0) {
        return 1;
    }
    int32_t fd = sp_rtasr_create();
    if (fd <= 0) {
        return 0;
    }
    const char *autoflush = "{\"strategy\":\"server_vad\",\"vad\":{\"silence_ms\":500}}";
    (void)sp_rtasr_set_autoflush_json(fd, autoflush, (uint32_t)strlen(autoflush));
    (void)sp_rtasr_set_param_string(fd, "transport", "websocket");
    int32_t c_rc = sp_rtasr_connect(fd);
    if (c_rc != 0) {
        (void)sp_rtasr_close(fd);
        return 0;
    }
    (void)sp_ep_ctl(epfd, SPEAR_EP_CTL_ADD, fd, SPEAR_EPOLLIN | SPEAR_EPOLLOUT | SPEAR_EPOLLERR | SPEAR_EPOLLHUP);
    *rtasr_fd = fd;
    return 1;
}

int main() {
    int32_t epfd = sp_ep_create();
    if (epfd < 0) {
        return 1;
    }
    int32_t ctl_fd = sp_user_stream_ctl_open();
    if (ctl_fd < 0) {
        sp_ep_close(epfd);
        return 1;
    }
    int32_t rc = sp_ep_ctl(epfd, SPEAR_EP_CTL_ADD, ctl_fd, SPEAR_EPOLLIN | SPEAR_EPOLLERR | SPEAR_EPOLLHUP);
    if (rc != 0) {
        sp_user_stream_close(ctl_fd);
        sp_ep_close(epfd);
        return 1;
    }

    sp_stream_t text_stream;
    sp_stream_t voice_stream;
    sp_stream_init(&text_stream, STREAM_TEXT);
    sp_stream_init(&voice_stream, STREAM_VOICE);

    int32_t rtasr_fd = 0;
    char cur_caption[8192] = {0};
    int caption_open = 0;
    int has_pending_warn = 0;
    char pending_warn[256] = {0};

    uint8_t ready_buf[8 * 64];
    for (;;) {
        uint32_t ready_len = sizeof(ready_buf);
        int32_t nready = sp_ep_wait(epfd, (int32_t)(uintptr_t)ready_buf, (int32_t)(uintptr_t)&ready_len, 2000);
        if (nready < 0) {
            break;
        }
        maybe_emit_protocol_warning(&text_stream, &has_pending_warn, pending_warn, sizeof(pending_warn));
        // One epoll loop multiplexes:
        // - ctl_fd: stream lifecycle notifications from the runtime
        // - text_stream / voice_stream: incoming SSF frames from the frontend
        // - rtasr_fd: ASR events produced from forwarded voice audio
        for (int i = 0; i < nready; i++) {
            int32_t fd = 0;
            int32_t ev = 0;
            memcpy(&fd, ready_buf + (i * 8), 4);
            memcpy(&ev, ready_buf + (i * 8) + 4, 4);

            if (ev & SPEAR_EPOLLHUP) {
                if (fd == ctl_fd) {
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
                        goto out;
                    }
                    if (evt.kind == SPEAR_USER_STREAM_CTL_EVENT_SESSION_CLOSED) {
                        goto out;
                    }
                    if (evt.kind != SPEAR_USER_STREAM_CTL_EVENT_STREAM_CONNECTED) {
                        continue;
                    }
                    if (evt.stream_id != STREAM_TEXT && evt.stream_id != STREAM_VOICE) {
                        continue;
                    }
                    // The control plane tells us a logical stream is connectable.
                    // We then open the bidirectional data fd and start watching it.
                    sp_stream_t *s = evt.stream_id == STREAM_TEXT ? &text_stream : &voice_stream;
                    if (s->fd > 0) {
                        continue;
                    }
                    int32_t sfd = sp_user_stream_open((int32_t)evt.stream_id, SPEAR_USER_STREAM_DIR_BIDIRECTIONAL);
                    if (sfd < 0) {
                        continue;
                    }
                    s->fd = sfd;
                    s->out_seq = 1;
                    s->opened = 0;
                    int32_t add_rc = sp_ep_ctl(epfd, SPEAR_EP_CTL_ADD, sfd,
                                              SPEAR_EPOLLIN | SPEAR_EPOLLOUT | SPEAR_EPOLLERR | SPEAR_EPOLLHUP);
                    if (add_rc != 0) {
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
                        // Convert incremental RTASR events into a single text stream transcript.
                        // 将增量 RTASR 事件转换为单条文本流字幕输出。
                        if (is_speech_started_event(json)) {
                            if (!caption_open) {
                                char prefix[16] = {0};
                                build_short_timestamp_prefix(prefix, sizeof(prefix));
                                cur_caption[0] = 0;
                                caption_open = 1;
                                (void)sp_stream_send_text(&text_stream, prefix, "{\"v\":1}");
                            }
                        } else if (is_delta_event(json)) {
                            char tmp[512] = {0};
                            if (extract_string_field(json, "delta", tmp, sizeof(tmp))) {
                                if (!caption_open) {
                                    char prefix[16] = {0};
                                    build_short_timestamp_prefix(prefix, sizeof(prefix));
                                    cur_caption[0] = 0;
                                    caption_open = 1;
                                    (void)sp_stream_send_text(&text_stream, prefix, "{\"v\":1}");
                                }
                                size_t cur = strlen(cur_caption);
                                size_t cap = sizeof(cur_caption) - 1;
                                if (cur < cap) {
                                    strncat(cur_caption, tmp, cap - cur);
                                }
                                (void)sp_stream_send_text(&text_stream, tmp, "{\"v\":1}");
                            }
                        } else if (is_completed_event(json)) {
                            char tmp[4096] = {0};
                            if (extract_string_field(json, "transcript", tmp, sizeof(tmp))) {
                                strncpy(cur_caption, tmp, sizeof(cur_caption) - 1);
                                cur_caption[sizeof(cur_caption) - 1] = 0;
                            }
                            if (caption_open) {
                                (void)sp_stream_send_text(&text_stream, "\n", "{\"v\":1}");
                                (void)sp_stream_send_commit(&text_stream, "{\"v\":1}");
                                caption_open = 0;
                                cur_caption[0] = 0;
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
                // This sample intentionally requires CTRL(open) before any DATA/COMMIT.
                // Frames received earlier are treated as protocol violations and dropped.
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
                        // Start or resume the ASR uplink only when the voice stream explicitly
                        // begins a new utterance.
                        (void)ensure_rtasr_connected(&rtasr_fd, epfd);
                    }
                } else if (v.hdr.msg_type == SPEAR_SSF_MSG_TYPE_DATA) {
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
                    // Voice DATA payloads are raw PCM chunks; they are forwarded directly to RTASR.
                    if (rtasr_fd > 0 && v.data && v.hdr.data_len > 0) {
                        (void)sp_rtasr_write(rtasr_fd, (int32_t)(uintptr_t)v.data, (int32_t)v.hdr.data_len);
                    }
                } else if (v.hdr.msg_type == SPEAR_SSF_MSG_TYPE_COMMIT) {
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
                    if (rtasr_fd > 0) {
                        (void)sp_rtasr_flush(rtasr_fd);
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
