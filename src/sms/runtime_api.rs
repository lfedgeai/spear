//! Shared runtime admin API mapping helpers for SMS.
//! SMS 的运行时管理 API 共享映射辅助模块。
//!
//! This module centralizes instance/execution presentation rules so Web Admin
//! handlers can stay focused on orchestration instead of JSON assembly.
//! 此模块集中管理 instance/execution 的展示规则，让 Web Admin handler 专注于流程编排，而不是手工拼 JSON。

use serde::Serialize;
use std::collections::HashMap;

use crate::proto::sms::{Execution, ExecutionSummary, Instance, InstanceSummary, LogRef};

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AdminTaskInstanceRow {
    pub(crate) instance_id: String,
    pub(crate) task_id: String,
    pub(crate) node_uuid: String,
    pub(crate) status: String,
    pub(crate) created_at_ms: i64,
    pub(crate) updated_at_ms: i64,
    pub(crate) last_seen_ms: i64,
    pub(crate) current_execution_id: String,
}

pub(crate) fn task_instance_row(
    summary: InstanceSummary,
    detail: Option<&Instance>,
) -> AdminTaskInstanceRow {
    let summary_status = crate::sms::instance_status_to_public_str(summary.status).to_string();
    AdminTaskInstanceRow {
        instance_id: summary.instance_id,
        task_id: detail.map(|inst| inst.task_id.clone()).unwrap_or_default(),
        node_uuid: summary.node_uuid,
        status: detail
            .map(|inst| crate::sms::instance_status_to_public_str(inst.status).to_string())
            .unwrap_or(summary_status),
        created_at_ms: detail.map(|inst| inst.created_at_ms).unwrap_or_default(),
        updated_at_ms: detail.map(|inst| inst.updated_at_ms).unwrap_or_default(),
        last_seen_ms: detail
            .map(|inst| inst.last_seen_ms)
            .unwrap_or(summary.last_seen_ms),
        current_execution_id: detail
            .map(|inst| inst.current_execution_id.clone())
            .unwrap_or(summary.current_execution_id),
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AdminTaskInstanceListResponse {
    pub(crate) success: bool,
    pub(crate) instances: Vec<AdminTaskInstanceRow>,
    pub(crate) next_page_token: String,
    pub(crate) message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AdminInstanceDetail {
    pub(crate) instance_id: String,
    pub(crate) task_id: String,
    pub(crate) node_uuid: String,
    pub(crate) status: String,
    pub(crate) created_at_ms: i64,
    pub(crate) updated_at_ms: i64,
    pub(crate) last_seen_ms: i64,
    pub(crate) current_execution_id: String,
    pub(crate) metadata: HashMap<String, String>,
}

pub(crate) fn instance_to_detail(instance: Instance) -> AdminInstanceDetail {
    AdminInstanceDetail {
        instance_id: instance.instance_id,
        task_id: instance.task_id,
        node_uuid: instance.node_uuid,
        status: crate::sms::instance_status_to_public_str(instance.status).to_string(),
        created_at_ms: instance.created_at_ms,
        updated_at_ms: instance.updated_at_ms,
        last_seen_ms: instance.last_seen_ms,
        current_execution_id: instance.current_execution_id,
        metadata: instance.metadata,
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AdminInstanceDetailEnvelope {
    pub(crate) success: bool,
    pub(crate) found: bool,
    pub(crate) active: bool,
    pub(crate) instance: Option<AdminInstanceDetail>,
    pub(crate) message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AdminExecutionSummaryRow {
    pub(crate) execution_id: String,
    pub(crate) task_id: String,
    pub(crate) status: String,
    pub(crate) started_at_ms: i64,
    pub(crate) completed_at_ms: i64,
    pub(crate) function_name: String,
}

pub(crate) fn execution_summary_to_row(execution: ExecutionSummary) -> AdminExecutionSummaryRow {
    AdminExecutionSummaryRow {
        execution_id: execution.execution_id,
        task_id: execution.task_id,
        status: crate::sms::execution_status_to_public_str(execution.status).to_string(),
        started_at_ms: execution.started_at_ms,
        completed_at_ms: execution.completed_at_ms,
        function_name: execution.function_name,
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AdminExecutionSummaryListResponse {
    pub(crate) success: bool,
    pub(crate) executions: Vec<AdminExecutionSummaryRow>,
    pub(crate) next_page_token: String,
    pub(crate) message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AdminExecutionHistoryRow {
    pub(crate) execution_id: String,
    pub(crate) invocation_id: String,
    pub(crate) task_id: String,
    pub(crate) function_name: String,
    pub(crate) node_uuid: String,
    pub(crate) instance_id: String,
    pub(crate) status: String,
    pub(crate) started_at_ms: i64,
    pub(crate) completed_at_ms: i64,
    pub(crate) updated_at_ms: i64,
}

pub(crate) fn execution_to_history_row(execution: Execution) -> AdminExecutionHistoryRow {
    AdminExecutionHistoryRow {
        execution_id: execution.execution_id,
        invocation_id: execution.invocation_id,
        task_id: execution.task_id,
        function_name: execution.function_name,
        node_uuid: execution.node_uuid,
        instance_id: execution.instance_id,
        status: crate::sms::execution_status_to_public_str(execution.status).to_string(),
        started_at_ms: execution.started_at_ms,
        completed_at_ms: execution.completed_at_ms,
        updated_at_ms: execution.updated_at_ms,
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AdminExecutionHistoryListResponse {
    pub(crate) success: bool,
    pub(crate) executions: Vec<AdminExecutionHistoryRow>,
    pub(crate) next_page_token: String,
    pub(crate) message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AdminLogRef {
    pub(crate) backend: String,
    pub(crate) uri_prefix: String,
    pub(crate) content_type: String,
    pub(crate) compression: String,
}

fn log_ref_to_response(log_ref: LogRef) -> AdminLogRef {
    AdminLogRef {
        backend: log_ref.backend,
        uri_prefix: log_ref.uri_prefix,
        content_type: log_ref.content_type,
        compression: log_ref.compression,
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AdminExecutionDetail {
    pub(crate) execution_id: String,
    pub(crate) invocation_id: String,
    pub(crate) task_id: String,
    pub(crate) function_name: String,
    pub(crate) node_uuid: String,
    pub(crate) instance_id: String,
    pub(crate) status: String,
    pub(crate) started_at_ms: i64,
    pub(crate) completed_at_ms: i64,
    pub(crate) updated_at_ms: i64,
    pub(crate) metadata: HashMap<String, String>,
    pub(crate) log_ref: Option<AdminLogRef>,
}

pub(crate) fn execution_to_detail(execution: Execution) -> AdminExecutionDetail {
    AdminExecutionDetail {
        execution_id: execution.execution_id,
        invocation_id: execution.invocation_id,
        task_id: execution.task_id,
        function_name: execution.function_name,
        node_uuid: execution.node_uuid,
        instance_id: execution.instance_id,
        status: crate::sms::execution_status_to_public_str(execution.status).to_string(),
        started_at_ms: execution.started_at_ms,
        completed_at_ms: execution.completed_at_ms,
        updated_at_ms: execution.updated_at_ms,
        metadata: execution.metadata,
        log_ref: execution.log_ref.map(log_ref_to_response),
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AdminExecutionDetailEnvelope {
    pub(crate) success: bool,
    pub(crate) found: bool,
    pub(crate) execution: Option<AdminExecutionDetail>,
    pub(crate) message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AdminExecutionMutationResponse {
    pub(crate) success: bool,
    pub(crate) node_uuid: Option<String>,
    pub(crate) execution_id: Option<String>,
    pub(crate) final_status: Option<String>,
    pub(crate) message: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AdminInstanceMutationResponse {
    pub(crate) success: bool,
    pub(crate) node_uuid: Option<String>,
    pub(crate) instance_id: Option<String>,
    pub(crate) message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_instance_row_prefers_detail_fields() {
        let summary = InstanceSummary {
            instance_id: "inst-1".to_string(),
            node_uuid: "node-1".to_string(),
            status: 0,
            last_seen_ms: 11,
            current_execution_id: "from-summary".to_string(),
        };
        let detail = Instance {
            instance_id: "inst-1".to_string(),
            task_id: "task-1".to_string(),
            node_uuid: "node-1".to_string(),
            status: 2,
            created_at_ms: 20,
            updated_at_ms: 30,
            last_seen_ms: 40,
            current_execution_id: "from-detail".to_string(),
            metadata: HashMap::from([("owner".to_string(), "team-a".to_string())]),
        };

        let row = task_instance_row(summary, Some(&detail));
        assert_eq!(row.task_id, "task-1");
        assert_eq!(row.created_at_ms, 20);
        assert_eq!(row.current_execution_id, "from-detail");
    }

    #[test]
    fn execution_detail_keeps_log_reference() {
        let detail = execution_to_detail(Execution {
            execution_id: "exe-1".to_string(),
            invocation_id: "inv-1".to_string(),
            task_id: "task-1".to_string(),
            function_name: "run".to_string(),
            node_uuid: "node-1".to_string(),
            instance_id: "inst-1".to_string(),
            status: 1,
            started_at_ms: 10,
            completed_at_ms: 20,
            updated_at_ms: 30,
            metadata: HashMap::from([("trace".to_string(), "1".to_string())]),
            log_ref: Some(LogRef {
                backend: "s3".to_string(),
                uri_prefix: "s3://bucket/prefix".to_string(),
                content_type: "text/plain".to_string(),
                compression: "gzip".to_string(),
            }),
        });

        assert_eq!(detail.execution_id, "exe-1");
        assert_eq!(
            detail
                .log_ref
                .as_ref()
                .map(|log_ref| log_ref.backend.as_str()),
            Some("s3")
        );
    }
}
