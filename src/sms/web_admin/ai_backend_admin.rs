use axum::{
    extract::{Path, Query},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::ai_backend_types::{
    CanonicalBackendDesiredState, CanonicalBackendHosting, CanonicalBackendKind,
    CanonicalBackendManagementMode, CanonicalBackendProvider,
};
use crate::sms::ai_admin_api::{
    backend_assignment_to_response, backend_node_status_to_response, backend_placement_to_response,
    backend_record_to_response, model_view_to_response, AdminAiBackendAssignmentListResponse,
    AdminAiBackendDetailResponse, AdminAiBackendListResponse,
    AdminAiBackendLocalModelPreflightSourceResponse, AdminAiBackendMutationResponse,
    AdminAiBackendNodeStatusListResponse, AdminAiBackendPlacementListResponse,
    AdminAiBackendOllamaPreflightOutcomeResponse,
    AdminAiBackendPlacementMutationResponse, AdminAiBackendPreflightCheckResponse,
    AdminAiBackendOpenAiCompatiblePreflightOutcomeResponse,
    AdminAiBackendPreflightDetailsResponse, AdminAiBackendPreflightNodeResponse,
    AdminAiBackendPreflightResponse, AdminAiBackendRemoteProviderPreflightOutcomeResponse,
    AdminAiBackendRemoteProviderPreflightResultResponse,
    AdminAiModelViewListResponse,
};
use crate::sms::gateway::GatewayState;

use super::types::{AiBackendsQuery, AiModelViewsQuery};

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct AiBackendWriteBody {
    pub(crate) display_name: String,
    pub(crate) provider: String,
    pub(crate) model: String,
    pub(crate) hosting: String,
    pub(crate) backend_kind: String,
    pub(crate) management_mode: Option<String>,
    pub(crate) credential_ref: Option<String>,
    pub(crate) desired_state: Option<String>,
    pub(crate) spec: AiBackendSpecBody,
    pub(crate) labels: Option<std::collections::HashMap<String, String>>,
    pub(crate) local: Option<AiBackendLocalProviderWriteBody>,
    pub(crate) remote: Option<AiBackendRemoteProviderWriteBody>,
    pub(crate) metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct AiBackendSpecBody {
    pub(crate) base_url: Option<String>,
    pub(crate) operations: Vec<String>,
    pub(crate) features: Option<Vec<String>>,
    pub(crate) transports: Option<Vec<String>>,
    pub(crate) weight: Option<u32>,
    pub(crate) priority: Option<i32>,
}

/// Provider-specific local write body accepted at the admin API boundary.
/// 管理端 API 边界接受的 provider-specific local 写入体。
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "provider_family", content = "config", rename_all = "snake_case")]
pub(crate) enum AiBackendLocalProviderWriteBody {
    /// Structured local llama.cpp input body / 结构化 local llama.cpp 输入体
    LlamaCpp(AiBackendLlamaCppLocalWriteBody),
    /// Structured local vLLM input body / 结构化 local vLLM 输入体
    Vllm(AiBackendVllmLocalWriteBody),
    /// Local Ollama input body with no extra structured fields yet / 暂无额外结构化字段的 local Ollama 输入体
    Ollama,
    /// Internal local provider body / 内部 local provider 输入体
    Internal,
}

/// Structured local llama.cpp write body accepted at the admin API boundary.
/// 管理端 API 边界接受的结构化 local llama.cpp 写入体。
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct AiBackendLlamaCppLocalWriteBody {
    pub(crate) model_url: Option<String>,
    pub(crate) model_path: Option<String>,
    pub(crate) skip_download: Option<bool>,
    pub(crate) download_timeout_s: Option<u64>,
    pub(crate) server_mode: Option<String>,
    pub(crate) server_cmd: Option<String>,
    pub(crate) server_cmd_args: Option<String>,
    pub(crate) threads: Option<u32>,
    pub(crate) ctx_size: Option<u32>,
    pub(crate) ready_probe: Option<String>,
    pub(crate) start_timeout_s: Option<u64>,
}

/// Structured local vLLM write body accepted at the admin API boundary.
/// 管理端 API 边界接受的结构化 local vLLM 写入体。
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct AiBackendVllmLocalWriteBody {
    pub(crate) mode: Option<String>,
    pub(crate) managed_externally: Option<bool>,
}

/// Provider-specific remote write body accepted at the admin API boundary.
/// 管理端 API 边界接受的 provider-specific remote 写入体。
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "provider_family", content = "config", rename_all = "snake_case")]
pub(crate) enum AiBackendRemoteProviderWriteBody {
    /// Structured OpenAI-compatible remote input body / 结构化 OpenAI-compatible remote 输入体
    OpenAiCompatible(AiBackendOpenAiCompatibleRemoteWriteBody),
    /// Structured Ollama remote input body / 结构化 Ollama remote 输入体
    Ollama(AiBackendOllamaRemoteWriteBody),
    /// Internal remote provider body / 内部 remote provider 输入体
    Internal,
}

/// Structured OpenAI-compatible remote write body accepted at the admin API boundary.
/// 管理端 API 边界接受的结构化 OpenAI-compatible remote 写入体。
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct AiBackendOpenAiCompatibleRemoteWriteBody {
    pub(crate) base_url: Option<String>,
    pub(crate) credential_ref: Option<String>,
    pub(crate) operations: Option<Vec<String>>,
    pub(crate) features: Option<Vec<String>>,
    pub(crate) transports: Option<Vec<String>>,
}

/// Structured Ollama remote write body accepted at the admin API boundary.
/// 管理端 API 边界接受的结构化 Ollama remote 写入体。
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct AiBackendOllamaRemoteWriteBody {
    pub(crate) base_url: Option<String>,
    pub(crate) operations: Option<Vec<String>>,
    pub(crate) features: Option<Vec<String>>,
    pub(crate) transports: Option<Vec<String>>,
}

/// Typed llama.cpp metadata accepted at the admin write boundary.
/// 管理端写入边界接受的强类型 llama.cpp metadata。
#[derive(Debug, Clone, PartialEq, Eq)]
struct LlamaCppAdminMetadataInput {
    model_url: Option<String>,
    model_path: Option<String>,
    skip_download: Option<bool>,
    download_timeout_s: Option<u64>,
    server_mode: Option<String>,
    server_cmd: Option<String>,
    server_cmd_args: Option<String>,
    threads: Option<String>,
    ctx_size: Option<String>,
    ready_probe: Option<String>,
    start_timeout_s: Option<u64>,
}

/// Typed vLLM metadata accepted at the admin write boundary.
/// 管理端写入边界接受的强类型 vLLM metadata。
#[derive(Debug, Clone, PartialEq, Eq)]
struct VllmAdminMetadataInput {
    mode: Option<String>,
    managed_externally: Option<bool>,
}

/// Typed local/remote write view derived from the admin backend write body.
/// 从管理端 backend 写入体派生出的 local/remote 强类型写入视图。
#[derive(Debug, Clone)]
enum AiBackendWriteTypedView {
    Local(LocalAiBackendWriteView),
    Remote(RemoteAiBackendWriteView),
}

/// Typed local backend write view.
/// 强类型 local backend 写入视图。
#[derive(Debug, Clone)]
struct LocalAiBackendWriteView {
    body: AiBackendWriteBody,
    kind: CanonicalBackendKind,
    provider: CanonicalBackendProvider,
}

/// Typed remote backend write view.
/// 强类型 remote backend 写入视图。
#[derive(Debug, Clone)]
struct RemoteAiBackendWriteView {
    body: AiBackendWriteBody,
    kind: CanonicalBackendKind,
    provider: CanonicalBackendProvider,
}

/// Provider-specific local backend write view.
/// provider-specific 的 local backend 写入视图。
#[derive(Debug, Clone)]
enum LocalAiBackendWriteProviderView<'a> {
    Ollama(&'a LocalAiBackendWriteView),
    LlamaCpp(LlamaCppLocalAiBackendWriteView<'a>),
    Vllm(VllmLocalAiBackendWriteView<'a>),
    Internal(&'a LocalAiBackendWriteView),
}

/// Provider-specific remote backend write view.
/// provider-specific 的 remote backend 写入视图。
#[derive(Debug, Clone, Copy)]
enum RemoteAiBackendWriteProviderView<'a> {
    OpenAi(OpenAiCompatibleRemoteAiBackendWriteView<'a>),
    Ollama(OllamaRemoteAiBackendWriteView<'a>),
    Internal(&'a RemoteAiBackendWriteView),
}

/// Local llama.cpp write view with parsed provider-specific metadata.
/// 带有 provider-specific metadata 的 local llama.cpp 写入视图。
#[derive(Debug, Clone)]
struct LlamaCppLocalAiBackendWriteView<'a> {
    local: &'a LocalAiBackendWriteView,
    metadata: LlamaCppAdminMetadataInput,
}

/// Local vLLM write view with parsed provider-specific metadata.
/// 带有 provider-specific metadata 的 local vLLM 写入视图。
#[derive(Debug, Clone)]
struct VllmLocalAiBackendWriteView<'a> {
    local: &'a LocalAiBackendWriteView,
    metadata: VllmAdminMetadataInput,
}

/// Remote OpenAI-compatible write view with resolved provider-specific config.
/// 带有 provider-specific 配置的 remote OpenAI-compatible 写入视图。
#[derive(Debug, Clone, Copy)]
struct OpenAiCompatibleRemoteAiBackendWriteView<'a> {
    remote: &'a RemoteAiBackendWriteView,
    config: Option<&'a AiBackendOpenAiCompatibleRemoteWriteBody>,
}

/// Remote Ollama write view with resolved provider-specific config.
/// 带有 provider-specific 配置的 remote Ollama 写入视图。
#[derive(Debug, Clone, Copy)]
struct OllamaRemoteAiBackendWriteView<'a> {
    remote: &'a RemoteAiBackendWriteView,
    config: Option<&'a AiBackendOllamaRemoteWriteBody>,
}

/// Resolved write-time backend spec used by create/update/preflight.
/// create/update/preflight 共享的写入时 backend 规范视图。
#[derive(Debug, Clone, PartialEq, Eq)]
struct ResolvedAdminWriteSpec {
    base_url: Option<String>,
    credential_ref: Option<String>,
    operations: Vec<String>,
    features: Vec<String>,
    transports: Vec<String>,
}

#[derive(Deserialize)]
pub(crate) struct AiBackendPreflightBody {
    pub(crate) backend: AiBackendWriteBody,
    pub(crate) node_uuids: Vec<String>,
    pub(crate) verification_policy: Option<String>,
    pub(crate) requested_checks: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize)]
struct SpearletLocalModelPreflightRequest {
    provider: String,
    model: String,
    model_url: Option<String>,
    model_path: Option<String>,
    skip_download: Option<bool>,
    download_timeout_s: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "source_kind", rename_all = "snake_case")]
enum SpearletLocalModelPreflightDetailsResponse {
    Unavailable,
    ModelPath {
        effective_model_path: String,
    },
    ModelUrl {
        effective_model_path: String,
        final_url: String,
        http_status: u16,
        content_length: Option<u64>,
    },
}

#[derive(Debug, Deserialize)]
struct SpearletLocalModelPreflightResponse {
    success: bool,
    details: SpearletLocalModelPreflightDetailsResponse,
    message: String,
}

#[derive(Debug, Clone, Serialize)]
struct SpearletRemoteBackendPreflightRequest {
    backend: SpearletRemoteBackendDraft,
    requested_checks: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize)]
struct SpearletRemoteBackendDraft {
    provider: String,
    model: String,
    hosting: String,
    backend_kind: String,
    base_url: Option<String>,
    credential_ref: Option<String>,
    operations: Option<Vec<String>>,
    features: Option<Vec<String>>,
    transports: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "outcome_kind", rename_all = "snake_case")]
