use axum::{
    extract::{Path, Query},
    routing::{delete, get, post},
    Json, Router,
};

use crate::sms::gateway::GatewayState;
use crate::sms::handlers::{
    delete_file, download_execution_logs_admin, download_file, get_execution_logs_admin,
    get_file_meta, list_files, presign_upload, upload_file,
};

use super::types::{ListQuery, PageTokenQuery, StreamQuery};

pub fn create_admin_router(state: GatewayState) -> Router {
    Router::new()
        .route("/", get(super::admin_index))
        .route("/admin", get(super::admin_index))
        .route("/admin/", get(super::admin_index))
        .route("/admin/static/{*path}", get(super::admin_static))
        .route(
            "/admin/api/nodes",
            get({
                let state = state.clone();
                move |q: Query<ListQuery>| super::list_nodes(state.clone(), q)
            }),
        )
        .route(
            "/admin/api/nodes/{uuid}",
            get({
                let state = state.clone();
                move |p: Path<String>| super::get_node_detail(state.clone(), p)
            }),
        )
        .route(
            "/admin/api/nodes/{uuid}/ai/credentials",
            get({
                let state = state.clone();
                move |p: Path<String>| super::get_node_ai_credential_sync(state.clone(), p)
            }),
        )
        .route(
            "/admin/api/nodes/stream",
            get({
                let state = state.clone();
                move |q: Query<StreamQuery>| super::nodes_stream(state.clone(), q)
            }),
        )
        .route(
            "/admin/api/stats",
            get({
                let state = state.clone();
                move || super::get_stats(state.clone())
            }),
        )
        .route(
            "/admin/api/nodes/{uuid}/backends",
            get({
                let state = state.clone();
                move |p: Path<String>| super::get_node_backends(state.clone(), p)
            }),
        )
        .route(
            "/admin/api/ai-models",
            get({
                let state = state.clone();
                move |q: Query<super::types::AiModelsQuery>| super::list_ai_models(state.clone(), q)
            }),
        )
        .route(
            "/admin/api/ai-models/{provider}/{model}",
            get({
                let state = state.clone();
                move |p: Path<(String, String)>, q: Query<super::types::AiModelsQuery>| {
                    super::get_ai_model_detail(state.clone(), p, q)
                }
            }),
        )
        .route(
            "/admin/api/nodes/{uuid}/ai-models",
            post({
                let state = state.clone();
                move |p: Path<String>, body: Json<super::CreateNodeModelDeploymentBody>| {
                    super::create_node_model_deployment(state.clone(), p, body)
                }
            }),
        )
        .route(
            "/admin/api/nodes/{uuid}/ai-models/deployments",
            get({
                let state = state.clone();
                move |p: Path<String>, q: Query<ListQuery>| {
                    super::list_node_model_deployments(state.clone(), p, q)
                }
            }),
        )
        .route(
            "/admin/api/nodes/{uuid}/ai-models/deployments/{deployment_id}",
            delete({
                let state = state.clone();
                move |p: Path<(String, String)>| {
                    super::delete_node_model_deployment(state.clone(), p)
                }
            }),
        )
        .route(
            "/admin/api/ai/remote-backends",
            get({
                let state = state.clone();
                move || super::list_remote_backends_admin(state.clone())
            }),
        )
        .route(
            "/admin/api/ai/remote-backends",
            post({
                let state = state.clone();
                move |body: Json<super::UpsertRemoteBackendBody>| {
                    super::upsert_remote_backend_admin(state.clone(), body)
                }
            }),
        )
        .route(
            "/admin/api/ai/remote-backends/{name}",
            delete({
                let state = state.clone();
                move |p: Path<String>| super::delete_remote_backend_admin(state.clone(), p)
            }),
        )
        .route(
            "/admin/api/ai/credentials",
            get({
                let state = state.clone();
                move || super::list_credentials_admin(state.clone())
            }),
        )
        .route(
            "/admin/api/ai/credentials",
            post({
                let state = state.clone();
                move |body: Json<super::UpsertCredentialBody>| {
                    super::upsert_credential_admin(state.clone(), body)
                }
            }),
        )
        .route(
            "/admin/api/ai/credentials/{name}",
            delete({
                let state = state.clone();
                move |p: Path<String>| super::delete_credential_admin(state.clone(), p)
            }),
        )
        .route(
            "/admin/api/mcp/servers",
            get({
                let state = state.clone();
                move || super::list_mcp_servers(state.clone())
            }),
        )
        .route(
            "/admin/api/mcp/servers/{server_id}",
            get({
                let state = state.clone();
                move |p: Path<String>| super::get_mcp_server(state.clone(), p)
            }),
        )
        .route(
            "/admin/api/mcp/servers",
            post({
                let state = state.clone();
                move |body: Json<super::McpServerUpsertBody>| {
                    super::upsert_mcp_server(state.clone(), body)
                }
            }),
        )
        .route(
            "/admin/api/mcp/servers/{server_id}",
            delete({
                let state = state.clone();
                move |p: Path<String>| super::delete_mcp_server(state.clone(), p)
            }),
        )
        .route(
            "/admin/api/tasks",
            get({
                let state = state.clone();
                move |q: Query<ListQuery>| super::list_tasks(state.clone(), q)
            }),
        )
        .route(
            "/admin/api/tasks",
            post({
                let state = state.clone();
                move |payload: axum::extract::Json<super::CreateTaskBody>| {
                    super::create_task(state.clone(), payload)
                }
            }),
        )
        .route(
            "/admin/api/tasks/{task_id}",
            get({
                let state = state.clone();
                move |p: Path<String>| super::get_task_detail(state.clone(), p)
            }),
        )
        .route(
            "/admin/api/tasks/{task_id}/instances",
            get({
                let state = state.clone();
                move |p: Path<String>, q: Query<PageTokenQuery>| {
                    super::list_task_instances_admin(state.clone(), p, q)
                }
            }),
        )
        .route(
            "/admin/api/instances/{instance_id}",
            get({
                let state = state.clone();
                move |p: Path<String>| super::get_instance_admin(state.clone(), p)
            }),
        )
        .route(
            "/admin/api/instances/{instance_id}/executions",
            get({
                let state = state.clone();
                move |p: Path<String>, q: Query<PageTokenQuery>| {
                    super::list_instance_executions_admin(state.clone(), p, q)
                }
            }),
        )
        .route(
            "/admin/api/instances/{instance_id}/destroy",
            post({
                let state = state.clone();
                move |p: Path<String>, body: axum::extract::Json<super::DestroyInstanceBody>| {
                    super::destroy_instance_admin(state.clone(), p, body)
                }
            }),
        )
        .route(
            "/admin/api/invocations",
            post({
                let state = state.clone();
                move |payload: axum::extract::Json<super::CreateExecutionBody>| {
                    super::create_invocation(state.clone(), payload)
                }
            }),
        )
        .route(
            "/admin/api/executions",
            post({
                let state = state.clone();
                move |payload: axum::extract::Json<super::CreateExecutionBody>| {
                    super::create_invocation(state.clone(), payload)
                }
            }),
        )
        .route(
            "/admin/api/executions/{execution_id}",
            get({
                let state = state.clone();
                move |p: Path<String>| super::get_execution_admin(state.clone(), p)
            }),
        )
        .route(
            "/admin/api/executions/{execution_id}/terminate",
            post({
                let state = state.clone();
                move |p: Path<String>, body: axum::extract::Json<super::TerminateExecutionBody>| {
                    super::terminate_execution_admin(state.clone(), p, body)
                }
            }),
        )
        .route(
            "/admin/api/executions/{execution_id}/logs",
            get(get_execution_logs_admin),
        )
        .route(
            "/admin/api/executions/{execution_id}/logs/download",
            get(download_execution_logs_admin),
        )
        .route("/admin/api/files", get(list_files))
        .route("/admin/api/files/presign-upload", post(presign_upload))
        .route("/admin/api/files", post(upload_file))
        .route("/admin/api/files/{id}", get(download_file))
        .route("/admin/api/files/{id}", delete(delete_file))
        .route("/admin/api/files/{id}/meta", get(get_file_meta))
        .with_state(state)
}
