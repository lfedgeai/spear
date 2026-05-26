#ifndef SPEAR_WASM_SPEAR_H
#define SPEAR_WASM_SPEAR_H

#include <stdint.h>
#include <stddef.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <errno.h>
#include <stdarg.h>

#include "spear_ssf.h"

#define SPEAR_IMPORT(name) __attribute__((import_module("spear"), import_name(name)))

enum {
    SPEAR_CCHAT_OK = 0,
    SPEAR_EPERM = 1,
    SPEAR_ENOENT = 2,
    SPEAR_EIO = 5,
    SPEAR_EBADF = 9,
    SPEAR_EAGAIN = 11,
    SPEAR_ENOMEM = 12,
    SPEAR_EFAULT = 14,
    SPEAR_EINVAL = 22,
    SPEAR_ENOSPC = 28,
    SPEAR_EPIPE = 32,
    SPEAR_ECONNRESET = 104,
    SPEAR_ENOTCONN = 107,
    SPEAR_ETIMEDOUT = 110,

    SPEAR_CCHAT_ERR_INVALID_FD = -SPEAR_EBADF,
    SPEAR_CCHAT_ERR_INVALID_PTR = -SPEAR_EFAULT,
    SPEAR_CCHAT_ERR_BUFFER_TOO_SMALL = -SPEAR_ENOSPC,
    SPEAR_CCHAT_ERR_INVALID_CMD = -SPEAR_EINVAL,
    SPEAR_CCHAT_ERR_INTERNAL = -SPEAR_EIO,
};

enum {
    SPEAR_CCHAT_CTL_SET_PARAM = 1,
    SPEAR_CCHAT_CTL_GET_METRICS = 2,
};

enum {
    SPEAR_CCHAT_SEND_FLAG_ENABLE_METRICS = 1 << 0,
    SPEAR_CCHAT_SEND_FLAG_AUTO_TOOL_CALL = 1 << 1,
};

#define AUTO_TOOL_CALL SPEAR_CCHAT_SEND_FLAG_AUTO_TOOL_CALL

enum {
    SPEAR_RTA_CTL_SET_PARAM = 1,
    SPEAR_RTA_CTL_CONNECT = 2,
    SPEAR_RTA_CTL_GET_STATUS = 3,
    SPEAR_RTA_CTL_SEND_EVENT = 4,
    SPEAR_RTA_CTL_FLUSH = 5,
    SPEAR_RTA_CTL_CLEAR = 6,
    SPEAR_RTA_CTL_SET_AUTOFLUSH = 7,
    SPEAR_RTA_CTL_GET_AUTOFLUSH = 8,
};

enum {
    SPEAR_MIC_CTL_SET_PARAM = 1,
    SPEAR_MIC_CTL_GET_STATUS = 2,
};

SPEAR_IMPORT("time_now_ms")
int64_t sp_time_now_ms(void);

SPEAR_IMPORT("cchat_create")
int32_t sp_cchat_create(void);

SPEAR_IMPORT("cchat_write_msg")
int32_t sp_cchat_write_msg(int32_t fd, int32_t role_ptr, int32_t role_len, int32_t content_ptr,
                           int32_t content_len);

SPEAR_IMPORT("cchat_write_fn")
int32_t sp_cchat_write_fn(int32_t fd, int32_t fn_offset, int32_t fn_ptr, int32_t fn_len);

SPEAR_IMPORT("cchat_ctl")
int32_t sp_cchat_ctl(int32_t fd, int32_t cmd, int32_t arg_ptr, int32_t arg_len_ptr);

SPEAR_IMPORT("cchat_send")
int32_t sp_cchat_send(int32_t fd, int32_t flags);

SPEAR_IMPORT("cchat_recv")
int32_t sp_cchat_recv(int32_t response_fd, int32_t out_ptr, int32_t out_len_ptr);

SPEAR_IMPORT("cchat_close")
int32_t sp_cchat_close(int32_t fd);