enum SpearletRemoteBackendPreflightGenericOutcomeResponse {
    Success {
        phase: String,
        resolved_endpoint: Option<String>,
        http_status: Option<u16>,
        checks: Vec<SpearletRemoteBackendPreflightCheck>,
        auth_valid: Option<bool>,
        model_accessible: Option<bool>,
    },
    Failure {
        phase: String,
        error_code: String,
        resolved_endpoint: Option<String>,
        http_status: Option<u16>,
        provider_code: Option<String>,
        checks: Vec<SpearletRemoteBackendPreflightCheck>,
        auth_valid: Option<bool>,
        model_accessible: Option<bool>,
    },
}

#[derive(Debug, Deserialize)]
#[serde(tag = "outcome_kind", rename_all = "snake_case")]
enum SpearletOpenAiCompatiblePreflightOutcomeResponse {
    ConnectivityVerified {
        resolved_endpoint: Option<String>,
        http_status: Option<u16>,
        checks: Vec<SpearletRemoteBackendPreflightCheck>,
    },
    AuthVerified {
        resolved_endpoint: Option<String>,
        http_status: Option<u16>,
        checks: Vec<SpearletRemoteBackendPreflightCheck>,
        auth_valid: bool,
    },
    ModelAccessVerified {
        resolved_endpoint: Option<String>,
        http_status: Option<u16>,
        checks: Vec<SpearletRemoteBackendPreflightCheck>,
        auth_valid: bool,
        model_accessible: bool,
    },
    ConnectivityFailed {
        error_code: String,
        resolved_endpoint: Option<String>,
        http_status: Option<u16>,
        provider_code: Option<String>,
        checks: Vec<SpearletRemoteBackendPreflightCheck>,
    },
    AuthFailed {
        error_code: String,
        resolved_endpoint: Option<String>,
        http_status: Option<u16>,
        provider_code: Option<String>,
        checks: Vec<SpearletRemoteBackendPreflightCheck>,
        auth_valid: bool,
    },
    ModelAccessFailed {
        error_code: String,
        resolved_endpoint: Option<String>,
        http_status: Option<u16>,
        provider_code: Option<String>,
        checks: Vec<SpearletRemoteBackendPreflightCheck>,
        auth_valid: bool,
        model_accessible: bool,
    },
}

#[derive(Debug, Deserialize)]
#[serde(tag = "outcome_kind", rename_all = "snake_case")]
enum SpearletOllamaPreflightOutcomeResponse {
    ConnectivityVerified {
        resolved_endpoint: Option<String>,
        http_status: Option<u16>,
        checks: Vec<SpearletRemoteBackendPreflightCheck>,
    },
    ModelAccessVerified {
        resolved_endpoint: Option<String>,
        http_status: Option<u16>,
        checks: Vec<SpearletRemoteBackendPreflightCheck>,
        model_accessible: bool,
    },
    ConnectivityFailed {
        error_code: String,
        resolved_endpoint: Option<String>,
        http_status: Option<u16>,
        provider_code: Option<String>,
        checks: Vec<SpearletRemoteBackendPreflightCheck>,
    },
    ModelAccessFailed {
        error_code: String,
        resolved_endpoint: Option<String>,
        http_status: Option<u16>,
        provider_code: Option<String>,
        checks: Vec<SpearletRemoteBackendPreflightCheck>,
        model_accessible: bool,
    },
}

#[derive(Debug, Deserialize)]
#[serde(tag = "provider_family", rename_all = "snake_case")]
enum SpearletRemoteBackendPreflightResultResponse {
    OpenAiCompatible {
        outcome: SpearletOpenAiCompatiblePreflightOutcomeResponse,
    },
    Ollama {
        outcome: SpearletOllamaPreflightOutcomeResponse,
    },
    Unknown {
        provider: String,
        outcome: SpearletRemoteBackendPreflightGenericOutcomeResponse,
    },
}

