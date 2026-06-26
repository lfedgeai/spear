use std::time::{SystemTime, UNIX_EPOCH};

use tonic::Status;

use crate::proto::sms::{McpRegistryEvent, McpServerRecord, McpTransport};
use crate::sms::registry::state::McpRegistryState;

/// Validate and normalize one MCP registry record before persistence.
/// 在持久化前校验并规范化一条 MCP registry 记录。
fn normalize_mcp_record(mut record: McpServerRecord) -> Result<(String, McpServerRecord), Status> {
    if record.server_id.is_empty() {
        return Err(Status::invalid_argument("server_id is required"));
    }
    let server_id = record.server_id.clone();
    if record.tool_namespace.is_empty() {
        record.tool_namespace = format!("mcp.{}", record.server_id);
    }

    match record.transport {
        x if x == McpTransport::Stdio as i32 => {
            let stdio = record
                .stdio
                .as_ref()
                .ok_or_else(|| Status::invalid_argument("stdio config is required"))?;
            if stdio.command.is_empty() {
                return Err(Status::invalid_argument("stdio.command is required"));
            }
        }
        x if x == McpTransport::StreamableHttp as i32 => {
            let http = record
                .http
                .as_ref()
                .ok_or_else(|| Status::invalid_argument("http config is required"))?;
            if http.url.is_empty() {
                return Err(Status::invalid_argument("http.url is required"));
            }
        }
        _ => {
            return Err(Status::invalid_argument("invalid transport"));
        }
    }

    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    record.updated_at_ms = now_ms;
    Ok((server_id, record))
}

/// Persist one MCP record and emit the matching registry event.
/// 持久化一条 MCP 记录并发送对应 registry 事件。
pub async fn upsert_mcp_record(
    state: &McpRegistryState,
    record: McpServerRecord,
) -> Result<u64, Status> {
    let (server_id, record) = normalize_mcp_record(record)?;

    {
        let mut records = state.records.write().await;
        records.insert(server_id.clone(), record);
    }

    let revision = state.bump_revision();
    state
        .push_event(McpRegistryEvent {
            revision,
            upserts: vec![server_id],
            deletes: vec![],
        })
        .await;

    Ok(revision)
}

/// Remove one MCP record and emit the corresponding delete event.
/// 删除一条 MCP 记录并发送对应删除事件。
pub async fn delete_mcp_record(state: &McpRegistryState, server_id: String) -> Result<u64, Status> {
    if server_id.is_empty() {
        return Err(Status::invalid_argument("server_id is required"));
    }

    let existed = {
        let mut records = state.records.write().await;
        records.remove(&server_id).is_some()
    };
    if !existed {
        return Err(Status::not_found("server not found"));
    }

    let revision = state.bump_revision();
    state
        .push_event(McpRegistryEvent {
            revision,
            upserts: vec![],
            deletes: vec![server_id],
        })
        .await;
    Ok(revision)
}

/// Snapshot current MCP registry records with the current revision.
/// 获取当前 MCP registry 记录快照及其 revision。
pub async fn list_mcp_records(state: &McpRegistryState) -> (u64, Vec<McpServerRecord>) {
    let revision = state.current_revision();
    let records = state.records.read().await;
    let servers = records.values().cloned().collect::<Vec<_>>();
    (revision, servers)
}
