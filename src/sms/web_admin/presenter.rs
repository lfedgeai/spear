use serde_json::{json, Value};

use crate::proto::sms::{
    CredentialInfo, Execution, ExecutionSummary, Instance, InstanceSummary, LogRef,
    McpServerRecord, Node, NodeBackendSnapshot, NodeResource,
};

fn empty_string_as_optional(value: String) -> Option<String> {
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

pub(super) fn task_instance_row_json(summary: InstanceSummary, detail: Option<&Instance>) -> Value {
    let summary_status = crate::sms::instance_status_to_public_str(summary.status).to_string();
    json!({
        "instance_id": summary.instance_id,
        "task_id": detail.map(|inst| inst.task_id.clone()).unwrap_or_default(),
        "node_uuid": summary.node_uuid,
        "status": detail
            .map(|inst| crate::sms::instance_status_to_public_str(inst.status).to_string())
            .unwrap_or(summary_status),
        "created_at_ms": detail.map(|inst| inst.created_at_ms).unwrap_or_default(),
        "updated_at_ms": detail.map(|inst| inst.updated_at_ms).unwrap_or_default(),
        "last_seen_ms": detail.map(|inst| inst.last_seen_ms).unwrap_or(summary.last_seen_ms),
        "current_execution_id": detail
            .map(|inst| inst.current_execution_id.clone())
            .unwrap_or(summary.current_execution_id),
    })
}

pub(super) fn instance_json(instance: Instance) -> Value {
    json!({
        "instance_id": instance.instance_id,
        "task_id": instance.task_id,
        "node_uuid": instance.node_uuid,
        "status": crate::sms::instance_status_to_public_str(instance.status),
        "created_at_ms": instance.created_at_ms,
        "updated_at_ms": instance.updated_at_ms,
        "last_seen_ms": instance.last_seen_ms,
        "current_execution_id": instance.current_execution_id,
        "metadata": instance.metadata,
    })
}

pub(super) fn instance_execution_summary_json(execution: ExecutionSummary) -> Value {
    json!({
        "execution_id": execution.execution_id,
        "task_id": execution.task_id,
        "status": crate::sms::execution_status_to_public_str(execution.status),
        "started_at_ms": execution.started_at_ms,
        "completed_at_ms": execution.completed_at_ms,
        "function_name": execution.function_name,
    })
}

pub(super) fn execution_history_row_json(execution: Execution) -> Value {
    json!({
        "execution_id": execution.execution_id,
        "invocation_id": execution.invocation_id,
        "task_id": execution.task_id,
        "function_name": execution.function_name,
        "node_uuid": execution.node_uuid,
        "instance_id": execution.instance_id,
        "status": crate::sms::execution_status_to_public_str(execution.status),
        "started_at_ms": execution.started_at_ms,
        "completed_at_ms": execution.completed_at_ms,
        "updated_at_ms": execution.updated_at_ms,
    })
}

fn log_ref_json(log_ref: LogRef) -> Value {
    json!({
        "backend": log_ref.backend,
        "uri_prefix": log_ref.uri_prefix,
        "content_type": log_ref.content_type,
        "compression": log_ref.compression,
    })
}

pub(super) fn execution_detail_json(execution: Execution) -> Value {
    let log_ref = execution.log_ref.map(log_ref_json);
    json!({
        "execution_id": execution.execution_id,
        "invocation_id": execution.invocation_id,
        "task_id": execution.task_id,
        "function_name": execution.function_name,
        "node_uuid": execution.node_uuid,
        "instance_id": execution.instance_id,
        "status": crate::sms::execution_status_to_public_str(execution.status),
        "started_at_ms": execution.started_at_ms,
        "completed_at_ms": execution.completed_at_ms,
        "updated_at_ms": execution.updated_at_ms,
        "metadata": execution.metadata,
        "log_ref": log_ref,
    })
}

pub(super) fn node_backends_snapshot_json(snapshot: Option<&NodeBackendSnapshot>) -> Value {
    let meta = snapshot.map(|s| {
        json!({
            "revision": s.revision,
            "reported_at_ms": s.reported_at_ms,
        })
    });
    let backends = snapshot
        .map(|s| {
            s.backends
                .iter()
                .filter_map(|b| {
                    b.spec.as_ref().map(|spec| {
                        json!({
                            "name": spec.name,
                            "kind": spec.kind,
                            "operations": spec.operations,
                            "features": spec.features,
                            "transports": spec.transports,
                            "weight": spec.weight,
                            "priority": spec.priority,
                            "base_url": spec.base_url,
                            "status": b.status,
                            "status_reason": b.status_reason,
                            "provider": spec.provider,
                            "model": spec.model,
                            "hosting": spec.hosting,
                        })
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    json!({
        "backends": backends,
        "snapshot": meta,
    })
}

fn node_json(node: Node) -> Value {
    json!({
        "uuid": node.uuid,
        "ip_address": node.ip_address,
        "port": node.port,
        "status": node.status,
        "last_heartbeat": node.last_heartbeat,
        "registered_at": node.registered_at,
        "metadata": node.metadata,
    })
}

fn node_resource_json(resource: NodeResource) -> Value {
    json!({
        "cpu_usage_percent": resource.cpu_usage_percent,
        "memory_usage_percent": resource.memory_usage_percent,
        "disk_usage_percent": resource.disk_usage_percent,
        "total_memory_bytes": resource.total_memory_bytes,
        "used_memory_bytes": resource.used_memory_bytes,
        "available_memory_bytes": resource.available_memory_bytes,
    })
}

pub(super) fn node_detail_json(node: Option<Node>, resource: Option<NodeResource>) -> Value {
    json!({
        "found": node.is_some(),
        "node": node.map(node_json),
        "resource": resource.map(node_resource_json),
    })
}

pub(super) fn credential_info_json(
    credential: CredentialInfo,
    referenced_by_count: usize,
) -> Value {
    json!({
        "name": credential.name,
        "provider_kind": credential.provider_kind,
        "version": credential.version,
        "description": empty_string_as_optional(credential.description),
        "disabled": credential.disabled,
        "created_at_ms": credential.created_at_ms,
        "updated_at_ms": credential.updated_at_ms,
        "referenced_by_count": referenced_by_count,
    })
}

fn mcp_stdio_json(record: &McpServerRecord) -> Option<Value> {
    record.stdio.as_ref().map(|x| {
        json!({
            "command": x.command,
            "args": x.args,
            "env": x.env,
            "cwd": x.cwd,
        })
    })
}

fn mcp_http_json(record: &McpServerRecord) -> Option<Value> {
    record.http.as_ref().map(|x| {
        json!({
            "url": x.url,
            "headers": x.headers,
            "auth_ref": x.auth_ref,
        })
    })
}

fn mcp_approval_policy_json(record: &McpServerRecord) -> Option<Value> {
    record.approval_policy.as_ref().map(|x| {
        json!({
            "default_policy": x.default_policy,
            "per_tool": x.per_tool,
        })
    })
}

fn mcp_budgets_json(record: &McpServerRecord) -> Option<Value> {
    record.budgets.as_ref().map(|x| {
        json!({
            "tool_timeout_ms": x.tool_timeout_ms,
            "max_concurrency": x.max_concurrency,
            "max_tool_output_bytes": x.max_tool_output_bytes,
        })
    })
}

pub(super) fn mcp_server_json(record: McpServerRecord) -> Value {
    json!({
        "server_id": record.server_id,
        "display_name": record.display_name,
        "transport": record.transport,
        "stdio": mcp_stdio_json(&record),
        "http": mcp_http_json(&record),
        "tool_namespace": record.tool_namespace,
        "allowed_tools": record.allowed_tools,
        "approval_policy": mcp_approval_policy_json(&record),
        "budgets": mcp_budgets_json(&record),
        "updated_at_ms": record.updated_at_ms,
    })
}