#[derive(Debug, Deserialize)]
struct SpearletRemoteBackendPreflightResponse {
    success: bool,
    result: SpearletRemoteBackendPreflightResultResponse,
    message: String,
    latency_ms: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct SpearletRemoteBackendPreflightCheck {
    name: String,
    ok: bool,
}

#[derive(Deserialize)]
pub(crate) struct SetAiBackendDesiredStateBody {
    pub(crate) desired_state: String,
}

#[derive(Deserialize)]
pub(crate) struct AiBackendPlacementWriteBody {
    pub(crate) placement_id: Option<String>,
    pub(crate) backend_id: String,
    pub(crate) node_uuid: String,
    pub(crate) desired_state: Option<String>,
    pub(crate) weight_override: Option<i32>,
    pub(crate) priority_override: Option<i32>,
}

#[derive(Deserialize)]
pub(crate) struct AiBackendPlacementsQuery {
    pub(crate) backend_id: Option<String>,
    pub(crate) node_uuid: Option<String>,
}

pub(crate) async fn list_ai_backends_admin(
    state: GatewayState,
    Query(q): Query<AiBackendsQuery>,
) -> Json<AdminAiBackendListResponse> {
    let mut client = state.ai_backend_control_plane_client.clone();
    match client
        .list_ai_backends(crate::proto::sms::ListAiBackendsRequest {
            limit: q.limit.unwrap_or(0),
            offset: q.offset.unwrap_or(0),
            q: q.q.unwrap_or_default(),
            hosting: q.hosting.unwrap_or_default(),
            desired_state: q.desired_state.unwrap_or_default(),
            provider: q.provider.unwrap_or_default(),
            model: q.model.unwrap_or_default(),
        })
        .await
    {
        Ok(resp) => {
            let inner = resp.into_inner();
            let backends = inner
                .backends
                .into_iter()
                .map(backend_record_to_response)
                .collect::<Vec<_>>();
            Json(AdminAiBackendListResponse {
                success: true,
                backends,
                total_count: inner.total_count,
                message: None,
            })
        }
        Err(e) => Json(AdminAiBackendListResponse {
            success: false,
            backends: Vec::new(),
            total_count: 0,
            message: Some(e.to_string()),
        }),
    }
}

pub(crate) async fn get_ai_backend_admin(
    state: GatewayState,
    p: Path<String>,
) -> Json<AdminAiBackendDetailResponse> {
    let backend_id = p.0;
    if backend_id.trim().is_empty() {
        return Json(AdminAiBackendDetailResponse {
            success: false,
            found: false,
            backend: None,
            message: Some("backend_id is required".to_string()),
        });
    }

    let mut client = state.ai_backend_control_plane_client.clone();
    match client
        .get_ai_backend(crate::proto::sms::GetAiBackendRequest { backend_id })
        .await
    {
        Ok(resp) => {
            let inner = resp.into_inner();
            Json(AdminAiBackendDetailResponse {
                success: true,
                found: inner.found,
                backend: inner.backend.map(backend_record_to_response),
                message: None,
            })
        }
        Err(e) => Json(AdminAiBackendDetailResponse {
            success: false,
            found: false,
            backend: None,
            message: Some(e.to_string()),
        }),
    }
}

pub(crate) async fn create_ai_backend_admin(
    state: GatewayState,
    body: Json<AiBackendWriteBody>,
) -> Json<AdminAiBackendMutationResponse> {
    let backend = match ai_backend_proto_from_body(body.0, String::new()) {
        Ok(value) => value,
        Err(message) => {
            return Json(AdminAiBackendMutationResponse {
                success: false,
                backend: None,
                deleted: None,
                message: Some(message),
            })
        }
    };

    let mut client = state.ai_backend_control_plane_client.clone();
    match client
        .create_ai_backend(crate::proto::sms::CreateAiBackendRequest {
            backend: Some(backend),
        })
        .await
    {
        Ok(resp) => {
            let inner = resp.into_inner();
            Json(AdminAiBackendMutationResponse {
                success: true,
                backend: inner.backend.map(backend_record_to_response),
                deleted: None,
                message: None,
            })
        }
        Err(e) => Json(AdminAiBackendMutationResponse {
            success: false,
            backend: None,
            deleted: None,
            message: Some(e.to_string()),
        }),
    }
}

pub(crate) async fn preflight_ai_backend_admin(
    state: GatewayState,
    body: Json<AiBackendPreflightBody>,
) -> Json<AdminAiBackendPreflightResponse> {
    if body.node_uuids.is_empty() {
        return Json(AdminAiBackendPreflightResponse {
            success: false,
            results: Vec::new(),
            message: Some("node_uuids is required".to_string()),
        });
    }

    if let Err(message) = ai_backend_proto_from_body(body.backend.clone(), String::new()) {
        return Json(AdminAiBackendPreflightResponse {
            success: false,
            results: Vec::new(),
            message: Some(message),
        });
    }

    let backend = &body.backend;
    let verification_policy =
        determine_preflight_policy(body.verification_policy.as_deref(), body.node_uuids.len());
    let target_node_uuids = select_preflight_nodes(&body.node_uuids, &verification_policy);
    if target_node_uuids.is_empty() {
        return Json(AdminAiBackendPreflightResponse {
            success: false,
            results: Vec::new(),
            message: Some("no target nodes remain after applying verification policy".to_string()),
        });
    }

    let http_client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
    {
        Ok(client) => client,
        Err(error) => {
            return Json(AdminAiBackendPreflightResponse {
                success: false,
                results: Vec::new(),
                message: Some(error.to_string()),
            })
        }
    };

    let mut results = Vec::new();
    let mut had_failures = false;
    for node_uuid in &target_node_uuids {
        let target = match super::node_rpc::resolve_node_http_target(&state, node_uuid).await {
            Ok(target) => target,
            Err(resp) => {
                let message = resp.0["message"]
                    .as_str()
                    .unwrap_or("failed to resolve node http target")
                    .to_string();
                return Json(AdminAiBackendPreflightResponse {
                    success: false,
                    results,
                    message: Some(format!("{}: {}", node_uuid, message)),
                });
            }
        };

        let node_result = match backend.hosting.trim().to_ascii_lowercase().as_str() {
            "local" => preflight_local_backend_node(&http_client, backend, &target.base_url).await,
            "remote" => {
                preflight_remote_backend_node(
                    &http_client,
                    backend,
                    &target.base_url,
                    body.requested_checks.as_ref(),
                )
                .await
            }
            _ => Err("preflight only supports local or remote backends".to_string()),
        };

        let result = match node_result {
            Ok(value) => value,
            Err(message) => {
                return Json(AdminAiBackendPreflightResponse {
                    success: false,
                    results,
                    message: Some(format!("{}: {}", target.node_uuid, message)),
                });
            }
        };
        let success = result.success;
        let message = result.message.clone();
        results.push(AdminAiBackendPreflightNodeResponse {
            node_uuid: target.node_uuid.clone(),
            ..result
        });
        if !success {
            had_failures = true;
            if verification_policy == "best_effort" {
                continue;
            }
            return Json(AdminAiBackendPreflightResponse {
                success: false,
                results,
                message: Some(format!("{}: {}", target.node_uuid, message)),
            });
        }
    }

    Json(AdminAiBackendPreflightResponse {
        success: !had_failures || verification_policy == "best_effort",
        results,
        message: if verification_policy == "best_effort" && had_failures {
            Some(
                "preflight completed with warnings because verification_policy=best_effort"
                    .to_string(),
            )
        } else if verification_policy == "sampled_strict"
            && body.node_uuids.len() > target_node_uuids.len()
        {
            Some(format!(
                "sampled {} of {} nodes before create; full verification continues after placement reconciliation",
                target_node_uuids.len(),
                body.node_uuids.len()
            ))
        } else {
            None
        },
    })
}

pub(crate) async fn update_ai_backend_admin(
    state: GatewayState,
    p: Path<String>,
    body: Json<AiBackendWriteBody>,
) -> Json<AdminAiBackendMutationResponse> {
    let backend_id = p.0;
    if backend_id.trim().is_empty() {
        return Json(AdminAiBackendMutationResponse {
            success: false,
            backend: None,
            deleted: None,
            message: Some("backend_id is required".to_string()),
        });
    }

    let backend = match ai_backend_proto_from_body(body.0, backend_id) {
        Ok(value) => value,
        Err(message) => {
            return Json(AdminAiBackendMutationResponse {
                success: false,
                backend: None,
                deleted: None,
                message: Some(message),
            })
        }
    };

    let mut client = state.ai_backend_control_plane_client.clone();
    match client
        .update_ai_backend(crate::proto::sms::UpdateAiBackendRequest {
            backend: Some(backend),
        })
        .await
    {
        Ok(resp) => {
            let inner = resp.into_inner();
            Json(AdminAiBackendMutationResponse {
                success: true,
                backend: inner.backend.map(backend_record_to_response),
                deleted: None,
                message: None,
            })
        }
        Err(e) => Json(AdminAiBackendMutationResponse {
            success: false,
            backend: None,
            deleted: None,
            message: Some(e.to_string()),
        }),
    }
}

pub(crate) async fn delete_ai_backend_admin(
    state: GatewayState,
    p: Path<String>,
) -> Json<AdminAiBackendMutationResponse> {
    let backend_id = p.0;
    if backend_id.trim().is_empty() {
        return Json(AdminAiBackendMutationResponse {
            success: false,
            backend: None,
            deleted: None,
            message: Some("backend_id is required".to_string()),
        });
    }

    let mut client = state.ai_backend_control_plane_client.clone();
    match client
        .delete_ai_backend(crate::proto::sms::DeleteAiBackendRequest { backend_id })
        .await
    {
        Ok(resp) => Json(AdminAiBackendMutationResponse {
            success: true,
            backend: None,
            deleted: Some(resp.into_inner().deleted),
            message: None,
        }),
        Err(e) => Json(AdminAiBackendMutationResponse {
            success: false,
            backend: None,
            deleted: None,
            message: Some(e.to_string()),
        }),
    }
}

pub(crate) async fn set_ai_backend_desired_state_admin(
    state: GatewayState,
    p: Path<String>,
    body: Json<SetAiBackendDesiredStateBody>,
) -> Json<AdminAiBackendMutationResponse> {
    let backend_id = p.0;
    if backend_id.trim().is_empty() {
        return Json(AdminAiBackendMutationResponse {
            success: false,
            backend: None,
            deleted: None,
            message: Some("backend_id is required".to_string()),
        });
    }

    let desired_state = match parse_ai_backend_desired_state(&body.0.desired_state) {
        Ok(value) => value,
        Err(message) => {
            return Json(AdminAiBackendMutationResponse {
                success: false,
                backend: None,
                deleted: None,
                message: Some(message),
            })
        }
    };

    let mut client = state.ai_backend_control_plane_client.clone();
    match client
        .set_ai_backend_desired_state(crate::proto::sms::SetAiBackendDesiredStateRequest {
            backend_id,
            desired_state: desired_state as i32,
        })
        .await
    {
        Ok(resp) => {
            let inner = resp.into_inner();
            Json(AdminAiBackendMutationResponse {
                success: true,
                backend: inner.backend.map(backend_record_to_response),
                deleted: None,
                message: None,
            })
        }
        Err(e) => Json(AdminAiBackendMutationResponse {
            success: false,
            backend: None,
            deleted: None,
            message: Some(e.to_string()),
        }),
    }
}

pub(crate) async fn list_ai_backend_placements_admin(
    state: GatewayState,
    Query(q): Query<AiBackendPlacementsQuery>,
) -> Json<AdminAiBackendPlacementListResponse> {
    let mut client = state.ai_backend_control_plane_client.clone();
    match client
        .list_ai_backend_placements(crate::proto::sms::ListAiBackendPlacementsRequest {
            backend_id: q.backend_id.unwrap_or_default(),
            node_uuid: q.node_uuid.unwrap_or_default(),
        })
        .await
    {
        Ok(resp) => {
            let inner = resp.into_inner();
            let placements = inner
                .placements
                .into_iter()
                .map(backend_placement_to_response)
                .collect::<Vec<_>>();
            Json(AdminAiBackendPlacementListResponse {
                success: true,
                placements,
                total_count: inner.total_count,
                message: None,
            })
        }
        Err(e) => Json(AdminAiBackendPlacementListResponse {
            success: false,
            placements: Vec::new(),
            total_count: 0,
            message: Some(e.to_string()),
        }),
    }
}

pub(crate) async fn upsert_ai_backend_placement_admin(
    state: GatewayState,
    body: Json<AiBackendPlacementWriteBody>,
) -> Json<AdminAiBackendPlacementMutationResponse> {
    let placement = match ai_backend_placement_proto_from_body(body.0) {
        Ok(value) => value,
        Err(message) => {
            return Json(AdminAiBackendPlacementMutationResponse {
                success: false,
                placement: None,
                deleted: None,
                message: Some(message),
            })
        }
    };

    let mut client = state.ai_backend_control_plane_client.clone();
    match client
        .upsert_ai_backend_placement(crate::proto::sms::UpsertAiBackendPlacementRequest {
            placement: Some(placement),
        })
        .await
    {
        Ok(resp) => {
            let inner = resp.into_inner();
            Json(AdminAiBackendPlacementMutationResponse {
                success: true,
                placement: inner.placement.map(backend_placement_to_response),
                deleted: None,
                message: None,
            })
        }
        Err(e) => Json(AdminAiBackendPlacementMutationResponse {
            success: false,
            placement: None,
            deleted: None,
            message: Some(e.to_string()),
        }),
    }
}

pub(crate) async fn delete_ai_backend_placement_admin(
    state: GatewayState,
    p: Path<String>,
) -> Json<AdminAiBackendPlacementMutationResponse> {
    let placement_id = p.0;
    if placement_id.trim().is_empty() {
        return Json(AdminAiBackendPlacementMutationResponse {
            success: false,
            placement: None,
            deleted: None,
            message: Some("placement_id is required".to_string()),
        });
    }

    let mut client = state.ai_backend_control_plane_client.clone();
    match client
        .delete_ai_backend_placement(crate::proto::sms::DeleteAiBackendPlacementRequest {
            placement_id,
        })
        .await
    {
        Ok(resp) => Json(AdminAiBackendPlacementMutationResponse {
            success: true,
            placement: None,
            deleted: Some(resp.into_inner().deleted),
            message: None,
        }),
        Err(e) => Json(AdminAiBackendPlacementMutationResponse {
            success: false,
            placement: None,
            deleted: None,
            message: Some(e.to_string()),
        }),
    }
}

pub(crate) async fn list_ai_backend_assignments_admin(
    state: GatewayState,
    p: Path<String>,
) -> Json<AdminAiBackendAssignmentListResponse> {
    let node_uuid = p.0;
    if node_uuid.trim().is_empty() {
        return Json(AdminAiBackendAssignmentListResponse {
            success: false,
            assignments: Vec::new(),
            total_count: 0,
            message: Some("node_uuid is required".to_string()),
        });
    }

    let mut client = state.ai_backend_control_plane_client.clone();
    match client
        .list_ai_backend_assignments(crate::proto::sms::ListAiBackendAssignmentsRequest {
            node_uuid,
        })
        .await
    {
        Ok(resp) => {
            let inner = resp.into_inner();
            let assignments = inner
                .assignments
                .into_iter()
                .map(backend_assignment_to_response)
                .collect::<Vec<_>>();
            Json(AdminAiBackendAssignmentListResponse {
                success: true,
                assignments,
                total_count: inner.total_count,
                message: None,
            })
        }
        Err(e) => Json(AdminAiBackendAssignmentListResponse {
            success: false,
            assignments: Vec::new(),
            total_count: 0,
            message: Some(e.to_string()),
        }),
    }
}

pub(crate) async fn list_ai_backend_node_statuses_admin(
    state: GatewayState,
    p: Path<String>,
) -> Json<AdminAiBackendNodeStatusListResponse> {
    let backend_id = p.0;
    if backend_id.trim().is_empty() {
        return Json(AdminAiBackendNodeStatusListResponse {
            success: false,
            statuses: Vec::new(),
            total_count: 0,
            message: Some("backend_id is required".to_string()),
        });
    }

    let mut client = state.ai_backend_control_plane_client.clone();
    match client
        .list_ai_backend_node_statuses(crate::proto::sms::ListAiBackendNodeStatusesRequest {
            backend_id,
        })
        .await
    {
        Ok(resp) => {
            let inner = resp.into_inner();
            let statuses = inner
                .statuses
                .into_iter()
                .map(backend_node_status_to_response)
                .collect::<Vec<_>>();
            Json(AdminAiBackendNodeStatusListResponse {
                success: true,
                statuses,
                total_count: inner.total_count,
                message: None,
            })
        }
        Err(e) => Json(AdminAiBackendNodeStatusListResponse {
            success: false,
            statuses: Vec::new(),
            total_count: 0,
            message: Some(e.to_string()),
        }),
    }
}

pub(crate) async fn list_ai_model_views_admin(
    state: GatewayState,
    Query(q): Query<AiModelViewsQuery>,
) -> Json<AdminAiModelViewListResponse> {
    let mut client = state.ai_backend_control_plane_client.clone();
    match client
        .list_ai_model_views(crate::proto::sms::ListAiModelViewsRequest {
            limit: q.limit.unwrap_or(0),
            offset: q.offset.unwrap_or(0),
            q: q.q.unwrap_or_default(),
            hosting: q.hosting.unwrap_or_default(),
            status: q.status.unwrap_or_default(),
            provider: q.provider.unwrap_or_default(),
            model: q.model.unwrap_or_default(),
        })
        .await
    {
        Ok(resp) => {
            let inner = resp.into_inner();
            let views = inner
                .views
                .into_iter()
                .map(model_view_to_response)
                .collect::<Vec<_>>();
            Json(AdminAiModelViewListResponse {
                success: true,
                views,
                total_count: inner.total_count,
                message: None,
            })
        }
        Err(e) => Json(AdminAiModelViewListResponse {
            success: false,
            views: Vec::new(),
            total_count: 0,
            message: Some(e.to_string()),
        }),
    }
}

fn ai_backend_proto_from_body(
    body: AiBackendWriteBody,
    backend_id: String,
) -> Result<crate::proto::sms::AiBackendRecord, String> {
    if body.display_name.trim().is_empty() {
        return Err("display_name is required".to_string());
    }
    if body.provider.trim().is_empty() {
        return Err("provider is required".to_string());
    }
    if body.model.trim().is_empty() {
        return Err("model is required".to_string());
    }
    if body.backend_kind.trim().is_empty() {
        return Err("backend_kind is required".to_string());
    }
    let canonical_kind = CanonicalBackendKind::parse(&body.backend_kind);
    let canonical_provider = parse_supported_ai_backend_provider(&body.provider)?;
    let hosting = parse_ai_backend_hosting(&body.hosting)?;
    let typed = AiBackendWriteTypedView::parse(body.clone())?;
    let resolved_spec = typed.resolved_spec()?;
    if resolved_spec.operations.is_empty() {
        return Err("spec.operations is required".to_string());
    }
    let management_mode = match body.management_mode.as_deref() {
        Some(value) => parse_ai_backend_management_mode(value)?,
        None => default_ai_backend_management_mode(hosting),
    };
    let desired_state = match body.desired_state.as_deref() {
        Some(value) => parse_ai_backend_desired_state(value)?,
        None => crate::proto::sms::AiBackendDesiredState::Enabled,
    };
    let credential_ref = resolved_spec.credential_ref.clone();
    let normalized_metadata = typed.normalized_metadata()?;

    let record = crate::sms::ai_backends::model::AiBackendRecordModel {
        backend_id,
        display_name: body.display_name.trim().to_string(),
        provider: canonical_provider.as_str().to_string(),
        model: body.model.trim().to_string(),
        hosting: match hosting {
            crate::proto::sms::AiBackendHosting::Remote => {
                crate::sms::ai_backends::model::AiBackendHostingModel::Remote
            }
            crate::proto::sms::AiBackendHosting::Local => {
                crate::sms::ai_backends::model::AiBackendHostingModel::Local
            }
            crate::proto::sms::AiBackendHosting::Unspecified => {
                return Err("hosting is required".to_string())
            }
        },
        backend_kind: canonical_kind.as_str().to_string(),
        desired_state: match desired_state {
            crate::proto::sms::AiBackendDesiredState::Enabled => {
                crate::sms::ai_backends::model::AiBackendDesiredStateModel::Enabled
            }
            crate::proto::sms::AiBackendDesiredState::Disabled => {
                crate::sms::ai_backends::model::AiBackendDesiredStateModel::Disabled
            }
            crate::proto::sms::AiBackendDesiredState::Unspecified => {
                return Err("desired_state is required".to_string())
            }
        },
        management_mode: match management_mode {
            crate::proto::sms::AiBackendManagementMode::SmsRemote => {
                crate::sms::ai_backends::model::AiBackendManagementModeModel::SmsRemote
            }
            crate::proto::sms::AiBackendManagementMode::SmsLocal => {
                crate::sms::ai_backends::model::AiBackendManagementModeModel::SmsLocal
            }
            crate::proto::sms::AiBackendManagementMode::Unspecified => {
                return Err("management_mode is required".to_string())
            }
        },
        credential_ref: credential_ref.clone(),
        spec: crate::sms::ai_backends::model::AiBackendSpecModel {
            name: String::new(),
            kind: canonical_kind.as_str().to_string(),
            operations: resolved_spec.operations,
            features: resolved_spec.features,
            transports: resolved_spec.transports,
            weight: body.spec.weight.unwrap_or(100),
            priority: body.spec.priority.unwrap_or(0),
            base_url: resolved_spec.base_url.unwrap_or_default(),
            provider: canonical_provider.as_str().to_string(),
            model: body.model.trim().to_string(),
            credential_ref: credential_ref.unwrap_or_default(),
            origin: 0,
            deployment_id: String::new(),
        },
        labels: body.labels.unwrap_or_default().into_iter().collect(),
        metadata: normalized_metadata,
        generation: 0,
        created_at_ms: 0,
        updated_at_ms: 0,
    };
    Ok(crate::sms::ai_backends::proto_conv::proto_backend_from_domain(&record))
}

fn determine_preflight_policy(policy: Option<&str>, node_count: usize) -> String {
    let normalized = policy
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase());
    match normalized.as_deref() {
        Some("single_node_strict") => "single_node_strict".to_string(),
        Some("sampled_strict") => "sampled_strict".to_string(),
        Some("strict_all_nodes") => "strict_all_nodes".to_string(),
        Some("best_effort") => "best_effort".to_string(),
        _ if node_count <= 1 => "single_node_strict".to_string(),
        _ => "sampled_strict".to_string(),
    }
}

