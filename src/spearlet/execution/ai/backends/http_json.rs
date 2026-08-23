use std::collections::HashMap;
use std::future::Future;
use std::time::Duration;

use serde_json::{Map, Value};

use crate::spearlet::execution::ai::ir::{
    CanonicalError, CanonicalRequestEnvelope, CanonicalResponseEnvelope, Operation, ResultPayload,
};
use crate::spearlet::param_keys::{chat as chat_keys, mcp as mcp_keys};

/// Result of one blocking JSON-over-HTTP request execution.
/// 一次阻塞式 JSON-over-HTTP 请求执行结果。
pub struct HttpJsonResponse {
    pub status: i32,
    pub body: Vec<u8>,
    pub headers: HashMap<String, String>,
}

/// Join a base URL with a relative path in a backend-friendly way.
/// 用 backend 友好的方式拼接 base URL 与相对路径。
pub fn join_url(base_url: &str, path: &str) -> String {
    let base = base_url.trim_end_matches('/');
    let p = path.trim_start_matches('/');
    format!("{}/{}", base, p)
}

/// Ensure the request is a chat-completions operation for chat adapters.
/// 确保请求对 chat adapter 来说是 chat-completions 操作。
pub fn ensure_chat_operation(
    req: &CanonicalRequestEnvelope,
    message: &str,
) -> Result<(), CanonicalError> {
    if req.operation != Operation::ChatCompletions {
        return Err(CanonicalError {
            code: "unsupported_operation".to_string(),
            message: message.to_string(),
            retryable: false,
            operation: Some(req.operation.clone()),
        });
    }
    Ok(())
}

/// Require a non-empty trimmed field value from a request payload.
/// 要求请求载荷中的某个字段在 trim 后非空。
pub fn require_non_empty_field(
    operation: Operation,
    value: &str,
    field_name: &str,
) -> Result<String, CanonicalError> {
    let trimmed = value.trim().to_string();
    if trimmed.is_empty() {
        return Err(CanonicalError {
            code: "invalid_request".to_string(),
            message: format!("missing {}", field_name),
            retryable: false,
            operation: Some(operation),
        });
    }
    Ok(trimmed)
}

/// Filter shared chat params, dropping MCP and structural keys.
/// 过滤共享 chat params，移除 MCP 与结构化控制字段。
pub fn filtered_chat_params(params: &HashMap<String, Value>) -> Map<String, Value> {
    let mut out = Map::new();
    for (k, v) in params {
        if k.starts_with(mcp_keys::param::PREFIX) {
            continue;
        }
        if chat_keys::is_structural_param_key(k) {
            continue;
        }
        out.insert(k.clone(), v.clone());
    }
    out
}

/// Attach tools to a JSON object only when the list is non-empty.
/// 仅当 tools 非空时才将其挂到 JSON object 上。
pub fn insert_tools_if_any(obj: &mut Map<String, Value>, tools: &[Value]) {
    if !tools.is_empty() {
        obj.insert("tools".to_string(), Value::Array(tools.to_vec()));
    }
}

/// Parse one JSON response body from an upstream HTTP backend.
/// 解析上游 HTTP backend 返回的 JSON 响应体。
pub fn parse_json_response_body(
    operation: Operation,
    status: i32,
    body: &[u8],
) -> Result<Value, CanonicalError> {
    let status_u16 = status as u16;
    serde_json::from_slice::<Value>(body).map_err(|e| CanonicalError {
        code: "invalid_response".to_string(),
        message: e.to_string(),
        retryable: status_u16 >= 500,
        operation: Some(operation),
    })
}

/// Build the standard upstream-status error used by JSON-over-HTTP adapters.
/// 构建 JSON-over-HTTP adapter 共用的 upstream-status 错误。
pub fn upstream_status_error(
    operation: Operation,
    status: i32,
    extra: Option<String>,
) -> CanonicalError {
    let status_u16 = status as u16;
    CanonicalError {
        code: "upstream_error".to_string(),
        message: match extra {
            Some(m) => format!("upstream status: {}: {}", status_u16, m),
            None => format!("upstream status: {}", status_u16),
        },
        retryable: status_u16 == 429 || status_u16 >= 500,
        operation: Some(operation),
    }
}

/// Build the standard canonical payload envelope for one parsed JSON response.
/// 为已解析的 JSON 响应构建标准 canonical payload envelope。
pub fn build_json_payload_response(
    req: &CanonicalRequestEnvelope,
    backend: &str,
    payload: Value,
    raw: Vec<u8>,
) -> CanonicalResponseEnvelope {
    CanonicalResponseEnvelope {
        version: 1,
        request_id: req.request_id.clone(),
        operation: req.operation.clone(),
        backend: backend.to_string(),
        result: ResultPayload::Payload(payload),
        raw: Some(raw),
    }
}

/// Execute one async future from a sync adapter context.
/// 在同步 adapter 上下文中执行一个异步 future。
pub fn run_blocking_async<T>(
    operation: Operation,
    fut: impl Future<Output = Result<T, CanonicalError>> + Send + 'static,
) -> Result<T, CanonicalError>
where
    T: Send + 'static,
{
    match tokio::runtime::Handle::try_current() {
        Ok(_) => {
            let op_for_spawn = operation.clone();
            std::thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().map_err(|e| CanonicalError {
                    code: "runtime_error".to_string(),
                    message: e.to_string(),
                    retryable: false,
                    operation: Some(op_for_spawn.clone()),
                })?;
                rt.block_on(fut)
            })
            .join()
            .unwrap_or_else(|_| {
                Err(CanonicalError {
                    code: "runtime_error".to_string(),
                    message: "thread join failed".to_string(),
                    retryable: false,
                    operation: Some(operation),
                })
            })
        }
        Err(_) => {
            let rt = tokio::runtime::Runtime::new().map_err(|e| CanonicalError {
                code: "runtime_error".to_string(),
                message: e.to_string(),
                retryable: false,
                operation: Some(operation),
            })?;
            rt.block_on(fut)
        }
    }
}

/// Perform one JSON POST request and collect status/body/headers.
/// 执行一次 JSON POST 请求并收集状态码/响应体/响应头。
pub fn post_json_blocking(
    operation: Operation,
    url: String,
    body: Vec<u8>,
    timeout: Option<Duration>,
    bearer_token: Option<&str>,
) -> Result<HttpJsonResponse, CanonicalError> {
    let token = bearer_token
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    run_blocking_async(operation.clone(), async move {
        let client = reqwest::Client::new();
        let mut r = client
            .post(url)
            .header("content-type", "application/json")
            .body(body);
        if let Some(token) = token {
            r = r.header("authorization", format!("Bearer {}", token));
        }
        if let Some(t) = timeout {
            r = r.timeout(t);
        }
        let resp = r.send().await.map_err(|e| CanonicalError {
            code: "network_error".to_string(),
            message: e.to_string(),
            retryable: true,
            operation: Some(operation.clone()),
        })?;
        let status = resp.status();
        let headers = resp
            .headers()
            .iter()
            .filter_map(|(k, v)| v.to_str().ok().map(|vs| (k.to_string(), vs.to_string())))
            .collect::<HashMap<_, _>>();
        let body = resp.bytes().await.map_err(|e| CanonicalError {
            code: "network_error".to_string(),
            message: e.to_string(),
            retryable: true,
            operation: Some(operation),
        })?;
        Ok(HttpJsonResponse {
            status: status.as_u16() as i32,
            body: body.to_vec(),
            headers,
        })
    })
}
