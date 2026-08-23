//! Shared admin API mapping helpers for SMS.
//! SMS 的管理端 API 共享映射辅助模块。
//!
//! This module centralizes typed response models used by Web Admin handlers
//! so endpoint implementations can focus on orchestration and validation.
//! 此模块集中管理 Web Admin handler 使用的类型化响应模型，让接口实现专注于编排与校验。

use serde::Serialize;
use std::collections::HashMap;

use crate::proto::sms::{CredentialInfo, McpServerRecord};

fn empty_string_as_optional(value: String) -> Option<String> {
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AdminCredentialInfoResponse {
    pub(crate) name: String,
    pub(crate) provider_kind: String,
    pub(crate) version: u64,
    pub(crate) description: Option<String>,
    pub(crate) disabled: bool,
    pub(crate) created_at_ms: i64,
    pub(crate) updated_at_ms: i64,
    pub(crate) referenced_by_count: usize,
}

pub(crate) fn credential_info_to_response(
    credential: CredentialInfo,
    referenced_by_count: usize,
) -> AdminCredentialInfoResponse {
    AdminCredentialInfoResponse {
        name: credential.name,
        provider_kind: credential.provider_kind,
        version: credential.version,
        description: empty_string_as_optional(credential.description),
        disabled: credential.disabled,
        created_at_ms: credential.created_at_ms,
        updated_at_ms: credential.updated_at_ms,
        referenced_by_count,
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AdminCredentialListResponse {
    pub(crate) success: bool,
    pub(crate) revision: u64,
    pub(crate) credentials: Vec<AdminCredentialInfoResponse>,
    pub(crate) message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AdminCredentialMutationResponse {
    pub(crate) success: bool,
    pub(crate) revision: u64,
    pub(crate) deleted: Option<bool>,
    pub(crate) message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AdminMcpStdioResponse {
    pub(crate) command: String,
    pub(crate) args: Vec<String>,
    pub(crate) env: HashMap<String, String>,
    pub(crate) cwd: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AdminMcpHttpResponse {
    pub(crate) url: String,
    pub(crate) headers: HashMap<String, String>,
    pub(crate) auth_ref: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AdminMcpApprovalPolicyResponse {
    pub(crate) default_policy: String,
    pub(crate) per_tool: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AdminMcpBudgetsResponse {
    pub(crate) tool_timeout_ms: u64,
    pub(crate) max_concurrency: u64,
    pub(crate) max_tool_output_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AdminMcpServerResponse {
    pub(crate) server_id: String,
    pub(crate) display_name: String,
    pub(crate) transport: i32,
    pub(crate) stdio: Option<AdminMcpStdioResponse>,
    pub(crate) http: Option<AdminMcpHttpResponse>,
    pub(crate) tool_namespace: String,
    pub(crate) allowed_tools: Vec<String>,
    pub(crate) approval_policy: Option<AdminMcpApprovalPolicyResponse>,
    pub(crate) budgets: Option<AdminMcpBudgetsResponse>,
    pub(crate) updated_at_ms: u64,
}

pub(crate) fn mcp_server_to_response(record: McpServerRecord) -> AdminMcpServerResponse {
    AdminMcpServerResponse {
        server_id: record.server_id,
        display_name: record.display_name,
        transport: record.transport,
        stdio: record.stdio.map(|stdio| AdminMcpStdioResponse {
            command: stdio.command,
            args: stdio.args,
            env: stdio.env,
            cwd: stdio.cwd,
        }),
        http: record.http.map(|http| AdminMcpHttpResponse {
            url: http.url,
            headers: http.headers,
            auth_ref: http.auth_ref,
        }),
        tool_namespace: record.tool_namespace,
        allowed_tools: record.allowed_tools,
        approval_policy: record
            .approval_policy
            .map(|policy| AdminMcpApprovalPolicyResponse {
                default_policy: policy.default_policy,
                per_tool: policy.per_tool,
            }),
        budgets: record.budgets.map(|budget| AdminMcpBudgetsResponse {
            tool_timeout_ms: budget.tool_timeout_ms,
            max_concurrency: budget.max_concurrency,
            max_tool_output_bytes: budget.max_tool_output_bytes,
        }),
        updated_at_ms: record.updated_at_ms,
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AdminMcpServerListResponse {
    pub(crate) success: bool,
    pub(crate) revision: u64,
    pub(crate) servers: Vec<AdminMcpServerResponse>,
    pub(crate) message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AdminMcpServerDetailResponse {
    pub(crate) success: bool,
    pub(crate) found: bool,
    pub(crate) server: Option<AdminMcpServerResponse>,
    pub(crate) message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AdminRevisionMutationResponse {
    pub(crate) success: bool,
    pub(crate) revision: u64,
    pub(crate) message: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::sms::{
        McpApprovalPolicy, McpBudgets, McpHttpConfig, McpStdioConfig, McpTransport,
    };

    #[test]
    fn credential_mapping_keeps_reference_count() {
        let response = credential_info_to_response(
            CredentialInfo {
                name: "cred-a".to_string(),
                provider_kind: "openai".to_string(),
                version: 1,
                description: "desc".to_string(),
                disabled: false,
                created_at_ms: 10,
                updated_at_ms: 20,
            },
            3,
        );

        assert_eq!(response.name, "cred-a");
        assert_eq!(response.referenced_by_count, 3);
        assert_eq!(response.description.as_deref(), Some("desc"));
    }

    #[test]
    fn mcp_server_mapping_keeps_nested_fields() {
        let response = mcp_server_to_response(McpServerRecord {
            server_id: "srv-1".to_string(),
            display_name: "server".to_string(),
            transport: McpTransport::Stdio as i32,
            stdio: Some(McpStdioConfig {
                command: "node".to_string(),
                args: vec!["mcp.js".to_string()],
                env: HashMap::from([("ENV".to_string(), "prod".to_string())]),
                cwd: "/tmp".to_string(),
            }),
            http: Some(McpHttpConfig {
                url: "https://example.com/mcp".to_string(),
                headers: HashMap::from([("Authorization".to_string(), "Bearer token".to_string())]),
                auth_ref: "auth-a".to_string(),
            }),
            tool_namespace: "tools".to_string(),
            allowed_tools: vec!["tool-a".to_string()],
            approval_policy: Some(McpApprovalPolicy {
                default_policy: "auto".to_string(),
                per_tool: HashMap::from([("tool-a".to_string(), "manual".to_string())]),
            }),
            budgets: Some(McpBudgets {
                tool_timeout_ms: 1000,
                max_concurrency: 2,
                max_tool_output_bytes: 4096,
            }),
            updated_at_ms: 30,
        });

        assert_eq!(response.server_id, "srv-1");
        assert_eq!(
            response
                .stdio
                .as_ref()
                .and_then(|stdio| stdio.env.get("ENV"))
                .map(String::as_str),
            Some("prod")
        );
        assert_eq!(
            response
                .approval_policy
                .as_ref()
                .and_then(|policy| policy.per_tool.get("tool-a"))
                .map(String::as_str),
            Some("manual")
        );
    }
}