impl AiBackendWriteTypedView {
    /// Parse one admin write body into a typed local/remote write view.
    /// 将管理端写入体解析成强类型 local/remote 写入视图。
    fn parse(mut body: AiBackendWriteBody) -> Result<Self, String> {
        let hosting = CanonicalBackendHosting::parse(&body.hosting);
        let kind = CanonicalBackendKind::parse(&body.backend_kind);
        let provider = parse_supported_ai_backend_provider(&body.provider)?;
        if body.metadata.is_none() {
            body.metadata = Some(json!({}));
        }
        match hosting {
            CanonicalBackendHosting::Local => Ok(Self::Local(LocalAiBackendWriteView {
                body,
                kind,
                provider,
            })),
            CanonicalBackendHosting::Remote => Ok(Self::Remote(RemoteAiBackendWriteView {
                body,
                kind,
                provider,
            })),
            CanonicalBackendHosting::Unknown(_) => {
                Err("hosting must be remote or local".to_string())
            }
        }
    }

    /// Render one stable normalized metadata JSON shape.
    /// 渲染稳定的规范化 metadata JSON 形态。
    fn normalized_metadata(&self) -> Result<Value, String> {
        match self {
            Self::Local(local) => local.normalized_metadata(),
            Self::Remote(remote) => Ok(remote.metadata_object().clone().into()),
        }
    }

    /// Resolve the effective write-time spec after applying provider-specific input views.
    /// 应用 provider-specific 输入视图后，解析有效的写入时 spec。
    fn resolved_spec(&self) -> Result<ResolvedAdminWriteSpec, String> {
        match self {
            Self::Local(local) => Ok(local.default_resolved_spec()),
            Self::Remote(remote) => remote.resolved_spec(),
        }
    }
}

impl LocalAiBackendWriteView {
    /// Resolve the default local write-time spec from the legacy top-level fields.
    /// 从旧的顶层字段解析默认 local 写入时 spec。
    fn default_resolved_spec(&self) -> ResolvedAdminWriteSpec {
        ResolvedAdminWriteSpec {
            base_url: trim_to_option(self.body.spec.base_url.clone()),
            credential_ref: trim_to_option(self.body.credential_ref.clone()),
            operations: self.body.spec.operations.clone(),
            features: self.body.spec.features.clone().unwrap_or_default(),
            transports: self
                .body
                .spec
                .transports
                .clone()
                .unwrap_or_else(|| vec!["http".to_string()]),
        }
    }

    /// Return the provider-specific local request body when present.
    /// 返回存在时的 provider-specific local 请求体。
    fn local_provider_body(&self) -> Option<&AiBackendLocalProviderWriteBody> {
        self.body.local.as_ref()
    }

    /// Reject duplicated legacy metadata keys once structured local input is present.
    /// 当结构化 local 输入存在时，拒绝重复出现的 legacy metadata 字段。
    fn ensure_no_legacy_metadata_overlap(&self, keys: &[&str], provider_label: &str) -> Result<(), String> {
        let duplicates = keys
            .iter()
            .copied()
            .filter(|key| self.metadata_object().contains_key(*key))
            .collect::<Vec<_>>();
        if duplicates.is_empty() {
            return Ok(());
        }
        Err(format!(
            "local {provider_label} structured input cannot be combined with metadata keys: {}",
            duplicates.join(", ")
        ))
    }

    /// Project the local write view into one provider-specific subview.
    /// 将 local 写入视图投影为 provider-specific 子视图。
    fn provider_view(&self) -> Result<LocalAiBackendWriteProviderView<'_>, String> {
        match self.provider {
            CanonicalBackendProvider::Ollama => {
                if matches!(
                    self.local_provider_body(),
                    Some(AiBackendLocalProviderWriteBody::LlamaCpp(_))
                        | Some(AiBackendLocalProviderWriteBody::Vllm(_))
                        | Some(AiBackendLocalProviderWriteBody::Internal)
                ) {
                    return Err(
                        "local.provider_family must match backend provider ollama".to_string(),
                    );
                }
                Ok(LocalAiBackendWriteProviderView::Ollama(self))
            }
            CanonicalBackendProvider::LlamaCpp => Ok(LocalAiBackendWriteProviderView::LlamaCpp(
                LlamaCppLocalAiBackendWriteView {
                    local: self,
                    metadata: match self.local_provider_body() {
                        Some(AiBackendLocalProviderWriteBody::LlamaCpp(body)) => {
                            self.ensure_no_legacy_metadata_overlap(
                                &[
                                    "model_url",
                                    "model_path",
                                    "skip_download",
                                    "download_timeout_s",
                                    "server_mode",
                                    "server_cmd",
                                    "server_cmd_args",
                                    "threads",
                                    "ctx_size",
                                    "ready_probe",
                                    "start_timeout_s",
                                ],
                                "llamacpp",
                            )?;
                            LlamaCppAdminMetadataInput::from_local_write_body(body)?
                        }
                        Some(_) => {
                            return Err(
                                "local.provider_family must match backend provider llamacpp"
                                    .to_string(),
                            )
                        }
                        None => {
                            return Err(
                                "local provider-specific input is required for backend provider llamacpp"
                                    .to_string(),
                            )
                        }
                    },
                },
            )),
            CanonicalBackendProvider::Vllm => Ok(LocalAiBackendWriteProviderView::Vllm(
                VllmLocalAiBackendWriteView {
                    local: self,
                    metadata: match self.local_provider_body() {
                        Some(AiBackendLocalProviderWriteBody::Vllm(body)) => {
                            self.ensure_no_legacy_metadata_overlap(
                                &["mode", "managed_externally"],
                                "vllm",
                            )?;
                            VllmAdminMetadataInput::from_local_write_body(body)?
                        }
                        Some(_) => {
                            return Err(
                                "local.provider_family must match backend provider vllm"
                                    .to_string(),
                            )
                        }
                        None => {
                            return Err(
                                "local provider-specific input is required for backend provider vllm"
                                    .to_string(),
                            )
                        }
                    },
                },
            )),
            CanonicalBackendProvider::Internal => {
                if matches!(
                    self.local_provider_body(),
                    Some(AiBackendLocalProviderWriteBody::LlamaCpp(_))
                        | Some(AiBackendLocalProviderWriteBody::Vllm(_))
                        | Some(AiBackendLocalProviderWriteBody::Ollama)
                ) {
                    return Err(
                        "local.provider_family must match backend provider internal".to_string(),
                    );
                }
                Ok(LocalAiBackendWriteProviderView::Internal(self))
            }
            CanonicalBackendProvider::OpenAi => {
                if self.local_provider_body().is_some() {
                    return Err(
                        "local provider-specific input is only supported for known local providers"
                            .to_string(),
                    );
                }
                return Err("openai does not support local hosting".to_string());
            }
            CanonicalBackendProvider::Unknown(_) => {
                Err("unsupported backend provider should have been rejected earlier".to_string())
            }
        }
    }

    /// Return the write-body metadata object for local providers.
    /// 返回 local provider 使用的写入体 metadata 对象。
    fn metadata_object(&self) -> &Map<String, Value> {
        self.body
            .metadata
            .as_ref()
            .and_then(|value| value.as_object())
            .expect("typed write view should always carry object metadata")
    }

    /// Render one normalized local metadata JSON shape.
    /// 渲染规范化后的 local metadata JSON 形态。
    fn normalized_metadata(&self) -> Result<Value, String> {
        match self.provider_view()? {
            LocalAiBackendWriteProviderView::LlamaCpp(llamacpp) => {
                Ok(llamacpp.normalized_metadata())
            }
            LocalAiBackendWriteProviderView::Vllm(vllm) => Ok(vllm.normalized_metadata()),
            LocalAiBackendWriteProviderView::Ollama(local)
            | LocalAiBackendWriteProviderView::Internal(local) => {
                Ok(local.metadata_object().clone().into())
            }
        }
    }
}

impl RemoteAiBackendWriteView {
    /// Return the provider-specific remote request body when present.
    /// 返回存在时的 provider-specific remote 请求体。
    fn remote_provider_body(&self) -> Option<&AiBackendRemoteProviderWriteBody> {
        self.body.remote.as_ref()
    }

    /// Reject duplicated legacy top-level fields once structured remote input is present.
    /// 当结构化 remote 输入存在时，拒绝重复出现的 legacy 顶层字段。
    fn ensure_no_legacy_remote_overlap(&self, provider_label: &str) -> Result<(), String> {
        let mut duplicates = Vec::new();
        if trim_to_option(self.body.spec.base_url.clone()).is_some() {
            duplicates.push("spec.base_url");
        }
        if trim_to_option(self.body.credential_ref.clone()).is_some() {
            duplicates.push("credential_ref");
        }
        if !self.body.spec.operations.is_empty() {
            duplicates.push("spec.operations");
        }
        if self
            .body
            .spec
            .features
            .as_ref()
            .is_some_and(|features| !features.is_empty())
        {
            duplicates.push("spec.features");
        }
        if self
            .body
            .spec
            .transports
            .as_ref()
            .is_some_and(|transports| !transports.is_empty())
        {
            duplicates.push("spec.transports");
        }
        if duplicates.is_empty() {
            return Ok(());
        }
        Err(format!(
            "remote {provider_label} structured input cannot be combined with legacy fields: {}",
            duplicates.join(", ")
        ))
    }