SPEAR_IMPORT("rtasr_create")
int32_t sp_rtasr_create(void);

SPEAR_IMPORT("rtasr_ctl")
int32_t sp_rtasr_ctl(int32_t fd, int32_t cmd, int32_t arg_ptr, int32_t arg_len_ptr);

SPEAR_IMPORT("rtasr_write")
int32_t sp_rtasr_write(int32_t fd, int32_t buf_ptr, int32_t buf_len);

SPEAR_IMPORT("rtasr_read")
int32_t sp_rtasr_read(int32_t fd, int32_t out_ptr, int32_t out_len_ptr);

SPEAR_IMPORT("rtasr_close")
int32_t sp_rtasr_close(int32_t fd);

SPEAR_IMPORT("mic_create")
int32_t sp_mic_create(void);

SPEAR_IMPORT("mic_ctl")
int32_t sp_mic_ctl(int32_t fd, int32_t cmd, int32_t arg_ptr, int32_t arg_len_ptr);

SPEAR_IMPORT("mic_read")
int32_t sp_mic_read(int32_t fd, int32_t out_ptr, int32_t out_len_ptr);

SPEAR_IMPORT("mic_close")
int32_t sp_mic_close(int32_t fd);

enum {
    SPEAR_USER_STREAM_DIR_INBOUND = 1,
    SPEAR_USER_STREAM_DIR_OUTBOUND = 2,
    SPEAR_USER_STREAM_DIR_BIDIRECTIONAL = 3,
};

enum {
    SPEAR_USER_STREAM_CTL_EVENT_STREAM_CONNECTED = 1,
    SPEAR_USER_STREAM_CTL_EVENT_SESSION_CLOSED = 2,
};

// SSF (Spear Stream Frame) v1 msg_type values.
// SSF（Spear Stream Frame）v1 的 msg_type 定义。
typedef struct {
    uint32_t stream_id;
    uint32_t kind;
} sp_user_stream_ctl_event_t;

SPEAR_IMPORT("user_stream_open")
int32_t sp_user_stream_open(int32_t stream_id, int32_t direction);

SPEAR_IMPORT("user_stream_read")
int32_t sp_user_stream_read(int32_t fd, int32_t out_ptr, int32_t out_len_ptr);

SPEAR_IMPORT("user_stream_write")
int32_t sp_user_stream_write(int32_t fd, int32_t buf_ptr, int32_t buf_len);

SPEAR_IMPORT("user_stream_close")
int32_t sp_user_stream_close(int32_t fd);

SPEAR_IMPORT("user_stream_ctl_open")
int32_t sp_user_stream_ctl_open(void);

SPEAR_IMPORT("user_stream_ctl_read")
int32_t sp_user_stream_ctl_read(int32_t fd, int32_t out_ptr, int32_t out_len_ptr);

enum {
    SPEAR_EPOLL_CTL_ADD = 1,
    SPEAR_EPOLL_CTL_MOD = 2,
    SPEAR_EPOLL_CTL_DEL = 3,
};

#define SPEAR_EP_CTL_ADD SPEAR_EPOLL_CTL_ADD
#define SPEAR_EP_CTL_MOD SPEAR_EPOLL_CTL_MOD
#define SPEAR_EP_CTL_DEL SPEAR_EPOLL_CTL_DEL

enum {
    SPEAR_EPOLLIN = 0x001,
    SPEAR_EPOLLOUT = 0x004,
    SPEAR_EPOLLERR = 0x008,
    SPEAR_EPOLLHUP = 0x010,
};

enum {
    SPEAR_FD_CTL_SET_FLAGS = 1,
    SPEAR_FD_CTL_GET_FLAGS = 2,
    SPEAR_FD_CTL_GET_KIND = 3,
    SPEAR_FD_CTL_GET_STATUS = 4,
    SPEAR_FD_CTL_GET_METRICS = 5,
};

SPEAR_IMPORT("spear_epoll_create")
int32_t sp_epoll_create(void);

