use super::errno::{EACCES, EBADF, EINVAL, EIO};
use crate::spearlet::execution::ai::ir::{CanonicalRequestEnvelope, Payload, ResultPayload};
use crate::spearlet::execution::ai::ir::{ChatMessage, ToolCall};
use crate::spearlet::execution::ai::normalize::chat::normalize_cchat_session;
use crate::spearlet::execution::host_api::DefaultHostApi;
use crate::spearlet::execution::hostcall::types::{
    ChatResponseState, ChatSessionState, FdEntry, FdFlags, FdInner, FdKind, PollEvents,
};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::time::Duration;

use crate::spearlet::mcp::policy::{
    decide_mcp_exec, filter_and_namespace_openai_tools, server_allowed_tools, McpSessionParams,
};
use crate::spearlet::mcp::task_subset::task_default_session_params;
use crate::spearlet::param_keys::{chat as chat_keys, mcp as mcp_keys};

fn redact_canonical_request_for_log(req: &CanonicalRequestEnvelope) -> CanonicalRequestEnvelope {
    let mut out = req.clone();

    for (k, v) in out.meta.iter_mut() {
        if should_redact_key(k) {
            *v = "***".to_string();
        }
    }

    for (k, v) in out.extra.iter_mut() {
        redact_value_by_key(k, v);
    }

    match &mut out.payload {
        Payload::ChatCompletions(p) => {
            for (k, v) in p.params.iter_mut() {
                redact_value_by_key(k, v);
            }
        }
        Payload::Embeddings(_) => {}
        Payload::ImageGeneration(_) => {}
        Payload::SpeechToText(_) => {}
        Payload::TextToSpeech(_) => {}
    }

    out
}

fn should_redact_key(key: &str) -> bool {
    let k = key.to_ascii_lowercase();
    k.contains("api_key")
        || k.contains("apikey")
        || k.contains("authorization")
        || k == "auth"
        || k.ends_with("_token")
        || k.contains("token")
        || k.contains("secret")
        || k.contains("password")
        || k.contains("cookie")
}

fn should_redact_header_name(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n == "authorization" || n == "cookie" || n == "x-api-key" || n.ends_with("-token")
}