    /// Project the remote write view into one provider-specific subview.
    /// 将 remote 写入视图投影为 provider-specific 子视图。
    fn provider_view(&self) -> Result<RemoteAiBackendWriteProviderView<'_>, String> {
        match self.provider {
            CanonicalBackendProvider::OpenAi => match self.remote_provider_body() {
                Some(AiBackendRemoteProviderWriteBody::OpenAiCompatible(config)) => {
                    self.ensure_no_legacy_remote_overlap("openai")?;
                    Ok(RemoteAiBackendWriteProviderView::OpenAi(
                        OpenAiCompatibleRemoteAiBackendWriteView {
                        remote: self,
                        config: Some(config),
                        },
                    ))
                }
                Some(_) => {
                    return Err(
                        "remote.provider_family must match backend provider openai".to_string(),
                    )
                }
                None => {
                    return Err(
                        "remote provider-specific input is required for backend provider openai"
                            .to_string(),
                    )
                }
            },
            CanonicalBackendProvider::Ollama => match self.remote_provider_body() {
                Some(AiBackendRemoteProviderWriteBody::Ollama(config)) => {
                    self.ensure_no_legacy_remote_overlap("ollama")?;
                    Ok(RemoteAiBackendWriteProviderView::Ollama(
                        OllamaRemoteAiBackendWriteView {
                        remote: self,
                        config: Some(config),
                        },
                    ))
                }
                Some(_) => {
                    return Err(
                        "remote.provider_family must match backend provider ollama".to_string(),
                    )
                }
                None => {
                    return Err(
                        "remote provider-specific input is required for backend provider ollama"
                            .to_string(),
                    )
                }
            },
            CanonicalBackendProvider::Internal => {
                if matches!(
                    self.remote_provider_body(),
                    Some(AiBackendRemoteProviderWriteBody::OpenAiCompatible(_))
                        | Some(AiBackendRemoteProviderWriteBody::Ollama(_))
                ) {
                    return Err(
                        "remote.provider_family must match backend provider internal".to_string(),
                    );
                }
                Ok(RemoteAiBackendWriteProviderView::Internal(self))
            }
            CanonicalBackendProvider::LlamaCpp
            | CanonicalBackendProvider::Vllm => {
                if self.remote_provider_body().is_some() {
                    return Err(
                        "remote provider-specific input is only supported for known remote providers"
                            .to_string(),
                    );
                }
                return Err(format!(
                    "{} does not support remote hosting",
                    self.provider.as_str()
                ));
            }
            CanonicalBackendProvider::Unknown(_) => {
                Err("unsupported backend provider should have been rejected earlier".to_string())
            }
        }
    }

    /// Return the write-body metadata object for remote providers.
    /// 返回 remote provider 使用的写入体 metadata 对象。
    fn metadata_object(&self) -> &Map<String, Value> {
        self.body
            .metadata
            .as_ref()
            .and_then(|value| value.as_object())
            .expect("typed write view should always carry object metadata")
    }

    /// Resolve the default remote write-time spec from the legacy top-level fields.
    /// 从旧的顶层字段解析默认 remote 写入时 spec。
    fn default_resolved_spec(&self) -> ResolvedAdminWriteSpec {
        ResolvedAdminWriteSpec {
            base_url: trim_to_option(self.body.spec.base_url.clone()),
            credential_ref: trim_to_option(self.body.credential_ref.clone()),
            operations: self.body.spec.operations.clone(),
            features: self.body.spec.features.clone().unwrap_or_default(),
            transports: self
                .body
                .spec
                .transports
                .clone()
                .unwrap_or_else(|| vec!["http".to_string()]),
        }
    }

    /// Resolve the effective remote write-time spec after provider-specific overrides.
    /// 应用 provider-specific 覆盖后解析有效的 remote 写入时 spec。
    fn resolved_spec(&self) -> Result<ResolvedAdminWriteSpec, String> {
        Ok(match self.provider_view()? {
            RemoteAiBackendWriteProviderView::OpenAi(openai) => openai.resolved_spec(),
            RemoteAiBackendWriteProviderView::Ollama(ollama) => ollama.resolved_spec(),
            RemoteAiBackendWriteProviderView::Internal(remote) => remote.default_resolved_spec(),
        })
    }
}

impl<'a> LlamaCppLocalAiBackendWriteView<'a> {
    /// Render one normalized llama.cpp metadata JSON shape.
    /// 渲染规范化后的 llama.cpp metadata JSON 形态。
    fn normalized_metadata(&self) -> Value {
        let mut normalized = self.local.metadata_object().clone();
        self.metadata.apply_normalized(&mut normalized);
        Value::Object(normalized)
    }

    /// Build the node-side local-model preflight request.
    /// 构建节点侧 local-model preflight 请求。
    fn to_preflight_request(&self) -> SpearletLocalModelPreflightRequest {
        SpearletLocalModelPreflightRequest {
            provider: self.local.body.provider.trim().to_string(),
            model: self.local.body.model.trim().to_string(),
            model_url: self.metadata.model_url.clone(),
            model_path: self.metadata.model_path.clone(),
            skip_download: self.metadata.skip_download,
            download_timeout_s: self.metadata.download_timeout_s,
        }
    }
}

impl<'a> VllmLocalAiBackendWriteView<'a> {
    /// Render one normalized vLLM metadata JSON shape.
    /// 渲染规范化后的 vLLM metadata JSON 形态。
    fn normalized_metadata(&self) -> Value {
        let mut normalized = self.local.metadata_object().clone();
        self.metadata.apply_normalized(&mut normalized);
        Value::Object(normalized)
    }
}

impl<'a> OpenAiCompatibleRemoteAiBackendWriteView<'a> {
    /// Resolve the effective OpenAI-compatible remote spec.
    /// 解析有效的 OpenAI-compatible remote spec。
    fn resolved_spec(&self) -> ResolvedAdminWriteSpec {
        let fallback = self.remote.default_resolved_spec();
        ResolvedAdminWriteSpec {
            base_url: self
                .config
                .and_then(|config| trim_to_option(config.base_url.clone()))
                .or(fallback.base_url),
            credential_ref: self
                .config
                .and_then(|config| trim_to_option(config.credential_ref.clone()))
                .or(fallback.credential_ref),
            operations: self
                .config
                .and_then(|config| config.operations.clone())
                .unwrap_or(fallback.operations),
            features: self
                .config
                .and_then(|config| config.features.clone())
                .unwrap_or(fallback.features),
            transports: self
                .config
                .and_then(|config| config.transports.clone())
                .unwrap_or(fallback.transports),
        }
    }
}

impl<'a> OllamaRemoteAiBackendWriteView<'a> {
    /// Resolve the effective Ollama remote spec.
    /// 解析有效的 Ollama remote spec。
    fn resolved_spec(&self) -> ResolvedAdminWriteSpec {
        let fallback = self.remote.default_resolved_spec();
        ResolvedAdminWriteSpec {
            base_url: self
                .config
                .and_then(|config| trim_to_option(config.base_url.clone()))
                .or(fallback.base_url),
            credential_ref: fallback.credential_ref,
            operations: self
                .config
                .and_then(|config| config.operations.clone())
                .unwrap_or(fallback.operations),
            features: self
                .config
                .and_then(|config| config.features.clone())
                .unwrap_or(fallback.features),
            transports: self
                .config
                .and_then(|config| config.transports.clone())
                .unwrap_or(fallback.transports),
        }
    }
}

impl LlamaCppAdminMetadataInput {
    /// Parse typed llama.cpp metadata from one structured local write body.
    /// 从结构化 local 写入体解析强类型 llama.cpp metadata。
    fn from_local_write_body(body: &AiBackendLlamaCppLocalWriteBody) -> Result<Self, String> {
        let server_mode = trim_to_option(body.server_mode.clone()).map(|value| value.to_ascii_lowercase());
        let ready_probe = trim_to_option(body.ready_probe.clone()).map(|value| value.to_ascii_lowercase());
        let input = Self {
            model_url: trim_to_option(body.model_url.clone()),
            model_path: trim_to_option(body.model_path.clone()),
            skip_download: body.skip_download,
            download_timeout_s: body.download_timeout_s,
            server_mode: server_mode.clone(),
            server_cmd: trim_to_option(body.server_cmd.clone()),
            server_cmd_args: trim_to_option(body.server_cmd_args.clone()),
            threads: body.threads.map(|value| value.to_string()),
            ctx_size: body.ctx_size.map(|value| value.to_string()),
            ready_probe,
            start_timeout_s: body.start_timeout_s,
        };
        if matches!(server_mode.as_deref(), Some("raw")) && input.server_cmd_args.is_none() {
            return Err("llamacpp metadata server_mode=raw requires server_cmd_args".to_string());
        }
        Ok(input)
    }

    /// Apply the canonical llama.cpp metadata rendering to one JSON object.
    /// 将规范化后的 llama.cpp metadata 渲染回 JSON 对象。
    fn apply_normalized(&self, metadata: &mut Map<String, Value>) {
        set_optional_string(metadata, "model_url", self.model_url.clone());
        set_optional_string(metadata, "model_path", self.model_path.clone());
        set_optional_bool_string(metadata, "skip_download", self.skip_download);
        set_optional_u64_string(
            metadata,
            "download_timeout_s",
            self.download_timeout_s,
        );
        set_optional_string(metadata, "server_mode", self.server_mode.clone());
        set_optional_string(metadata, "server_cmd", self.server_cmd.clone());
        set_optional_string(metadata, "server_cmd_args", self.server_cmd_args.clone());
        set_optional_string(metadata, "threads", self.threads.clone());
        set_optional_string(metadata, "ctx_size", self.ctx_size.clone());
        set_optional_string(metadata, "ready_probe", self.ready_probe.clone());
        set_optional_u64_string(metadata, "start_timeout_s", self.start_timeout_s);
    }
}

impl VllmAdminMetadataInput {
    /// Parse typed vLLM metadata from one structured local write body.
    /// 从结构化 local 写入体解析强类型 vLLM metadata。
    fn from_local_write_body(body: &AiBackendVllmLocalWriteBody) -> Result<Self, String> {
        Ok(Self {
            mode: trim_to_option(body.mode.clone()).map(|value| value.to_ascii_lowercase()),
            managed_externally: body.managed_externally,
        })
    }

    /// Apply the canonical vLLM metadata rendering to one JSON object.
    /// 将规范化后的 vLLM metadata 渲染回 JSON 对象。
    fn apply_normalized(&self, metadata: &mut Map<String, Value>) {
        set_optional_string(metadata, "mode", self.mode.clone());
        set_optional_bool_string(
            metadata,
            "managed_externally",
            self.managed_externally,
        );
    }
}

fn set_optional_string(metadata: &mut Map<String, Value>, key: &str, value: Option<String>) {
    match value {
        Some(value) => {
            metadata.insert(key.to_string(), Value::String(value));
        }
        None => {
            metadata.remove(key);
        }
    }
}

fn set_optional_bool_string(metadata: &mut Map<String, Value>, key: &str, value: Option<bool>) {
    set_optional_string(metadata, key, value.map(|value| value.to_string()));
}

fn set_optional_u64_string(metadata: &mut Map<String, Value>, key: &str, value: Option<u64>) {
    set_optional_string(metadata, key, value.map(|value| value.to_string()));
}

fn llamacpp_preflight_request_from_backend(
    backend: &AiBackendWriteBody,
) -> Result<SpearletLocalModelPreflightRequest, String> {
    let typed = AiBackendWriteTypedView::parse(backend.clone())?;
    match typed {
        AiBackendWriteTypedView::Local(local) => match local.provider_view()? {
            LocalAiBackendWriteProviderView::LlamaCpp(llamacpp) => {
                Ok(llamacpp.to_preflight_request())
            }
            _ => Err("preflight only supports local llamacpp backends".to_string()),
        },
        AiBackendWriteTypedView::Remote(_) => {
            Err("preflight only supports local llamacpp backends".to_string())
        }
    }
}