SPEAR_IMPORT("spear_epoll_ctl")
int32_t sp_epoll_ctl(int32_t epfd, int32_t op, int32_t fd, int32_t events);

SPEAR_IMPORT("spear_epoll_wait")
int32_t sp_epoll_wait(int32_t epfd, int32_t out_ptr, int32_t out_len_ptr, int32_t timeout_ms);

SPEAR_IMPORT("spear_epoll_close")
int32_t sp_epoll_close(int32_t epfd);

static inline int32_t sp_ep_create(void) {
    return sp_epoll_create();
}

static inline int32_t sp_ep_ctl(int32_t epfd, int32_t op, int32_t fd, int32_t events) {
    return sp_epoll_ctl(epfd, op, fd, events);
}

static inline int32_t sp_ep_wait(int32_t epfd, int32_t out_ptr, int32_t out_len_ptr, int32_t timeout_ms) {
    return sp_epoll_wait(epfd, out_ptr, out_len_ptr, timeout_ms);
}

static inline int32_t sp_ep_close(int32_t epfd) {
    return sp_epoll_close(epfd);
}

SPEAR_IMPORT("spear_fd_ctl")
int32_t sp_fd_ctl(int32_t fd, int32_t cmd, int32_t arg_ptr, int32_t arg_len_ptr);

static inline int32_t sp_cchat_write_msg_str(int32_t fd, const char *role, const char *content) {
    return sp_cchat_write_msg(fd, (int32_t)(uintptr_t)role, (int32_t)strlen(role),
                              (int32_t)(uintptr_t)content, (int32_t)strlen(content));
}

static inline int32_t sp_cchat_write_fn_str(int32_t fd, int32_t fn_offset, const char *fn_json) {
    return sp_cchat_write_fn(fd, fn_offset, (int32_t)(uintptr_t)fn_json, (int32_t)strlen(fn_json));
}

static inline int32_t sp_cchat_set_param_json(int32_t fd, const char *json, uint32_t json_len) {
    uint32_t len = json_len;
    return sp_cchat_ctl(fd, SPEAR_CCHAT_CTL_SET_PARAM, (int32_t)(uintptr_t)json,
                        (int32_t)(uintptr_t)&len);
}

static inline int32_t sp_cchat_set_param_string(int32_t fd, const char *key, const char *value) {
    char buf[512];
    int n = snprintf(buf, sizeof(buf), "{\"key\":\"%s\",\"value\":\"%s\"}", key, value);
    if (n <= 0 || (size_t)n >= sizeof(buf)) {
        return SPEAR_CCHAT_ERR_INTERNAL;
    }
    return sp_cchat_set_param_json(fd, buf, (uint32_t)n);
}

static inline int32_t sp_cchat_set_param_u32(int32_t fd, const char *key, uint32_t value) {
    char buf[256];
    int n = snprintf(buf, sizeof(buf), "{\"key\":\"%s\",\"value\":%u}", key, value);
    if (n <= 0 || (size_t)n >= sizeof(buf)) {
        return SPEAR_CCHAT_ERR_INTERNAL;
    }
    return sp_cchat_set_param_json(fd, buf, (uint32_t)n);
}

