#include <spear.h>

// Tool calling sample (WASM-C).
// Tool calling 示例（WASM-C）。
//
// This sample registers a host function `sum(a,b)` and lets the model call it.
// 本示例注册一个宿主函数 `sum(a,b)`，并让模型自动发起工具调用。

static uint8_t TOOL_ARENA[128 * 1024];

static int32_t find_int_field(const char *s, int32_t len, const char *key, int32_t fallback) {
    if (!s || len <= 0 || !key) {
        return fallback;
    }
    const char *end = s + len;
    size_t key_len = strlen(key);
    for (const char *p = s; p + key_len < end; p++) {
        if (*p != '"') {
            continue;
        }
        if ((size_t)(end - (p + 1)) < key_len) {
            break;
        }
        if (memcmp(p + 1, key, key_len) != 0) {
            continue;
        }
        const char *q = p + 1 + key_len;
        while (q < end && *q != ':') {
            q++;
        }
        if (q >= end) {
            break;
        }
        q++;
        while (q < end && (*q == ' ' || *q == '\t' || *q == '\n' || *q == '\r')) {
            q++;
        }
        int sign = 1;
        if (q < end && *q == '-') {
            sign = -1;
            q++;
        }
        int32_t v = 0;
        int any = 0;
        while (q < end && *q >= '0' && *q <= '9') {
            any = 1;
            v = (int32_t)(v * 10 + (*q - '0'));
            q++;
        }
        return any ? (int32_t)(v * sign) : fallback;
    }
    return fallback;
}

int32_t sum(int32_t args_ptr, int32_t args_len, int32_t out_ptr, int32_t out_len_ptr) {
    // Tool function entry: args is a JSON object.
    // 工具函数入口：args 为 JSON 对象。
    const char *args = (const char *)(uintptr_t)args_ptr;
    int32_t a = find_int_field(args, args_len, "a", 0);
    int32_t b = find_int_field(args, args_len, "b", 0);
    int32_t s = a + b;

    uint32_t cap = *(uint32_t *)(uintptr_t)out_len_ptr;
    char result[128];
    int n = snprintf(result, sizeof(result), "{\"sum\":%d}", s);
    if (n <= 0 || (size_t)n >= sizeof(result)) {
        return -SPEAR_EIO;
    }

    uint32_t need = (uint32_t)n;
    printf("sum invoked: a=%d b=%d sum=%d cap=%u\n", a, b, s, cap);

    if (cap < need) {
        *(uint32_t *)(uintptr_t)out_len_ptr = need;
        return -SPEAR_ENOSPC;
    }

    memcpy((void *)(uintptr_t)out_ptr, result, need);
    *(uint32_t *)(uintptr_t)out_len_ptr = need;
    return 0;
}

int main() {
    // Create a chat request context fd.
    // 创建 chat 请求上下文 fd。
    int32_t fd = sp_cchat_create();
    if (fd < 0) {
        printf("cchat_create failed: %d\n", fd);
        return 1;
    }

    // Provide arena memory for tool call arguments/results.
    // 提供 tool call 参数/结果的 arena 内存。
    int32_t arena_ptr = (int32_t)(uintptr_t)TOOL_ARENA;
    uint32_t arena_len = (uint32_t)sizeof(TOOL_ARENA);
    sp_cchat_set_param_u32(fd, "tool_arena_ptr", (uint32_t)arena_ptr);
    sp_cchat_set_param_u32(fd, "tool_arena_len", arena_len);
    sp_cchat_set_param_u32(fd, "max_total_tool_calls", 4);
    sp_cchat_set_param_u32(fd, "max_iterations", 4);

    // Choose a model that supports tool calling.
    // 选择支持 tool calling 的模型。
    sp_cchat_set_param_string(fd, "model", "gpt-4o-mini");

    // Ask the model to call the tool.
    // 让模型触发工具调用。
    sp_cchat_write_msg_str(fd, "user", "Please call sum(a,b) for a=7 and b=35.");

    const char *fn_json =
        "{\"type\":\"function\",\"function\":{\"name\":\"sum\",\"description\":\"Add two integers\",\"parameters\":{\"type\":\"object\",\"properties\":{\"a\":{\"type\":\"integer\"},\"b\":{\"type\":\"integer\"}},\"required\":[\"a\",\"b\"]}}}";
    int32_t fn_offset = (int32_t)(uintptr_t)sum;
    printf("tool_fn_offset=%d\n", fn_offset);

    int32_t rc = sp_cchat_write_fn_str(fd, fn_offset, fn_json);
    printf("cchat_write_fn_rc=%d\n", rc);

    if (rc == 0) {
        // AUTO_TOOL_CALL makes the runtime iterate tool calls automatically.
        // AUTO_TOOL_CALL 会让 runtime 自动迭代处理工具调用。
        int32_t resp_fd = sp_cchat_send(fd, AUTO_TOOL_CALL);
        if (resp_fd < 0) {
            printf("cchat_send failed: %d\n", resp_fd);
            sp_cchat_close(fd);
            return 1;
        }

        uint32_t resp_len = 0;
        uint8_t *resp = sp_cchat_recv_alloc(resp_fd, &resp_len);
        if (!resp) {
            printf("cchat_recv_alloc failed\n");
            sp_cchat_close(resp_fd);
            sp_cchat_close(fd);
            return 1;
        }
        printf("chat response (%u bytes):\n%s\n", resp_len, (char *)resp);
        free(resp);
        sp_cchat_close(resp_fd);
    }

    sp_cchat_close(fd);
    return rc == 0 ? 0 : 1;
}
