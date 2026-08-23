use axum::{extract::Path, Json};
use serde::Deserialize;

use crate::proto::sms::{DeleteCredentialRequest, ListCredentialsRequest, UpsertCredentialRequest};
use crate::sms::admin_api::{
    credential_info_to_response, mcp_server_to_response, AdminCredentialListResponse,
    AdminCredentialMutationResponse, AdminMcpServerDetailResponse, AdminMcpServerListResponse,
    AdminRevisionMutationResponse,
};
use crate::sms::gateway::GatewayState;

#[derive(Deserialize)]
pub(crate) struct McpServerUpsertBody {
    pub(crate) server_id: String,
    pub(crate) display_name: Option<String>,
    pub(crate) transport: String,
    pub(crate) stdio: Option<McpStdioBody>,
    pub(crate) http: Option<McpHttpBody>,
    pub(crate) tool_namespace: Option<String>,
    pub(crate) allowed_tools: Option<Vec<String>>,
    pub(crate) budgets: Option<McpBudgetsBody>,
    pub(crate) approval_policy: Option<McpApprovalPolicyBody>,
}

#[derive(Deserialize)]
pub(crate) struct McpStdioBody {
    command: String,
    args: Option<Vec<String>>,
    env: Option<std::collections::HashMap<String, String>>,
    cwd: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct McpHttpBody {
    url: String,
    headers: Option<std::collections::HashMap<String, String>>,
    auth_ref: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct UpsertCredentialBody {
    pub(crate) name: String,
    pub(crate) secret: Option<String>,
    pub(crate) description: Option<String>,
    pub(crate) disabled: Option<bool>,
}

#[derive(Deserialize)]
pub(crate) struct McpBudgetsBody {
    tool_timeout_ms: Option<u64>,
    max_concurrency: Option<u64>,
    max_tool_output_bytes: Option<u64>,
}

#[derive(Deserialize)]
pub(crate) struct McpApprovalPolicyBody {
    default_policy: Option<String>,
    per_tool: Option<std::collections::HashMap<String, String>>,
}

pub(crate) async fn list_credentials_admin(
    state: GatewayState,
) -> Json<AdminCredentialListResponse> {
    let mut client = state.admin_credential_client.clone();
    match client.list_credentials(ListCredentialsRequest {}).await {
        Ok(resp) => {
            let inner = resp.into_inner();
            let mut backend_client = state.ai_backend_control_plane_client.clone();
            let referenced_by_count = match backend_client
                .list_ai_backends(crate::proto::sms::ListAiBackendsRequest {
                    limit: 0,
                    offset: 0,
                    q: String::new(),
                    hosting: String::new(),
                    desired_state: String::new(),
                    provider: String::new(),
                    model: String::new(),
                })
                .await
            {
                Ok(resp) => {
                    let mut counts = std::collections::HashMap::<String, usize>::new();
                    for backend in resp.into_inner().backends {
                        let credential_ref = backend.credential_ref.as_deref().unwrap_or("").trim();
                        if credential_ref.is_empty() {
                            continue;
                        }
                        *counts.entry(credential_ref.to_string()).or_insert(0) += 1;
                    }
                    counts
                }
                Err(_) => std::collections::HashMap::new(),
            };
            let credentials = inner
                .credentials
                .into_iter()
                .map(|credential| {
                    let referenced = referenced_by_count
                        .get(&credential.name)
                        .copied()
                        .unwrap_or(0);
                    credential_info_to_response(credential, referenced)
                })
                .collect::<Vec<_>>();
            Json(AdminCredentialListResponse {
                success: true,
                revision: inner.revision,
                credentials,
                message: None,
            })
        }
        Err(error) => Json(AdminCredentialListResponse {
            success: false,
            revision: 0,
            credentials: Vec::new(),
            message: Some(error.to_string()),
        }),
    }
}

pub(crate) async fn upsert_credential_admin(
    state: GatewayState,
    body: Json<UpsertCredentialBody>,
) -> Json<AdminCredentialMutationResponse> {
    let body = body.0;
    if body.name.trim().is_empty() {
        return Json(AdminCredentialMutationResponse {
            success: false,
            revision: 0,
            deleted: None,
            message: Some("name is required".to_string()),
        });
    }
    let mut client = state.admin_credential_client.clone();
    let response = match client
        .upsert_credential(UpsertCredentialRequest {
            name: body.name,
            secret: body.secret.unwrap_or_default(),
            description: body.description.unwrap_or_default(),
            disabled: body.disabled.unwrap_or(false),
        })
        .await
    {
        Ok(response) => response.into_inner(),
        Err(error) => {
            return Json(AdminCredentialMutationResponse {
                success: false,
                revision: 0,
                deleted: None,
                message: Some(error.to_string()),
            })
        }
    };
    Json(AdminCredentialMutationResponse {
        success: true,
        revision: response.revision,
        deleted: None,
        message: None,
    })
}

pub(crate) async fn delete_credential_admin(
    state: GatewayState,
    path: Path<String>,
) -> Json<AdminCredentialMutationResponse> {
    let name = path.0;
    if name.trim().is_empty() {
        return Json(AdminCredentialMutationResponse {
            success: false,
            revision: 0,
            deleted: None,
            message: Some("name is required".to_string()),
        });
    }
    let mut client = state.admin_credential_client.clone();
    let response = match client
        .delete_credential(DeleteCredentialRequest { name })
        .await
    {
        Ok(response) => response.into_inner(),
        Err(error) => {
            return Json(AdminCredentialMutationResponse {
                success: false,
                revision: 0,
                deleted: None,
                message: Some(error.to_string()),
            })
        }
    };
    Json(AdminCredentialMutationResponse {
        success: true,
        revision: response.revision,
        deleted: Some(response.deleted),
        message: None,
    })
}

pub(crate) async fn list_mcp_servers(state: GatewayState) -> Json<AdminMcpServerListResponse> {
    let mut client = state.mcp_registry_client.clone();
    match client
        .list_mcp_servers(crate::proto::sms::ListMcpServersRequest { since_revision: 0 })
        .await
    {
        Ok(response) => {
            let inner = response.into_inner();
            let servers = inner
                .servers
                .into_iter()
                .map(mcp_server_to_response)
                .collect::<Vec<_>>();
            Json(AdminMcpServerListResponse {
                success: true,
                revision: inner.revision,
                servers,
                message: None,
            })
        }
        Err(error) => Json(AdminMcpServerListResponse {
            success: false,
            revision: 0,
            servers: Vec::new(),
            message: Some(error.to_string()),
        }),
    }
}

pub(crate) async fn get_mcp_server(
    state: GatewayState,
    path: Path<String>,
) -> Json<AdminMcpServerDetailResponse> {
    let server_id = path.0;
    if server_id.trim().is_empty() {
        return Json(AdminMcpServerDetailResponse {
            success: true,
            found: false,
            server: None,
            message: None,
        });
    }
    let mut client = state.mcp_registry_client.clone();
    let response = match client
        .list_mcp_servers(crate::proto::sms::ListMcpServersRequest { since_revision: 0 })
        .await
    {
        Ok(response) => response.into_inner(),
        Err(error) => {
            return Json(AdminMcpServerDetailResponse {
                success: false,
                found: false,
                server: None,
                message: Some(error.to_string()),
            })
        }
    };

    for server in response.servers {
        if server.server_id != server_id {
            continue;
        }
        return Json(AdminMcpServerDetailResponse {
            success: true,
            found: true,
            server: Some(mcp_server_to_response(server)),
            message: None,
        });
    }

    Json(AdminMcpServerDetailResponse {
        success: true,
        found: false,
        server: None,
        message: None,
    })
}

pub(crate) async fn upsert_mcp_server(
    state: GatewayState,
    body: Json<McpServerUpsertBody>,
) -> Json<AdminRevisionMutationResponse> {
    use crate::proto::sms::{
        McpApprovalPolicy, McpBudgets, McpHttpConfig, McpServerRecord, McpStdioConfig, McpTransport,
    };

    let body = body.0;
    if body.server_id.trim().is_empty() {
        return Json(AdminRevisionMutationResponse {
            success: false,
            revision: 0,
            message: Some("server_id is required".to_string()),
        });
    }

    let transport = match body.transport.as_str() {
        "stdio" => McpTransport::Stdio as i32,
        "streamable_http" => McpTransport::StreamableHttp as i32,
        _ => {
            return Json(AdminRevisionMutationResponse {
                success: false,
                revision: 0,
                message: Some("invalid transport".to_string()),
            });
        }
    };

    let stdio = body.stdio.map(|stdio| McpStdioConfig {
        command: stdio.command,
        args: stdio.args.unwrap_or_default(),
        env: stdio.env.unwrap_or_default(),
        cwd: stdio.cwd.unwrap_or_default(),
    });
    let http = body.http.map(|http| McpHttpConfig {
        url: http.url,
        headers: http.headers.unwrap_or_default(),
        auth_ref: http.auth_ref.unwrap_or_default(),
    });
    let budgets = body.budgets.map(|budgets| McpBudgets {
        tool_timeout_ms: budgets.tool_timeout_ms.unwrap_or(0),
        max_concurrency: budgets.max_concurrency.unwrap_or(0),
        max_tool_output_bytes: budgets.max_tool_output_bytes.unwrap_or(0),
    });
    let approval_policy = body.approval_policy.map(|policy| McpApprovalPolicy {
        default_policy: policy.default_policy.unwrap_or_default(),
        per_tool: policy.per_tool.unwrap_or_default(),
    });

    let record = McpServerRecord {
        server_id: body.server_id.trim().to_string(),
        display_name: body.display_name.unwrap_or_default(),
        transport,
        stdio,
        http,
        tool_namespace: body.tool_namespace.unwrap_or_default(),
        allowed_tools: body.allowed_tools.unwrap_or_default(),
        approval_policy,
        budgets,
        updated_at_ms: 0,
    };

    let mut client = state.mcp_registry_client.clone();
    match client
        .upsert_mcp_server(crate::proto::sms::UpsertMcpServerRequest {
            record: Some(record),
        })
        .await
    {
        Ok(response) => Json(AdminRevisionMutationResponse {
            success: true,
            revision: response.into_inner().revision,
            message: None,
        }),
        Err(error) => Json(AdminRevisionMutationResponse {
            success: false,
            revision: 0,
            message: Some(error.to_string()),
        }),
    }
}

pub(crate) async fn delete_mcp_server(
    state: GatewayState,
    path: Path<String>,
) -> Json<AdminRevisionMutationResponse> {
    let server_id = path.0;
    if server_id.trim().is_empty() {
        return Json(AdminRevisionMutationResponse {
            success: false,
            revision: 0,
            message: Some("server_id is required".to_string()),
        });
    }
    let mut client = state.mcp_registry_client.clone();
    match client
        .delete_mcp_server(crate::proto::sms::DeleteMcpServerRequest {
            server_id: server_id.trim().to_string(),
        })
        .await
    {
        Ok(response) => Json(AdminRevisionMutationResponse {
            success: true,
            revision: response.into_inner().revision,
            message: None,
        }),
        Err(error) => Json(AdminRevisionMutationResponse {
            success: false,
            revision: 0,
            message: Some(error.to_string()),
        }),
    }
}