fn select_preflight_nodes(node_uuids: &[String], verification_policy: &str) -> Vec<String> {
    let mut nodes = node_uuids
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    nodes.sort();
    nodes.dedup();
    match verification_policy {
        "sampled_strict" if nodes.len() > 1 => {
            let sample_size = nodes.len().min(3);
            nodes.into_iter().take(sample_size).collect()
        }
        _ => nodes,
    }
}

async fn preflight_local_backend_node(
    http_client: &reqwest::Client,
    backend: &AiBackendWriteBody,
    base_url: &str,
) -> Result<AdminAiBackendPreflightNodeResponse, String> {
    let typed = AiBackendWriteTypedView::parse(backend.clone())?;
    let local = match typed {
        AiBackendWriteTypedView::Local(local) => local,
        AiBackendWriteTypedView::Remote(_) => {
            return Err("preflight only supports local llamacpp backends".to_string())
        }
    };
    if !local.kind.supports_local_model_preflight() {
        return Err("preflight only supports local llamacpp backends".to_string());
    }
    let request = llamacpp_preflight_request_from_backend(&local.body)?;
    let url = format!("{}/internal/ai/local-models/preflight", base_url);
    let response = http_client
        .post(&url)
        .json(&request)
        .send()
        .await
        .map_err(|error| error.to_string())?;
    let status = response.status();
    let payload = response
        .json::<SpearletLocalModelPreflightResponse>()
        .await
        .map_err(|error| error.to_string())?;
    let details = match payload.details {
        SpearletLocalModelPreflightDetailsResponse::Unavailable => {
            AdminAiBackendLocalModelPreflightSourceResponse::Unavailable
        }
        SpearletLocalModelPreflightDetailsResponse::ModelPath {
            effective_model_path,
        } => AdminAiBackendLocalModelPreflightSourceResponse::ModelPath {
            effective_model_path,
        },
        SpearletLocalModelPreflightDetailsResponse::ModelUrl {
            effective_model_path,
            final_url,
            http_status,
            content_length,
        } => AdminAiBackendLocalModelPreflightSourceResponse::ModelUrl {
            effective_model_path,
            final_url,
            http_status,
            content_length,
        },
    };
    Ok(AdminAiBackendPreflightNodeResponse {
        node_uuid: String::new(),
        success: status.is_success() && payload.success,
        details: AdminAiBackendPreflightDetailsResponse::LocalModel {
            source: details,
        },
        latency_ms: None,
        message: payload.message,
    })
}

async fn preflight_remote_backend_node(
    http_client: &reqwest::Client,
    backend: &AiBackendWriteBody,
    base_url: &str,
    requested_checks: Option<&Vec<String>>,
) -> Result<AdminAiBackendPreflightNodeResponse, String> {
    let typed = AiBackendWriteTypedView::parse(backend.clone())?;
    let remote = match typed {
        AiBackendWriteTypedView::Remote(remote) => remote,
        AiBackendWriteTypedView::Local(_) => {
            return Err(
                "preflight only supports remote OpenAI-compatible or Ollama backends".to_string(),
            )
        }
    };
    if !remote.kind.supports_remote_preflight() {
        return Err(
            "preflight only supports remote OpenAI-compatible or Ollama backends".to_string(),
        );
    }
    match remote.provider_view()? {
        RemoteAiBackendWriteProviderView::OpenAi(_)
        | RemoteAiBackendWriteProviderView::Ollama(_) => {}
        RemoteAiBackendWriteProviderView::Internal(_) => {
            return Err(
                "preflight only supports remote OpenAI-compatible or Ollama backends".to_string(),
            )
        }
    }
    let resolved_spec = remote.resolved_spec()?;
    let request = SpearletRemoteBackendPreflightRequest {
        backend: SpearletRemoteBackendDraft {
            provider: remote.provider.as_str().to_string(),
            model: remote.body.model.trim().to_string(),
            hosting: CanonicalBackendHosting::Remote.as_str().to_string(),
            backend_kind: remote.kind.as_str().to_string(),
            base_url: resolved_spec.base_url,
            credential_ref: resolved_spec.credential_ref,
            operations: Some(resolved_spec.operations),
            features: Some(resolved_spec.features),
            transports: Some(resolved_spec.transports),
        },
        requested_checks: requested_checks.cloned(),
    };
    let url = format!("{}/internal/ai/backends/preflight", base_url);
    let response = http_client
        .post(&url)
        .json(&request)
        .send()
        .await
        .map_err(|error| error.to_string())?;
    let status = response.status();
    let payload = response
        .json::<SpearletRemoteBackendPreflightResponse>()
        .await
        .map_err(|error| error.to_string())?;
    let to_admin_checks = |checks: Vec<SpearletRemoteBackendPreflightCheck>| {
        checks
            .into_iter()
            .map(|check| AdminAiBackendPreflightCheckResponse {
                name: check.name,
                ok: check.ok,
            })
            .collect()
    };
    let to_unknown_outcome =
        |outcome: SpearletRemoteBackendPreflightGenericOutcomeResponse| match outcome {
            SpearletRemoteBackendPreflightGenericOutcomeResponse::Success {
                phase,
                resolved_endpoint,
                http_status,
                checks,
                auth_valid,
                model_accessible,
            } => AdminAiBackendRemoteProviderPreflightOutcomeResponse::Success {
                phase,
                resolved_endpoint,
                http_status,
                checks: to_admin_checks(checks),
                auth_valid,
                model_accessible,
            },
            SpearletRemoteBackendPreflightGenericOutcomeResponse::Failure {
                phase,
                error_code,
                resolved_endpoint,
                http_status,
                provider_code,
                checks,
                auth_valid,
                model_accessible,
            } => AdminAiBackendRemoteProviderPreflightOutcomeResponse::Failure {
                phase,
                error_code,
                resolved_endpoint,
                http_status,
                provider_code,
                checks: to_admin_checks(checks),
                auth_valid,
                model_accessible,
            },
        };
    let result = match payload.result {
        SpearletRemoteBackendPreflightResultResponse::OpenAiCompatible { outcome } => {
            AdminAiBackendRemoteProviderPreflightResultResponse::OpenAiCompatible {
                outcome: match outcome {
                    SpearletOpenAiCompatiblePreflightOutcomeResponse::ConnectivityVerified {
                        resolved_endpoint,
                        http_status,
                        checks,
                    } => AdminAiBackendOpenAiCompatiblePreflightOutcomeResponse::ConnectivityVerified {
                        resolved_endpoint,
                        http_status,
                        checks: to_admin_checks(checks),
                    },
                    SpearletOpenAiCompatiblePreflightOutcomeResponse::AuthVerified {
                        resolved_endpoint,
                        http_status,
                        checks,
                        auth_valid,
                    } => AdminAiBackendOpenAiCompatiblePreflightOutcomeResponse::AuthVerified {
                        resolved_endpoint,
                        http_status,
                        checks: to_admin_checks(checks),
                        auth_valid,
                    },
                    SpearletOpenAiCompatiblePreflightOutcomeResponse::ModelAccessVerified {
                        resolved_endpoint,
                        http_status,
                        checks,
                        auth_valid,
                        model_accessible,
                    } => {
                        AdminAiBackendOpenAiCompatiblePreflightOutcomeResponse::ModelAccessVerified {
                            resolved_endpoint,
                            http_status,
                            checks: to_admin_checks(checks),
                            auth_valid,
                            model_accessible,
                        }
                    }
                    SpearletOpenAiCompatiblePreflightOutcomeResponse::ConnectivityFailed {
                        error_code,
                        resolved_endpoint,
                        http_status,
                        provider_code,
                        checks,
                    } => AdminAiBackendOpenAiCompatiblePreflightOutcomeResponse::ConnectivityFailed {
                        error_code,
                        resolved_endpoint,
                        http_status,
                        provider_code,
                        checks: to_admin_checks(checks),
                    },
                    SpearletOpenAiCompatiblePreflightOutcomeResponse::AuthFailed {
                        error_code,
                        resolved_endpoint,
                        http_status,
                        provider_code,
                        checks,
                        auth_valid,
                    } => AdminAiBackendOpenAiCompatiblePreflightOutcomeResponse::AuthFailed {
                        error_code,
                        resolved_endpoint,
                        http_status,
                        provider_code,
                        checks: to_admin_checks(checks),
                        auth_valid,
                    },
                    SpearletOpenAiCompatiblePreflightOutcomeResponse::ModelAccessFailed {
                        error_code,
                        resolved_endpoint,
                        http_status,
                        provider_code,
                        checks,
                        auth_valid,
                        model_accessible,
                    } => AdminAiBackendOpenAiCompatiblePreflightOutcomeResponse::ModelAccessFailed {
                        error_code,
                        resolved_endpoint,
                        http_status,
                        provider_code,
                        checks: to_admin_checks(checks),
                        auth_valid,
                        model_accessible,
                    },
                },
            }
        }
        SpearletRemoteBackendPreflightResultResponse::Ollama { outcome } => {
            AdminAiBackendRemoteProviderPreflightResultResponse::Ollama {
                outcome: match outcome {
                    SpearletOllamaPreflightOutcomeResponse::ConnectivityVerified {
                        resolved_endpoint,
                        http_status,
                        checks,
                    } => AdminAiBackendOllamaPreflightOutcomeResponse::ConnectivityVerified {
                        resolved_endpoint,
                        http_status,
                        checks: to_admin_checks(checks),
                    },
                    SpearletOllamaPreflightOutcomeResponse::ModelAccessVerified {
                        resolved_endpoint,
                        http_status,
                        checks,
                        model_accessible,
                    } => AdminAiBackendOllamaPreflightOutcomeResponse::ModelAccessVerified {
                        resolved_endpoint,
                        http_status,
                        checks: to_admin_checks(checks),
                        model_accessible,
                    },
                    SpearletOllamaPreflightOutcomeResponse::ConnectivityFailed {
                        error_code,
                        resolved_endpoint,
                        http_status,
                        provider_code,
                        checks,
                    } => AdminAiBackendOllamaPreflightOutcomeResponse::ConnectivityFailed {
                        error_code,
                        resolved_endpoint,
                        http_status,
                        provider_code,
                        checks: to_admin_checks(checks),
                    },
                    SpearletOllamaPreflightOutcomeResponse::ModelAccessFailed {
                        error_code,
                        resolved_endpoint,
                        http_status,
                        provider_code,
                        checks,
                        model_accessible,
                    } => AdminAiBackendOllamaPreflightOutcomeResponse::ModelAccessFailed {
                        error_code,
                        resolved_endpoint,
                        http_status,
                        provider_code,
                        checks: to_admin_checks(checks),
                        model_accessible,
                    },
                },
            }
        }
        SpearletRemoteBackendPreflightResultResponse::Unknown { provider, outcome } => {
            AdminAiBackendRemoteProviderPreflightResultResponse::Unknown {
                provider,
                outcome: to_unknown_outcome(outcome),
            }
        }
    };
    Ok(AdminAiBackendPreflightNodeResponse {
        node_uuid: String::new(),
        success: status.is_success() && payload.success,
        details: AdminAiBackendPreflightDetailsResponse::RemoteProvider { result },
        latency_ms: payload.latency_ms,
        message: payload.message,
    })
}