static inline uint8_t *sp_cchat_recv_alloc(int32_t resp_fd, uint32_t *out_len) {
    uint32_t cap = 64 * 1024;
    uint8_t *buf = (uint8_t *)malloc(cap + 1);
    if (!buf) {
        return NULL;
    }
    for (int attempt = 0; attempt < 3; attempt++) {
        uint32_t len = cap;
        int32_t rc = sp_cchat_recv(resp_fd, (int32_t)(uintptr_t)buf, (int32_t)(uintptr_t)&len);
        if (rc >= 0) {
            buf[len] = 0;
            *out_len = len;
            return buf;
        }
        if (rc != -SPEAR_ENOSPC) {
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

static inline int32_t sp_rtasr_set_param_json(int32_t fd, const char *json, uint32_t json_len) {
    uint32_t len = json_len;
    return sp_rtasr_ctl(fd, SPEAR_RTA_CTL_SET_PARAM, (int32_t)(uintptr_t)json, (int32_t)(uintptr_t)&len);
}

static inline int32_t sp_rtasr_set_param_string(int32_t fd, const char *key, const char *value) {
    char buf[512];
    int n = snprintf(buf, sizeof(buf), "{\"key\":\"%s\",\"value\":\"%s\"}", key, value);
    if (n <= 0 || (size_t)n >= sizeof(buf)) {
        return SPEAR_CCHAT_ERR_INTERNAL;
    }
    return sp_rtasr_set_param_json(fd, buf, (uint32_t)n);
}

static inline int32_t sp_rtasr_set_param_u32(int32_t fd, const char *key, uint32_t value) {
    char buf[256];
    int n = snprintf(buf, sizeof(buf), "{\"key\":\"%s\",\"value\":%u}", key, value);
    if (n <= 0 || (size_t)n >= sizeof(buf)) {
        return SPEAR_CCHAT_ERR_INTERNAL;
    }
    return sp_rtasr_set_param_json(fd, buf, (uint32_t)n);
}

static inline int32_t sp_rtasr_connect(int32_t fd) {
    uint32_t len = 0;
    return sp_rtasr_ctl(fd, SPEAR_RTA_CTL_CONNECT, 0, (int32_t)(uintptr_t)&len);
}

static inline int32_t sp_rtasr_flush(int32_t fd) {
    uint32_t len = 0;
    return sp_rtasr_ctl(fd, SPEAR_RTA_CTL_FLUSH, 0, (int32_t)(uintptr_t)&len);
}

static inline int32_t sp_rtasr_set_autoflush_json(int32_t fd, const char *json, uint32_t json_len) {
    uint32_t len = json_len;
    return sp_rtasr_ctl(fd, SPEAR_RTA_CTL_SET_AUTOFLUSH, (int32_t)(uintptr_t)json, (int32_t)(uintptr_t)&len);
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
        if (rc != -SPEAR_ENOSPC) {
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

static inline int32_t sp_mic_set_param_json(int32_t fd, const char *json, uint32_t json_len) {
    uint32_t len = json_len;
    return sp_mic_ctl(fd, SPEAR_MIC_CTL_SET_PARAM, (int32_t)(uintptr_t)json, (int32_t)(uintptr_t)&len);
}

static inline uint8_t *sp_mic_get_status_alloc(int32_t fd, uint32_t *out_len) {
    uint32_t cap = 8 * 1024;
    uint8_t *buf = (uint8_t *)malloc(cap + 1);
    if (!buf) {
        return NULL;
    }
    for (int attempt = 0; attempt < 3; attempt++) {
        uint32_t len = cap;
        int32_t rc = sp_mic_ctl(fd, SPEAR_MIC_CTL_GET_STATUS, (int32_t)(uintptr_t)buf, (int32_t)(uintptr_t)&len);
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

static inline uint8_t *sp_mic_read_alloc(int32_t fd, uint32_t *out_len) {
    uint32_t cap = 64 * 1024;
    uint8_t *buf = (uint8_t *)malloc(cap);
    if (!buf) {
        return NULL;
    }
    for (int attempt = 0; attempt < 3; attempt++) {
        uint32_t len = cap;
        int32_t rc = sp_mic_read(fd, (int32_t)(uintptr_t)buf, (int32_t)(uintptr_t)&len);
        if (rc >= 0) {
            *out_len = len;
            return buf;
        }
        if (rc != -SPEAR_ENOSPC) {
            free(buf);
            return NULL;
        }
        cap = len;
        uint8_t *b2 = (uint8_t *)realloc(buf, cap);
        if (!b2) {
            free(buf);
            return NULL;
        }
        buf = b2;
    }
    free(buf);
    return NULL;
}

static inline uint8_t *sp_user_stream_read_alloc(int32_t fd, uint32_t *out_len) {
    uint32_t cap = 64 * 1024;
    uint8_t *buf = (uint8_t *)malloc(cap);
    if (!buf) {
        return NULL;
    }
    for (int attempt = 0; attempt < 3; attempt++) {
        uint32_t len = cap;
        int32_t rc = sp_user_stream_read(fd, (int32_t)(uintptr_t)buf, (int32_t)(uintptr_t)&len);
        if (rc >= 0) {
            *out_len = len;
            return buf;
        }
        if (rc != -SPEAR_ENOSPC) {
            free(buf);
            return NULL;
        }
        cap = len;
        uint8_t *b2 = (uint8_t *)realloc(buf, cap);
        if (!b2) {
            free(buf);
            return NULL;
        }
        buf = b2;
    }
    free(buf);
    return NULL;
}

static inline int32_t sp_user_stream_ctl_read_event(int32_t fd, sp_user_stream_ctl_event_t *out_evt) {
    if (!out_evt) {
        return SPEAR_CCHAT_ERR_INVALID_PTR;
    }
    uint32_t len = (uint32_t)sizeof(*out_evt);
    return sp_user_stream_ctl_read(fd, (int32_t)(uintptr_t)out_evt, (int32_t)(uintptr_t)&len);
}

enum {
    SP_LOG_TRACE = 0,
    SP_LOG_DEBUG = 1,
    SP_LOG_INFO = 2,
    SP_LOG_WARN = 3,
    SP_LOG_ERROR = 4,
};

SPEAR_IMPORT("log")
int32_t sp_log(int32_t level, int32_t msg_ptr, int32_t msg_len);

static inline int32_t sp_log_write(int32_t level, const void *msg, size_t msg_len) {
    if (!msg && msg_len != 0) {
        return SPEAR_CCHAT_ERR_INVALID_PTR;
    }
    if (msg_len > (size_t)(16 * 1024)) {
        msg_len = (size_t)(16 * 1024);
    }
    return sp_log(level, (int32_t)(uintptr_t)msg, (int32_t)msg_len);
}

static inline int32_t sp_log_str(int32_t level, const char *msg) {
    if (!msg) {
        return SPEAR_CCHAT_ERR_INVALID_PTR;
    }
    return sp_log_write(level, msg, strlen(msg));
}

static inline int sp_logf(int32_t level, const char *fmt, ...) {
    if (!fmt) {
        return SPEAR_CCHAT_ERR_INVALID_PTR;
    }

    char stack_buf[1024];
    va_list ap;
    va_start(ap, fmt);
    int n = vsnprintf(stack_buf, sizeof(stack_buf), fmt, ap);
    va_end(ap);

    if (n < 0) {
        return n;
    }

    size_t need = (size_t)n;
    if (need < sizeof(stack_buf)) {
        int32_t rc = sp_log_write(level, stack_buf, need);
        return (rc == 0) ? n : rc;
    }

    size_t cap = need + 1;
    if (cap > (size_t)(16 * 1024) + 1) {
        cap = (size_t)(16 * 1024) + 1;
    }

    char *heap_buf = (char *)malloc(cap);
    if (!heap_buf) {
        int32_t rc = sp_log_write(level, stack_buf, sizeof(stack_buf) - 1);
        return (rc == 0) ? (int)(sizeof(stack_buf) - 1) : rc;
    }

    va_start(ap, fmt);
    (void)vsnprintf(heap_buf, cap, fmt, ap);
    va_end(ap);

    size_t wrote = strnlen(heap_buf, cap - 1);
    int32_t rc = sp_log_write(level, heap_buf, wrote);
    free(heap_buf);
    return (rc == 0) ? (int)wrote : rc;
}

static inline int sp_tracef(const char *fmt, ...) {
    va_list ap;
    va_start(ap, fmt);
    char buf[1024];
    int n = vsnprintf(buf, sizeof(buf), fmt, ap);
    va_end(ap);
    if (n < 0) return n;
    size_t wrote = (size_t)n < sizeof(buf) ? (size_t)n : (sizeof(buf) - 1);
    int32_t rc = sp_log_write(SP_LOG_TRACE, buf, wrote);
    return (rc == 0) ? (int)wrote : rc;
}

static inline int sp_debugf(const char *fmt, ...) {
    va_list ap;
    va_start(ap, fmt);
    char buf[1024];
    int n = vsnprintf(buf, sizeof(buf), fmt, ap);
    va_end(ap);
    if (n < 0) return n;
    size_t wrote = (size_t)n < sizeof(buf) ? (size_t)n : (sizeof(buf) - 1);
    int32_t rc = sp_log_write(SP_LOG_DEBUG, buf, wrote);
    return (rc == 0) ? (int)wrote : rc;
}

static inline int sp_infof(const char *fmt, ...) {
    va_list ap;
    va_start(ap, fmt);
    char buf[1024];
    int n = vsnprintf(buf, sizeof(buf), fmt, ap);
    va_end(ap);
    if (n < 0) return n;
    size_t wrote = (size_t)n < sizeof(buf) ? (size_t)n : (sizeof(buf) - 1);
    int32_t rc = sp_log_write(SP_LOG_INFO, buf, wrote);
    return (rc == 0) ? (int)wrote : rc;
}

static inline int sp_warnf(const char *fmt, ...) {
    va_list ap;
    va_start(ap, fmt);
    char buf[1024];
    int n = vsnprintf(buf, sizeof(buf), fmt, ap);
    va_end(ap);
    if (n < 0) return n;
    size_t wrote = (size_t)n < sizeof(buf) ? (size_t)n : (sizeof(buf) - 1);
    int32_t rc = sp_log_write(SP_LOG_WARN, buf, wrote);
    return (rc == 0) ? (int)wrote : rc;
}

static inline int sp_errorf(const char *fmt, ...) {
    va_list ap;
    va_start(ap, fmt);
    char buf[1024];
    int n = vsnprintf(buf, sizeof(buf), fmt, ap);
    va_end(ap);
    if (n < 0) return n;
    size_t wrote = (size_t)n < sizeof(buf) ? (size_t)n : (sizeof(buf) - 1);
    int32_t rc = sp_log_write(SP_LOG_ERROR, buf, wrote);
    return (rc == 0) ? (int)wrote : rc;
}

static inline int sp_fprintf(FILE *stream, const char *fmt, ...) {
    int32_t level = SP_LOG_INFO;
    if (stream == stderr) {
        level = SP_LOG_ERROR;
    } else if (stream != stdout && stream != NULL) {
        level = SP_LOG_DEBUG;
    }

    va_list ap;
    va_start(ap, fmt);
    char buf[1024];
    int n = vsnprintf(buf, sizeof(buf), fmt, ap);
    va_end(ap);
    if (n < 0) return n;
    size_t wrote = (size_t)n < sizeof(buf) ? (size_t)n : (sizeof(buf) - 1);
    int32_t rc = sp_log_write(level, buf, wrote);
    return (rc == 0) ? (int)wrote : rc;
}

static inline int sp_puts_log(const char *s) {
    if (!s) {
        return SPEAR_CCHAT_ERR_INVALID_PTR;
    }
    return sp_infof("%s\n", s);
}

#if !defined(SPEAR_DISABLE_STDIO_REDIRECT_TO_LOG)
#if !defined(SPEAR_REDIRECT_STDIO_TO_LOG)
#if defined(__wasm32__) || defined(__wasi__)
#define SPEAR_REDIRECT_STDIO_TO_LOG 1
#endif
#endif
#endif

#if defined(SPEAR_REDIRECT_STDIO_TO_LOG)
#define printf(...) sp_infof(__VA_ARGS__)
#define fprintf(stream, ...) sp_fprintf((stream), __VA_ARGS__)
#define puts(s) sp_puts_log((s))
#endif

#endif
