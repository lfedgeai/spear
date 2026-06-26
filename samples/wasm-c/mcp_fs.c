#include <spear.h>

// MCP filesystem tools sample (WASM-C).
// MCP 文件系统工具示例（WASM-C）。
//
// This sample enables MCP and allows a limited set of filesystem tools.
// 本示例启用 MCP，并为文件系统工具设置最小 allowlist。

#ifndef SP_OPENAI_MODEL
#define SP_OPENAI_MODEL "gpt-4o-mini"
#endif

static int32_t sp_cchat_set_param_bool(int32_t fd, const char *key, int enabled) {
    char buf[256];
    int n = snprintf(buf, sizeof(buf), "{\"key\":\"%s\",\"value\":%s}", key,
                     enabled ? "true" : "false");
    if (n <= 0 || (size_t)n >= sizeof(buf)) {
        return SPEAR_CCHAT_ERR_INTERNAL;
    }
    return sp_cchat_set_param_json(fd, buf, (uint32_t)n);
}

static int32_t sp_cchat_set_param_string_array1(int32_t fd, const char *key, const char *v0) {
    char buf[256];
    int n = snprintf(buf, sizeof(buf), "{\"key\":\"%s\",\"value\":[\"%s\"]}", key, v0);
    if (n <= 0 || (size_t)n >= sizeof(buf)) {
        return SPEAR_CCHAT_ERR_INTERNAL;
    }
    return sp_cchat_set_param_json(fd, buf, (uint32_t)n);
}

static int32_t sp_cchat_set_param_string_array2(int32_t fd, const char *key, const char *v0,
                                                const char *v1) {
    char buf[512];
    int n = snprintf(buf, sizeof(buf), "{\"key\":\"%s\",\"value\":[\"%s\",\"%s\"]}", key, v0, v1);
    if (n <= 0 || (size_t)n >= sizeof(buf)) {
        return SPEAR_CCHAT_ERR_INTERNAL;
    }
    return sp_cchat_set_param_json(fd, buf, (uint32_t)n);
}

int main() {
    // Create chat request context.
    // 创建 chat 会话。
    int32_t fd = sp_cchat_create();
    if (fd < 0) {
        printf("cchat_create failed: %d\n", fd);
        return 1;
    }

    int32_t rc = 0;
    // Configure model and execution limits.
    // 配置模型与执行限制。
    rc = sp_cchat_set_param_string(fd, "model", SP_OPENAI_MODEL);
    if (rc != 0) {
        printf("set model failed: %d\n", rc);
        sp_cchat_close(fd);
        return 1;
    }

    rc = sp_cchat_set_param_u32(fd, "timeout_ms", 30000);
    if (rc != 0) {
        printf("set timeout_ms failed: %d\n", rc);
        sp_cchat_close(fd);
        return 1;
    }

    rc = sp_cchat_set_param_u32(fd, "max_iterations", 6);
    if (rc != 0) {
        printf("set max_iterations failed: %d\n", rc);
        sp_cchat_close(fd);
        return 1;
    }

    rc = sp_cchat_set_param_u32(fd, "max_total_tool_calls", 6);
    if (rc != 0) {
        printf("set max_total_tool_calls failed: %d\n", rc);
        sp_cchat_close(fd);
        return 1;
    }

    // Enable MCP.
    // 启用 MCP。
    rc = sp_cchat_set_param_bool(fd, "mcp.enabled", 1);
    if (rc != 0) {
        printf("set mcp.enabled failed: %d\n", rc);
        sp_cchat_close(fd);
        return 1;
    }

    // Select MCP servers.
    // 选择 MCP server。
    rc = sp_cchat_set_param_string_array1(fd, "mcp.server_ids", "fs");
    if (rc != 0) {
        printf("set mcp.server_ids failed: %d\n", rc);
        sp_cchat_close(fd);
        return 1;
    }

    // Restrict allowed tools.
    // 限制允许的工具。
    rc = sp_cchat_set_param_string_array2(fd, "mcp.tool_allowlist", "read_*", "list_*");
    if (rc != 0) {
        printf("set mcp.tool_allowlist failed: %d\n", rc);
        sp_cchat_close(fd);
        return 1;
    }

    // Ask model to read a file via MCP tool.
    // 让模型通过 MCP 工具读取文件。
    const char *prompt =
        "Please use the MCP filesystem tools (server_id=fs). "
        "Read the file path=\"Cargo.toml\" using the provided tool, "
        "then reply with the first 5 lines of that file, and finally say: MCP_OK.";
    rc = sp_cchat_write_msg_str(fd, "user", prompt);
    if (rc != 0) {
        printf("cchat_write_msg failed: %d\n", rc);
        sp_cchat_close(fd);
        return 1;
    }

    int32_t resp_fd = sp_cchat_send(fd, AUTO_TOOL_CALL);
    if (resp_fd < 0) {
        printf("cchat_send failed: %d\n", resp_fd);
        sp_cchat_close(fd);
        return 1;
    }

    uint32_t resp_len = 0;
    uint8_t *resp = sp_cchat_recv_alloc(resp_fd, &resp_len);
    if (!resp) {
        printf("cchat_recv failed\n");
        sp_cchat_close(resp_fd);
        sp_cchat_close(fd);
        return 1;
    }

    printf("response_bytes=%u\n", resp_len);
    printf("response_json=%s\n", (char *)resp);

    free(resp);
    sp_cchat_close(resp_fd);
    sp_cchat_close(fd);
    return 0;
}