fn ai_backend_placement_proto_from_body(
    body: AiBackendPlacementWriteBody,
) -> Result<crate::proto::sms::AiBackendPlacementRecord, String> {
    if body.backend_id.trim().is_empty() {
        return Err("backend_id is required".to_string());
    }
    if body.node_uuid.trim().is_empty() {
        return Err("node_uuid is required".to_string());
    }

    Ok(crate::proto::sms::AiBackendPlacementRecord {
        placement_id: body.placement_id.unwrap_or_default(),
        backend_id: body.backend_id,
        node_uuid: body.node_uuid,
        desired_state: match body.desired_state.as_deref() {
            Some(value) => parse_ai_backend_desired_state(value)? as i32,
            None => crate::proto::sms::AiBackendDesiredState::Enabled as i32,
        },
        weight_override: body.weight_override,
        priority_override: body.priority_override,
        generation: 0,
        created_at_ms: 0,
        updated_at_ms: 0,
    })
}

fn parse_ai_backend_hosting(value: &str) -> Result<crate::proto::sms::AiBackendHosting, String> {
    match CanonicalBackendHosting::parse(value) {
        CanonicalBackendHosting::Remote => Ok(CanonicalBackendHosting::Remote.to_proto_enum()),
        CanonicalBackendHosting::Local => Ok(CanonicalBackendHosting::Local.to_proto_enum()),
        CanonicalBackendHosting::Unknown(_) => Err("hosting must be remote or local".to_string()),
    }
}

/// Parse one admin backend provider and reject unsupported compatibility aliases.
/// 解析管理端 backend provider，并拒绝仍停留在兼容层的未建模 provider。
fn parse_supported_ai_backend_provider(value: &str) -> Result<CanonicalBackendProvider, String> {
    match CanonicalBackendProvider::parse(value) {
        CanonicalBackendProvider::Unknown(_) => {
            Err(format!("unsupported backend provider: {}", value.trim()))
        }
        provider => Ok(provider),
    }
}

fn parse_ai_backend_desired_state(
    value: &str,
) -> Result<crate::proto::sms::AiBackendDesiredState, String> {
    match CanonicalBackendDesiredState::parse(value) {
        CanonicalBackendDesiredState::Enabled => {
            Ok(CanonicalBackendDesiredState::Enabled.to_proto_enum())
        }
        CanonicalBackendDesiredState::Disabled => {
            Ok(CanonicalBackendDesiredState::Disabled.to_proto_enum())
        }
        CanonicalBackendDesiredState::Unknown(_) => {
            Err("desired_state must be enabled or disabled".to_string())
        }
    }
}

fn parse_ai_backend_management_mode(
    value: &str,
) -> Result<crate::proto::sms::AiBackendManagementMode, String> {
    match CanonicalBackendManagementMode::parse(value) {
        CanonicalBackendManagementMode::SmsRemote => {
            Ok(CanonicalBackendManagementMode::SmsRemote.to_proto_enum())
        }
        CanonicalBackendManagementMode::SmsLocal => {
            Ok(CanonicalBackendManagementMode::SmsLocal.to_proto_enum())
        }
        CanonicalBackendManagementMode::Unknown(_) => {
            Err("management_mode must be sms_remote or sms_local".to_string())
        }
    }
}

fn default_ai_backend_management_mode(
    hosting: crate::proto::sms::AiBackendHosting,
) -> crate::proto::sms::AiBackendManagementMode {
    match CanonicalBackendManagementMode::default_for_hosting(
        &CanonicalBackendHosting::from_proto_i32(hosting as i32),
    ) {
        CanonicalBackendManagementMode::SmsRemote => {
            CanonicalBackendManagementMode::SmsRemote.to_proto_enum()
        }
        CanonicalBackendManagementMode::SmsLocal => {
            CanonicalBackendManagementMode::SmsLocal.to_proto_enum()
        }
        CanonicalBackendManagementMode::Unknown(_) => {
            crate::proto::sms::AiBackendManagementMode::Unspecified
        }
    }
}