fn redact_value_by_key(key: &str, v: &mut Value) {
    if should_redact_key(key) {
        *v = Value::String("***".to_string());
        return;
    }

    if key.eq_ignore_ascii_case("headers") {
        if let Value::Object(obj) = v {
            for (hk, hv) in obj.iter_mut() {
                if should_redact_header_name(hk) {
                    *hv = Value::String("***".to_string());
                }
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct ChatSessionSnapshot {
    pub fd: i32,
    pub messages: Vec<ChatMessage>,
    pub tools: Vec<(i32, String)>,
    pub params: HashMap<String, serde_json::Value>,
    pub mcp: McpSessionParams,
}

impl DefaultHostApi {
    fn cchat_attach_debug_fields(&self, mut v: Value, backend: &str, model: &str) -> Value {
        let resp_model = v
            .get("model")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        let model = if model.trim().is_empty() && !resp_model.is_empty() {
            resp_model.as_str()
        } else {
            model
        };
        match v.as_object_mut() {
            Some(obj) => {
                obj.insert(
                    "_spear".to_string(),
                    json!({"backend": backend, "model": model}),
                );
                v
            }
            None => json!({"_spear": {"backend": backend, "model": model}, "result": v}),
        }
    }

    pub fn cchat_create(&self) -> i32 {
        let fd = self.fd_table.alloc(FdEntry {
            kind: FdKind::ChatSession,
            flags: FdFlags::default(),
            poll_mask: PollEvents::default(),
            watchers: HashSet::new(),
            closed: false,
            inner: FdInner::ChatSession(ChatSessionState::default()),
        });
        self.cchat_apply_task_mcp_defaults(fd);
        fd
    }

    fn cchat_apply_task_mcp_defaults(&self, fd: i32) {
        let Some(task_policy) = self.mcp_task_policy.as_ref() else {
            return;
        };
        let defaults = task_default_session_params(task_policy);
        if !defaults.is_effectively_enabled() {
            return;
        }

        let Some(entry) = self.fd_table.get(fd) else {
            return;
        };
        let mut e = match entry.lock() {
            Ok(v) => v,
            Err(_) => return,
        };
        if e.closed {
            return;
        }
        let FdInner::ChatSession(s) = &mut e.inner else {
            return;
        };
        if !s.mcp.enabled {
            s.mcp.enabled = defaults.enabled;
        }
        if s.mcp.server_ids.is_empty() {
            s.mcp.server_ids = defaults.server_ids;
        }
        if s.mcp.task_tool_allowlist.is_empty() {
            s.mcp.task_tool_allowlist = defaults.task_tool_allowlist;
        }
        if s.mcp.task_tool_denylist.is_empty() {
            s.mcp.task_tool_denylist = defaults.task_tool_denylist;
        }
    }

    pub fn cchat_write_msg(&self, fd: i32, role: String, content: String) -> i32 {
        let Some(entry) = self.fd_table.get(fd) else {
            return -EBADF;
        };
        let mut e = match entry.lock() {
            Ok(v) => v,
            Err(_) => return -EIO,
        };
        if e.closed {
            return -EBADF;
        }
        let FdInner::ChatSession(s) = &mut e.inner else {
            return -EBADF;
        };
        s.messages.push(ChatMessage {
            role,
            content: serde_json::Value::String(content),
            tool_call_id: None,
            tool_calls: None,
            name: None,
        });
        0
    }

    pub fn cchat_append_message(&self, fd: i32, msg: ChatMessage) -> i32 {
        let Some(entry) = self.fd_table.get(fd) else {
            return -EBADF;
        };
        let mut e = match entry.lock() {
            Ok(v) => v,
            Err(_) => return -EIO,
        };
        if e.closed {
            return -EBADF;
        }
        let FdInner::ChatSession(s) = &mut e.inner else {
            return -EBADF;
        };
        s.messages.push(msg);
        0
    }

    pub fn cchat_snapshot(&self, fd: i32) -> Result<ChatSessionSnapshot, i32> {
        self.cchat_get_session_snapshot(fd)
    }

    pub fn cchat_write_fn(&self, fd: i32, fn_offset: i32, fn_json: String) -> i32 {
        let Some(entry) = self.fd_table.get(fd) else {
            return -EBADF;
        };
        let mut e = match entry.lock() {
            Ok(v) => v,
            Err(_) => return -EIO,
        };
        if e.closed {
            return -EBADF;
        }
        let FdInner::ChatSession(s) = &mut e.inner else {
            return -EBADF;
        };
        s.tools.push((fn_offset, fn_json));
        0
    }

    pub fn cchat_ctl_set_param(&self, fd: i32, key: String, value: serde_json::Value) -> i32 {
        let Some(entry) = self.fd_table.get(fd) else {
            return -EBADF;
        };
        if key.starts_with(mcp_keys::param::TASK_PREFIX) {
            return -EACCES;
        }
        if key == mcp_keys::param::ENABLED || key == mcp_keys::param::SERVER_IDS {
            if let Some(task_policy) = self.mcp_task_policy.as_ref() {
                if key == mcp_keys::param::ENABLED {
                    let Some(b) = value.as_bool() else {
                        return -EINVAL;
                    };
                    if b && !task_policy.enabled {
                        return -EACCES;
                    }
                }
                if key == mcp_keys::param::SERVER_IDS {
                    let Some(arr) = value.as_array() else {
                        return -EINVAL;
                    };
                    let requested = arr
                        .iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect::<Vec<_>>();
                    if crate::spearlet::mcp::task_subset::validate_requested_server_ids(
                        task_policy,
                        &requested,
                    )
                    .is_err()
                    {
                        return -EACCES;
                    }
                }
            }
        }
        let mut e = match entry.lock() {
            Ok(v) => v,
            Err(_) => return -EIO,
        };
        if e.closed {
            return -EBADF;
        }
        match &mut e.inner {
            FdInner::ChatSession(s) => {
                if key == mcp_keys::param::ENABLED {
                    let Some(b) = value.as_bool() else {
                        return -EINVAL;
                    };
                    s.mcp.enabled = b;
                    return 0;
                }
                if key == mcp_keys::param::SERVER_IDS {
                    let Some(arr) = value.as_array() else {
                        return -EINVAL;
                    };
                    let requested = arr
                        .iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect::<Vec<_>>();
                    s.mcp.server_ids = requested;
                    return 0;
                }
                if key == mcp_keys::param::TOOL_ALLOWLIST {
                    let Some(arr) = value.as_array() else {
                        return -EINVAL;
                    };
                    s.mcp.tool_allowlist = arr
                        .iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect::<Vec<_>>();
                    return 0;
                }
                if key == mcp_keys::param::TOOL_DENYLIST {
                    let Some(arr) = value.as_array() else {
                        return -EINVAL;
                    };
                    s.mcp.tool_denylist = arr
                        .iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect::<Vec<_>>();
                    return 0;
                }

                s.params.insert(key, value);
                0
            }
            FdInner::ChatResponse(_) => 0,
            _ => -EBADF,
        }
    }

    pub fn cchat_ctl_get_metrics(&self, fd: i32) -> Result<Vec<u8>, i32> {
        let Some(entry) = self.fd_table.get(fd) else {
            return Err(-EBADF);
        };
        let e = match entry.lock() {
            Ok(v) => v,
            Err(_) => return Err(-EIO),
        };
        let FdInner::ChatResponse(r) = &e.inner else {
            return Err(-EBADF);
        };
        if r.metrics_bytes.is_empty() {
            Ok(b"{}".to_vec())
        } else {
            Ok(r.metrics_bytes.clone())
        }
    }

    pub fn cchat_send(&self, fd: i32, flags: i32) -> Result<i32, i32> {
        let metrics_enabled = (flags & 1) != 0;
        let snapshot = self.cchat_get_session_snapshot(fd)?;
        let snapshot = self.cchat_inject_mcp_tools(&snapshot);
        let resp_fd = self.fd_table.alloc(FdEntry {
            kind: FdKind::ChatResponse,
            flags: FdFlags::default(),
            poll_mask: PollEvents::default(),
            watchers: HashSet::new(),
            closed: false,
            inner: FdInner::ChatResponse(ChatResponseState::default()),
        });

        let req = normalize_cchat_session(&snapshot);
        tracing::debug!(
            chat_fd = fd,
            response_fd = resp_fd,
            flags,
            req = ?redact_canonical_request_for_log(&req),
            "cchat_send canonical request"
        );
        let resp = match self.ai_engine_holder.get().invoke(&req) {
            Ok(r) => r,
            Err(e) => {
                let body = json!({"error": {"message": e.to_string()}});
                let bytes = serde_json::to_vec(&body).map_err(|_| -EIO)?;
                let metrics_bytes = if metrics_enabled {
                    b"{}".to_vec()
                } else {
                    Vec::new()
                };
                self.cchat_put_response(resp_fd, bytes, metrics_bytes)?;
                return Ok(resp_fd);
            }
        };

        let req_model = match &req.payload {
            Payload::ChatCompletions(p) => p.model.as_str(),
            _ => "",
        };

        let bytes = match resp.result {
            ResultPayload::Payload(v) => {
                let v = self.cchat_attach_debug_fields(v, &resp.backend, req_model);
                serde_json::to_vec(&v).map_err(|_| -EIO)?
            }
            ResultPayload::Error(e) => {
                let body = json!({"error": {"code": e.code, "message": e.message}});
                serde_json::to_vec(&body).map_err(|_| -EIO)?
            }
        };
        let metrics_bytes = if metrics_enabled {
            let usage = json!({
                "prompt_tokens": snapshot.messages.len() as i64,
                "completion_tokens": 1,
                "total_tokens": (snapshot.messages.len() as i64) + 1,
            });
            serde_json::to_vec(&usage).unwrap_or_else(|_| b"{}".to_vec())
        } else {
            Vec::new()
        };

        self.cchat_put_response(resp_fd, bytes, metrics_bytes)?;
        Ok(resp_fd)
    }

    pub fn cchat_send_with_tools<F>(
        &self,
        fd: i32,
        flags: i32,
        mut tool_exec: F,
    ) -> Result<i32, i32>
    where
        F: FnMut(i32, &str) -> Result<String, i32>,
    {
        const METRICS_ENABLED: i32 = 1;
        const AUTO_TOOL_CALL: i32 = 2;

        if (flags & AUTO_TOOL_CALL) == 0 {
            return self.cchat_send(fd, flags);
        }

        let metrics_enabled = (flags & METRICS_ENABLED) != 0;
        let resp_fd = self.fd_table.alloc(FdEntry {
            kind: FdKind::ChatResponse,
            flags: FdFlags::default(),
            poll_mask: PollEvents::default(),
            watchers: HashSet::new(),
            closed: false,
            inner: FdInner::ChatResponse(ChatResponseState::default()),
        });

        let mut total_tool_calls: u32 = 0;
        let mut iter: u32 = 0;

        loop {
            let snapshot = match self.cchat_get_session_snapshot(fd) {
                Ok(s) => s,
                Err(e) => return Err(e),
            };

            let injected_snapshot = self.cchat_inject_mcp_tools(&snapshot);

            let max_iterations = snapshot
                .params
                .get(chat_keys::MAX_ITERATIONS)
                .and_then(|v| v.as_u64())
                .unwrap_or(8)
                .min(128) as u32;
            let max_total_tool_calls = snapshot
                .params
                .get(chat_keys::MAX_TOTAL_TOOL_CALLS)
                .or_else(|| snapshot.params.get(chat_keys::MAX_TOOL_CALLS))
                .and_then(|v| v.as_u64())
                .unwrap_or(32)
                .min(10_000) as u32;

            if iter >= max_iterations {
                let body = json!({"error": {"code": "tool_call_limit", "message": "exceeded max_iterations"}});
                let bytes = serde_json::to_vec(&body).map_err(|_| -EIO)?;
                let metrics_bytes = if metrics_enabled {
                    b"{}".to_vec()
                } else {
                    Vec::new()
                };
                self.cchat_put_response(resp_fd, bytes, metrics_bytes)?;
                return Ok(resp_fd);
            }

            if total_tool_calls > max_total_tool_calls {
                let body = json!({"error": {"code": "tool_call_limit", "message": "exceeded max_total_tool_calls"}});
                let bytes = serde_json::to_vec(&body).map_err(|_| -EIO)?;
                let metrics_bytes = if metrics_enabled {
                    b"{}".to_vec()
                } else {
                    Vec::new()
                };
                self.cchat_put_response(resp_fd, bytes, metrics_bytes)?;
                return Ok(resp_fd);
            }

            let tool_name_to_offset = build_tool_name_to_offset(&snapshot.tools);

            let req = normalize_cchat_session(&injected_snapshot);
            tracing::debug!(
                chat_fd = fd,
                response_fd = resp_fd,
                iter,
                total_tool_calls,
                max_iterations,
                max_total_tool_calls,
                req = ?redact_canonical_request_for_log(&req),
                "cchat_send canonical request"
            );
            let resp = match self.ai_engine_holder.get().invoke(&req) {
                Ok(r) => r,
                Err(e) => {
                    let body = json!({"error": {"message": e.to_string()}});
                    let bytes = serde_json::to_vec(&body).map_err(|_| -EIO)?;
                    let metrics_bytes = if metrics_enabled {
                        b"{}".to_vec()
                    } else {
                        Vec::new()
                    };
                    self.cchat_put_response(resp_fd, bytes, metrics_bytes)?;
                    return Ok(resp_fd);
                }
            };

            let req_model = match &req.payload {
                Payload::ChatCompletions(p) => p.model.as_str(),
                _ => "",
            };

            let response_value = match resp.result {
                ResultPayload::Payload(v) => v,
                ResultPayload::Error(e) => {
                    let body = json!({"error": {"code": e.code, "message": e.message}});
                    let bytes = serde_json::to_vec(&body).map_err(|_| -EIO)?;
                    let metrics_bytes = if metrics_enabled {
                        b"{}".to_vec()
                    } else {
                        Vec::new()
                    };
                    self.cchat_put_response(resp_fd, bytes, metrics_bytes)?;
                    return Ok(resp_fd);
                }
            };

            let parsed = parse_openai_tool_calls(&response_value);

            match parsed {
                None => {
                    let assistant_msg = extract_openai_assistant_message(&response_value);
                    if let Some(m) = assistant_msg {
                        let _ = self.cchat_append_message(fd, m);
                    }

                    let response_value =
                        self.cchat_attach_debug_fields(response_value, &resp.backend, req_model);
                    let bytes = serde_json::to_vec(&response_value).map_err(|_| -EIO)?;
                    let metrics_bytes = if metrics_enabled {
                        let usage = json!({
                            "prompt_tokens": snapshot.messages.len() as i64,
                            "completion_tokens": 1,
                            "total_tokens": (snapshot.messages.len() as i64) + 1,
                        });
                        serde_json::to_vec(&usage).unwrap_or_else(|_| b"{}".to_vec())
                    } else {
                        Vec::new()
                    };
                    self.cchat_put_response(resp_fd, bytes, metrics_bytes)?;
                    return Ok(resp_fd);
                }
                Some((assistant, tool_calls)) => {
                    let _ = self.cchat_append_message(fd, assistant);

                    for tc in tool_calls.iter() {
                        if total_tool_calls >= max_total_tool_calls {
                            let body = json!({"error": {"code": "tool_call_limit", "message": "exceeded max_total_tool_calls"}});
                            let bytes = serde_json::to_vec(&body).map_err(|_| -EIO)?;
                            let metrics_bytes = if metrics_enabled {
                                b"{}".to_vec()
                            } else {
                                Vec::new()
                            };
                            self.cchat_put_response(resp_fd, bytes, metrics_bytes)?;
                            return Ok(resp_fd);
                        }

                        total_tool_calls += 1;
                        let tool_name = tc.function.name.clone();
                        let args = tc.function.arguments.clone();
                        let out = if let Some(off) = tool_name_to_offset.get(&tool_name).copied() {
                            match tool_exec(off, &args) {
                                Ok(s) => s,
                                Err(rc) => json!({"error": {"code": "tool_exec_failed", "message": format!("tool rc: {}", rc)}}).to_string(),
                            }
                        } else if tool_name.starts_with(mcp_keys::tool::NAMESPACE_PREFIX_DOT)
                            || tool_name
                                .starts_with(mcp_keys::tool::NAMESPACE_PREFIX_DBL_UNDERSCORE)
                        {
                            match self.cchat_exec_mcp_tool(&snapshot, &tool_name, &args) {
                                Ok(s) => s,
                                Err(msg) => {
                                    json!({"error": {"code": "mcp_tool_failed", "message": msg}})
                                        .to_string()
                                }
                            }
                        } else {
                            json!({"error": {"code": "unknown_tool", "message": format!("unknown tool: {}", tool_name)}}).to_string()
                        };
                        let _ = self.cchat_append_message(
                            fd,
                            ChatMessage {
                                role: "tool".to_string(),
                                content: serde_json::Value::String(out),
                                tool_call_id: Some(tc.id.clone()),
                                tool_calls: None,
                                name: None,
                            },
                        );
                    }

                    iter += 1;
                    continue;
                }
            }
        }
    }

    pub fn cchat_recv(&self, response_fd: i32) -> Result<Vec<u8>, i32> {
        let Some(entry) = self.fd_table.get(response_fd) else {
            return Err(-EBADF);
        };
        let e = match entry.lock() {
            Ok(v) => v,
            Err(_) => return Err(-EIO),
        };
        let FdInner::ChatResponse(r) = &e.inner else {
            return Err(-EBADF);
        };
        Ok(r.bytes.clone())
    }

    pub fn cchat_close(&self, fd: i32) -> i32 {
        self.fd_table.close(fd)
    }

    fn cchat_get_session_snapshot(&self, fd: i32) -> Result<ChatSessionSnapshot, i32> {
        let Some(entry) = self.fd_table.get(fd) else {
            return Err(-EBADF);
        };
        let e = entry.lock().map_err(|_| -EIO)?;
        if e.closed {
            return Err(-EBADF);
        }
        let FdInner::ChatSession(s) = &e.inner else {
            return Err(-EBADF);
        };
        let mut params = s.params.clone();
        s.mcp.materialize_into(&mut params);
        Ok(ChatSessionSnapshot {
            fd,
            messages: s.messages.clone(),
            tools: s.tools.clone(),
            params,
            mcp: s.mcp.clone(),
        })
    }

    fn cchat_put_response(
        &self,
        resp_fd: i32,
        bytes: Vec<u8>,
        metrics_bytes: Vec<u8>,
    ) -> Result<(), i32> {
        let Some(entry) = self.fd_table.get(resp_fd) else {
            return Err(-EBADF);
        };
        {
            let mut e = entry.lock().map_err(|_| -EIO)?;
            let FdInner::ChatResponse(r) = &mut e.inner else {
                return Err(-EBADF);
            };
            r.bytes = bytes;
            r.metrics_bytes = metrics_bytes;
            e.poll_mask.insert(PollEvents::IN);
        }
        self.fd_table.notify_watchers(resp_fd);
        Ok(())
    }

    fn cchat_inject_mcp_tools(&self, snapshot: &ChatSessionSnapshot) -> ChatSessionSnapshot {
        let session = &snapshot.mcp;
        if !session.is_effectively_enabled() {
            return snapshot.clone();
        }

        let Some(sync) = self.mcp_registry_sync.as_ref() else {
            tracing::debug!(
                chat_fd = snapshot.fd,
                server_ids = ?session.server_ids,
                "mcp tool injection skipped: registry sync unavailable"
            );
            return snapshot.clone();
        };

        let reg = sync.cache().snapshot();
        let mut out = snapshot.clone();
        let mut appended: Vec<(i32, String)> = Vec::new();

        for sid in session.server_ids.iter() {
            let Some(server) = reg.servers.iter().find(|s| s.server_id == *sid) else {
                tracing::debug!(
                    chat_fd = snapshot.fd,
                    server_id = sid,
                    known_servers = reg.servers.len(),
                    "mcp tool injection skipped: unknown server_id"
                );
                continue;
            };

            let allowed = server_allowed_tools(server);
            if allowed.is_empty() {
                tracing::debug!(
                    chat_fd = snapshot.fd,
                    server_id = sid,
                    "mcp tool injection skipped: empty server allowed_tools"
                );
                continue;
            }

            let timeout_ms = server
                .budgets
                .as_ref()
                .map(|b| b.tool_timeout_ms)
                .unwrap_or(12000)
                .max(100)
                .min(180_000);
            let tools_res = self.block_on(async {
                tokio::time::timeout(
                    Duration::from_millis(timeout_ms),
                    crate::spearlet::mcp::client::list_tools(server),
                )
                .await
            });

            let tools = match tools_res {
                Ok(Ok(t)) => t,
                Ok(Err(e)) => {
                    tracing::debug!(
                        chat_fd = snapshot.fd,
                        server_id = sid,
                        error = %e,
                        "mcp tool injection: list_tools failed"
                    );
                    Vec::new()
                }
                Err(_) => {
                    tracing::debug!(
                        chat_fd = snapshot.fd,
                        server_id = sid,
                        timeout_ms,
                        "mcp tool injection: list_tools timeout"
                    );
                    Vec::new()
                }
            };

            let filtered = filter_and_namespace_openai_tools(sid, &allowed, session, &tools);
            if filtered.is_empty() {
                tracing::debug!(
                    chat_fd = snapshot.fd,
                    server_id = sid,
                    tools_count = tools.len(),
                    allowlist = ?session.tool_allowlist,
                    denylist = ?session.tool_denylist,
                    task_allowlist = ?session.task_tool_allowlist,
                    task_denylist = ?session.task_tool_denylist,
                    "mcp tool injection produced no tools after filtering"
                );
            }
            for s in filtered.into_iter() {
                appended.push((0, s));
            }
        }

        tracing::debug!(
            chat_fd = snapshot.fd,
            server_ids = ?session.server_ids,
            injected_tools = appended.len(),
            "mcp tool injection completed"
        );
        out.tools.extend(appended);
        out
    }

    fn cchat_exec_mcp_tool(
        &self,
        snapshot: &ChatSessionSnapshot,
        namespaced: &str,
        args: &str,
    ) -> Result<String, String> {
        let Some(sync) = self.mcp_registry_sync.as_ref() else {
            return Err("mcp registry not available".to_string());
        };

        let (server_id, _) =
            crate::spearlet::mcp::policy::parse_namespaced_mcp_tool_name(namespaced)?;
        let reg = sync.cache().snapshot();
        let server = reg
            .servers
            .iter()
            .find(|s| s.server_id == server_id)
            .ok_or_else(|| "unknown mcp server".to_string())?
            .clone();

        let decision = decide_mcp_exec(&snapshot.mcp, &snapshot.params, &server, namespaced)?;

        let out = self.block_on(async {
            tokio::time::timeout(
                Duration::from_millis(decision.timeout_ms),
                crate::spearlet::mcp::client::call_tool(&server, &decision.tool_name, args),
            )
            .await
        });

        let v = match out {
            Ok(Ok(v)) => v,
            Ok(Err(e)) => return Err(e),
            Err(_) => return Err("mcp tool timeout".to_string()),
        };
        let mut s = serde_json::to_string(&v).map_err(|e| e.to_string())?;
        if s.as_bytes().len() > decision.max_tool_output_bytes as usize {
            s.truncate(decision.max_tool_output_bytes as usize);
        }
        Ok(s)
    }
}

fn build_tool_name_to_offset(tools: &[(i32, String)]) -> HashMap<String, i32> {
    let mut m: HashMap<String, i32> = HashMap::new();
    for (off, s) in tools.iter() {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(s) else {
            continue;
        };
        let name = v
            .get("function")
            .and_then(|x| x.get("name"))
            .and_then(|x| x.as_str())
            .or_else(|| v.get("name").and_then(|x| x.as_str()));
        let Some(name) = name else {
            continue;
        };
        if name.is_empty() {
            continue;
        }
        m.entry(name.to_string()).or_insert(*off);
    }
    m
}

fn parse_openai_tool_calls(v: &serde_json::Value) -> Option<(ChatMessage, Vec<ToolCall>)> {
    let msg = v
        .get("choices")
        .and_then(|x| x.get(0))
        .and_then(|x| x.get("message"))?;
    let tool_calls_val = msg.get("tool_calls")?;
    let calls: Vec<ToolCall> = serde_json::from_value(tool_calls_val.clone()).ok()?;
    if calls.is_empty() {
        return None;
    }
    let content = msg
        .get("content")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let role = msg
        .get("role")
        .and_then(|x| x.as_str())
        .unwrap_or("assistant")
        .to_string();
    Some((
        ChatMessage {
            role,
            content,
            tool_call_id: None,
            tool_calls: Some(calls.clone()),
            name: msg
                .get("name")
                .and_then(|x| x.as_str())
                .map(|s| s.to_string()),
        },
        calls,
    ))
}

fn extract_openai_assistant_message(v: &serde_json::Value) -> Option<ChatMessage> {
    let msg = v
        .get("choices")
        .and_then(|x| x.get(0))
        .and_then(|x| x.get("message"))?;
    let role = msg
        .get("role")
        .and_then(|x| x.as_str())
        .unwrap_or("assistant")
        .to_string();
    let content = msg
        .get("content")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    Some(ChatMessage {
        role,
        content,
        tool_call_id: None,
        tool_calls: None,
        name: msg
            .get("name")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string()),
    })
}