fn trim_to_option(value: Option<String>) -> Option<String> {
    value.and_then(|inner| {
        let trimmed = inner.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

#[cfg(test)]
mod tests {
    use super::{
        ai_backend_proto_from_body, default_ai_backend_management_mode,
        llamacpp_preflight_request_from_backend, parse_ai_backend_desired_state,
        parse_ai_backend_management_mode, AiBackendLlamaCppLocalWriteBody,
        AiBackendLocalProviderWriteBody, AiBackendOllamaRemoteWriteBody,
        AiBackendOpenAiCompatibleRemoteWriteBody, AiBackendRemoteProviderWriteBody,
        AiBackendSpecBody, AiBackendVllmLocalWriteBody, AiBackendWriteBody,
    };

    fn proto_struct_string_field<'a>(
        value: Option<&'a prost_types::Struct>,
        field_name: &str,
    ) -> Option<&'a str> {
        value
            .and_then(|inner| inner.fields.get(field_name))
            .and_then(|field| field.kind.as_ref())
            .and_then(|kind| match kind {
                prost_types::value::Kind::StringValue(value) => Some(value.as_str()),
                _ => None,
            })
    }

    #[test]
    fn ai_backend_proto_from_body_normalizes_provider_and_kind_aliases() {
        let record = ai_backend_proto_from_body(
            AiBackendWriteBody {
                display_name: "Local llama.cpp".to_string(),
                provider: "llama_cpp".to_string(),
                model: "qwen".to_string(),
                hosting: "local".to_string(),
                backend_kind: "llama.cpp".to_string(),
                management_mode: None,
                credential_ref: None,
                desired_state: None,
                spec: AiBackendSpecBody {
                    base_url: Some(String::new()),
                    operations: vec!["chat_completions".to_string()],
                    features: None,
                    transports: None,
                    weight: None,
                    priority: None,
                },
                labels: None,
                local: Some(AiBackendLocalProviderWriteBody::LlamaCpp(
                    AiBackendLlamaCppLocalWriteBody {
                        model_url: None,
                        model_path: Some("/models/qwen.gguf".to_string()),
                        skip_download: None,
                        download_timeout_s: None,
                        server_mode: None,
                        server_cmd: None,
                        server_cmd_args: None,
                        threads: None,
                        ctx_size: None,
                        ready_probe: None,
                        start_timeout_s: None,
                    },
                )),
                remote: None,
                metadata: Some(serde_json::json!({})),
            },
            "backend-1".to_string(),
        )
        .expect("body should normalize aliases");

        assert_eq!(record.provider, "llamacpp");
        assert_eq!(record.backend_kind, "llamacpp");
        assert_eq!(
            record.spec.as_ref().expect("spec should exist").provider,
            "llamacpp"
        );
        assert_eq!(
            record.spec.as_ref().expect("spec should exist").kind,
            "llamacpp"
        );
    }

    #[test]
    fn desired_state_and_management_mode_use_canonical_parsers() {
        assert_eq!(
            parse_ai_backend_desired_state("enabled").expect("enabled should parse") as i32,
            crate::proto::sms::AiBackendDesiredState::Enabled as i32
        );
        assert_eq!(
            parse_ai_backend_management_mode("remote").expect("remote should parse") as i32,
            crate::proto::sms::AiBackendManagementMode::SmsRemote as i32
        );
        assert_eq!(
            default_ai_backend_management_mode(crate::proto::sms::AiBackendHosting::Local) as i32,
            crate::proto::sms::AiBackendManagementMode::SmsLocal as i32
        );
    }

    #[test]
    fn ai_backend_proto_from_body_normalizes_llamacpp_metadata_scalars() {
        let record = ai_backend_proto_from_body(
            AiBackendWriteBody {
                display_name: "Local llama.cpp".to_string(),
                provider: "llamacpp".to_string(),
                model: "qwen".to_string(),
                hosting: "local".to_string(),
                backend_kind: "llamacpp".to_string(),
                management_mode: None,
                credential_ref: None,
                desired_state: None,
                spec: AiBackendSpecBody {
                    base_url: Some(String::new()),
                    operations: vec!["chat_completions".to_string()],
                    features: None,
                    transports: None,
                    weight: None,
                    priority: None,
                },
                labels: None,
                local: Some(AiBackendLocalProviderWriteBody::LlamaCpp(
                    AiBackendLlamaCppLocalWriteBody {
                        model_url: Some(" https://models.example.com/qwen.gguf ".to_string()),
                        model_path: None,
                        skip_download: Some(true),
                        download_timeout_s: Some(45),
                        server_mode: Some(" RAW ".to_string()),
                        server_cmd: None,
                        server_cmd_args: Some("--port 8080".to_string()),
                        threads: Some(8),
                        ctx_size: Some(4096),
                        ready_probe: Some(" HTTP ".to_string()),
                        start_timeout_s: None,
                    },
                )),
                remote: None,
                metadata: Some(serde_json::json!({
                    "extra_key": "preserve-me"
                })),
            },
            "backend-1".to_string(),
        )
        .expect("body should normalize metadata");

        assert_eq!(
            proto_struct_string_field(record.metadata.as_ref(), "model_url"),
            Some("https://models.example.com/qwen.gguf")
        );
        assert_eq!(
            proto_struct_string_field(record.metadata.as_ref(), "skip_download"),
            Some("true")
        );
        assert_eq!(
            proto_struct_string_field(record.metadata.as_ref(), "download_timeout_s"),
            Some("45")
        );
        assert_eq!(
            proto_struct_string_field(record.metadata.as_ref(), "server_mode"),
            Some("raw")
        );
        assert_eq!(
            proto_struct_string_field(record.metadata.as_ref(), "threads"),
            Some("8")
        );
        assert_eq!(
            proto_struct_string_field(record.metadata.as_ref(), "ctx_size"),
            Some("4096")
        );
        assert_eq!(
            proto_struct_string_field(record.metadata.as_ref(), "ready_probe"),
            Some("http")
        );
        assert_eq!(
            proto_struct_string_field(record.metadata.as_ref(), "extra_key"),
            Some("preserve-me")
        );
    }

    #[test]
    fn ai_backend_proto_from_body_rejects_raw_llamacpp_mode_without_args() {
        let err = ai_backend_proto_from_body(
            AiBackendWriteBody {
                display_name: "Local llama.cpp".to_string(),
                provider: "llamacpp".to_string(),
                model: "qwen".to_string(),
                hosting: "local".to_string(),
                backend_kind: "llamacpp".to_string(),
                management_mode: None,
                credential_ref: None,
                desired_state: None,
                spec: AiBackendSpecBody {
                    base_url: Some(String::new()),
                    operations: vec!["chat_completions".to_string()],
                    features: None,
                    transports: None,
                    weight: None,
                    priority: None,
                },
                labels: None,
                local: Some(AiBackendLocalProviderWriteBody::LlamaCpp(
                    AiBackendLlamaCppLocalWriteBody {
                        model_url: None,
                        model_path: None,
                        skip_download: None,
                        download_timeout_s: None,
                        server_mode: Some("raw".to_string()),
                        server_cmd: None,
                        server_cmd_args: None,
                        threads: None,
                        ctx_size: None,
                        ready_probe: None,
                        start_timeout_s: None,
                    },
                )),
                remote: None,
                metadata: Some(serde_json::json!({})),
            },
            "backend-1".to_string(),
        )
        .expect_err("raw mode without args should fail");

        assert_eq!(
            err,
            "llamacpp metadata server_mode=raw requires server_cmd_args"
        );
    }

    #[test]
    fn llamacpp_preflight_request_accepts_typed_metadata_scalars() {
        let request = llamacpp_preflight_request_from_backend(&AiBackendWriteBody {
            display_name: "Local llama.cpp".to_string(),
            provider: "llamacpp".to_string(),
            model: "qwen".to_string(),
            hosting: "local".to_string(),
            backend_kind: "llamacpp".to_string(),
            management_mode: None,
            credential_ref: None,
            desired_state: None,
            spec: AiBackendSpecBody {
                base_url: Some(String::new()),
                operations: vec!["chat_completions".to_string()],
                features: None,
                transports: None,
                weight: None,
                priority: None,
            },
            labels: None,
            local: Some(AiBackendLocalProviderWriteBody::LlamaCpp(
                AiBackendLlamaCppLocalWriteBody {
                    model_url: Some("https://models.example.com/qwen.gguf".to_string()),
                    model_path: None,
                    skip_download: Some(false),
                    download_timeout_s: Some(30),
                    server_mode: None,
                    server_cmd: None,
                    server_cmd_args: None,
                    threads: None,
                    ctx_size: None,
                    ready_probe: None,
                    start_timeout_s: None,
                },
            )),
            remote: None,
            metadata: Some(serde_json::json!({})),
        })
        .expect("preflight request should parse typed metadata");

        assert_eq!(request.provider, "llamacpp");
        assert_eq!(request.model, "qwen");
        assert_eq!(
            request.model_url.as_deref(),
            Some("https://models.example.com/qwen.gguf")
        );
        assert_eq!(request.skip_download, Some(false));
        assert_eq!(request.download_timeout_s, Some(30));
    }

    #[test]
    fn ai_backend_proto_from_body_normalizes_vllm_local_body() {
        let record = ai_backend_proto_from_body(
            AiBackendWriteBody {
                display_name: "Local vLLM".to_string(),
                provider: "vllm".to_string(),
                model: "qwen2.5".to_string(),
                hosting: "local".to_string(),
                backend_kind: "vllm".to_string(),
                management_mode: None,
                credential_ref: None,
                desired_state: None,
                spec: AiBackendSpecBody {
                    base_url: Some("http://127.0.0.1:8000".to_string()),
                    operations: vec!["chat_completions".to_string()],
                    features: None,
                    transports: None,
                    weight: None,
                    priority: None,
                },
                labels: None,
                local: Some(AiBackendLocalProviderWriteBody::Vllm(
                    AiBackendVllmLocalWriteBody {
                        mode: Some(" OpenAI ".to_string()),
                        managed_externally: Some(false),
                    },
                )),
                remote: None,
                metadata: Some(serde_json::json!({
                    "extra_key": "preserve-me"
                })),
            },
            "backend-2".to_string(),
        )
        .expect("body should normalize vllm local input");

        assert_eq!(
            proto_struct_string_field(record.metadata.as_ref(), "mode"),
            Some("openai")
        );
        assert_eq!(
            proto_struct_string_field(record.metadata.as_ref(), "managed_externally"),
            Some("false")
        );
        assert_eq!(
            proto_struct_string_field(record.metadata.as_ref(), "extra_key"),
            Some("preserve-me")
        );
    }

    #[test]
    fn ai_backend_proto_from_body_rejects_mismatched_local_provider_body() {
        let err = ai_backend_proto_from_body(
            AiBackendWriteBody {
                display_name: "Local llama.cpp".to_string(),
                provider: "llamacpp".to_string(),
                model: "qwen".to_string(),
                hosting: "local".to_string(),
                backend_kind: "llamacpp".to_string(),
                management_mode: None,
                credential_ref: None,
                desired_state: None,
                spec: AiBackendSpecBody {
                    base_url: Some(String::new()),
                    operations: vec!["chat_completions".to_string()],
                    features: None,
                    transports: None,
                    weight: None,
                    priority: None,
                },
                labels: None,
                local: Some(AiBackendLocalProviderWriteBody::Vllm(
                    AiBackendVllmLocalWriteBody {
                        mode: Some("openai".to_string()),
                        managed_externally: Some(false),
                    },
                )),
                remote: None,
                metadata: Some(serde_json::json!({})),
            },
            "backend-3".to_string(),
        )
        .expect_err("mismatched local provider body should fail");

        assert_eq!(
            err,
            "local.provider_family must match backend provider llamacpp"
        );
    }

    #[test]
    fn ai_backend_proto_from_body_rejects_duplicate_local_llamacpp_sources() {
        let err = ai_backend_proto_from_body(
            AiBackendWriteBody {
                display_name: "Local llama.cpp".to_string(),
                provider: "llamacpp".to_string(),
                model: "qwen".to_string(),
                hosting: "local".to_string(),
                backend_kind: "llamacpp".to_string(),
                management_mode: None,
                credential_ref: None,
                desired_state: None,
                spec: AiBackendSpecBody {
                    base_url: Some(String::new()),
                    operations: vec!["chat_completions".to_string()],
                    features: None,
                    transports: None,
                    weight: None,
                    priority: None,
                },
                labels: None,
                local: Some(AiBackendLocalProviderWriteBody::LlamaCpp(
                    AiBackendLlamaCppLocalWriteBody {
                        model_url: Some("https://models.example.com/qwen.gguf".to_string()),
                        model_path: None,
                        skip_download: Some(false),
                        download_timeout_s: None,
                        server_mode: None,
                        server_cmd: None,
                        server_cmd_args: None,
                        threads: None,
                        ctx_size: None,
                        ready_probe: None,
                        start_timeout_s: None,
                    },
                )),
                remote: None,
                metadata: Some(serde_json::json!({
                    "model_url": "https://duplicate.example.com/qwen.gguf"
                })),
            },
            "backend-local-dup".to_string(),
        )
        .expect_err("duplicate local structured and metadata fields should fail");

        assert_eq!(
            err,
            "local llamacpp structured input cannot be combined with metadata keys: model_url"
        );
    }

    #[test]
    fn ai_backend_proto_from_body_requires_local_structured_input_for_llamacpp() {
        let err = ai_backend_proto_from_body(
            AiBackendWriteBody {
                display_name: "Local llama.cpp".to_string(),
                provider: "llamacpp".to_string(),
                model: "qwen".to_string(),
                hosting: "local".to_string(),
                backend_kind: "llamacpp".to_string(),
                management_mode: None,
                credential_ref: None,
                desired_state: None,
                spec: AiBackendSpecBody {
                    base_url: Some("http://127.0.0.1:8080/v1".to_string()),
                    operations: vec!["chat_completions".to_string()],
                    features: None,
                    transports: None,
                    weight: None,
                    priority: None,
                },
                labels: None,
                local: None,
                remote: None,
                metadata: Some(serde_json::json!({
                    "model_path": "/models/qwen.gguf"
                })),
            },
            "backend-local-required".to_string(),
        )
        .expect_err("llamacpp should require structured local input");

        assert_eq!(
            err,
            "local provider-specific input is required for backend provider llamacpp"
        );
    }

    #[test]
    fn ai_backend_proto_from_body_resolves_remote_openai_body() {
        let record = ai_backend_proto_from_body(
            AiBackendWriteBody {
                display_name: "Remote OpenAI".to_string(),
                provider: "openai".to_string(),
                model: "gpt-4o-mini".to_string(),
                hosting: "remote".to_string(),
                backend_kind: "openai_chat_completion".to_string(),
                management_mode: None,
                credential_ref: None,
                desired_state: None,
                spec: AiBackendSpecBody {
                    base_url: Some(String::new()),
                    operations: vec![],
                    features: Some(vec![]),
                    transports: Some(vec![]),
                    weight: None,
                    priority: None,
                },
                labels: None,
                local: None,
                remote: Some(AiBackendRemoteProviderWriteBody::OpenAiCompatible(
                    AiBackendOpenAiCompatibleRemoteWriteBody {
                        base_url: Some(" https://api.openai.com/v1 ".to_string()),
                        credential_ref: Some("openai-prod".to_string()),
                        operations: Some(vec!["chat_completions".to_string()]),
                        features: Some(vec!["streaming".to_string()]),
                        transports: Some(vec!["http".to_string()]),
                    },
                )),
                metadata: Some(serde_json::json!({})),
            },
            "backend-4".to_string(),
        )
        .expect("body should resolve remote openai input");

        let spec = record.spec.as_ref().expect("spec should exist");
        assert_eq!(spec.base_url, "https://api.openai.com/v1");
        assert_eq!(spec.operations, vec!["chat_completions"]);
        assert_eq!(spec.features, vec!["streaming"]);
        assert_eq!(spec.transports, vec!["http"]);
        assert_eq!(record.credential_ref.as_deref(), Some("openai-prod"));
    }

    #[test]
    fn ai_backend_proto_from_body_rejects_mismatched_remote_provider_body() {
        let err = ai_backend_proto_from_body(
            AiBackendWriteBody {
                display_name: "Remote OpenAI".to_string(),
                provider: "openai".to_string(),
                model: "gpt-4o-mini".to_string(),
                hosting: "remote".to_string(),
                backend_kind: "openai_chat_completion".to_string(),
                management_mode: None,
                credential_ref: None,
                desired_state: None,
                spec: AiBackendSpecBody {
                    base_url: Some(String::new()),
                    operations: vec!["chat_completions".to_string()],
                    features: None,
                    transports: None,
                    weight: None,
                    priority: None,
                },
                labels: None,
                local: None,
                remote: Some(AiBackendRemoteProviderWriteBody::Ollama(
                    AiBackendOllamaRemoteWriteBody {
                        base_url: Some("http://ollama.local:11434".to_string()),
                        operations: Some(vec!["chat_completions".to_string()]),
                        features: None,
                        transports: Some(vec!["http".to_string()]),
                    },
                )),
                metadata: Some(serde_json::json!({})),
            },
            "backend-5".to_string(),
        )
        .expect_err("mismatched remote provider body should fail");

        assert_eq!(err, "remote.provider_family must match backend provider openai");
    }

    #[test]
    fn ai_backend_proto_from_body_rejects_duplicate_remote_openai_sources() {
        let err = ai_backend_proto_from_body(
            AiBackendWriteBody {
                display_name: "Remote OpenAI".to_string(),
                provider: "openai".to_string(),
                model: "gpt-4o-mini".to_string(),
                hosting: "remote".to_string(),
                backend_kind: "openai_chat_completion".to_string(),
                management_mode: None,
                credential_ref: Some("legacy-openai".to_string()),
                desired_state: None,
                spec: AiBackendSpecBody {
                    base_url: Some("https://legacy.example.com/v1".to_string()),
                    operations: vec!["chat_completions".to_string()],
                    features: Some(vec!["streaming".to_string()]),
                    transports: Some(vec!["http".to_string()]),
                    weight: None,
                    priority: None,
                },
                labels: None,
                local: None,
                remote: Some(AiBackendRemoteProviderWriteBody::OpenAiCompatible(
                    AiBackendOpenAiCompatibleRemoteWriteBody {
                        base_url: Some("https://api.openai.com/v1".to_string()),
                        credential_ref: Some("openai-prod".to_string()),
                        operations: Some(vec!["chat_completions".to_string()]),
                        features: Some(vec!["streaming".to_string()]),
                        transports: Some(vec!["http".to_string()]),
                    },
                )),
                metadata: Some(serde_json::json!({})),
            },
            "backend-remote-dup".to_string(),
        )
        .expect_err("duplicate remote structured and legacy fields should fail");

        assert_eq!(
            err,
            "remote openai structured input cannot be combined with legacy fields: spec.base_url, credential_ref, spec.operations, spec.features, spec.transports"
        );
    }

    #[test]
    fn ai_backend_proto_from_body_requires_remote_structured_input_for_openai() {
        let err = ai_backend_proto_from_body(
            AiBackendWriteBody {
                display_name: "Remote OpenAI".to_string(),
                provider: "openai".to_string(),
                model: "gpt-4o-mini".to_string(),
                hosting: "remote".to_string(),
                backend_kind: "openai_chat_completion".to_string(),
                management_mode: None,
                credential_ref: None,
                desired_state: None,
                spec: AiBackendSpecBody {
                    base_url: Some("https://api.openai.com/v1".to_string()),
                    operations: vec!["chat_completions".to_string()],
                    features: Some(vec!["streaming".to_string()]),
                    transports: Some(vec!["http".to_string()]),
                    weight: None,
                    priority: None,
                },
                labels: None,
                local: None,
                remote: None,
                metadata: Some(serde_json::json!({})),
            },
            "backend-remote-required".to_string(),
        )
        .expect_err("openai should require structured remote input");

        assert_eq!(
            err,
            "remote provider-specific input is required for backend provider openai"
        );
    }
}
