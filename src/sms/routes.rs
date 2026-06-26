//! HTTP routes for SPEAR Metadata Server
//! SPEAR元数据服务器的HTTP路由
//!
//! This module defines all HTTP routes and their mappings to handlers
//! 此模块定义所有HTTP路由及其到处理器的映射

use axum::{
    routing::{delete, get, post, put},
    Router,
};

use super::gateway::GatewayState;
use super::handlers::{
    console_index, console_static, create_stream_session, delete_file, delete_node, download_file,
    endpoint_ws_proxy, get_execution, get_file_meta, get_instance, get_node, get_node_resource,
    get_node_with_resource, get_task, health_check, heartbeat, list_files,
    list_instance_executions, list_node_resources, list_nodes, list_task_instances, list_tasks,
    openapi_spec, place_invocation, presign_upload, register_node, register_task,
    report_invocation_outcome, stream_ws_proxy, swagger_ui, swagger_ui_assets, unregister_task,
    update_node, update_node_resource, upload_file,
};

/// Create HTTP routes / 创建HTTP路由
pub(crate) fn create_routes(state: GatewayState) -> Router {
    let mut r = Router::new();
    if state.config.enable_console {
        r = r
            // SPEAR Console (user-facing) / 用户前端
            .route("/console", get(console_index))
            .route("/console/", get(console_index))
            .route("/console/static/{*path}", get(console_static));
    }
    r
        // Node management endpoints / 节点管理端点
        .route("/api/v1/nodes", post(register_node))
        .route("/api/v1/nodes", get(list_nodes))
        .route("/api/v1/nodes/{uuid}", get(get_node))
        .route("/api/v1/nodes/{uuid}", put(update_node))
        .route("/api/v1/nodes/{uuid}", delete(delete_node))
        .route("/api/v1/nodes/{uuid}/heartbeat", post(heartbeat))
        // Node resource management endpoints / 节点资源管理端点
        .route("/api/v1/nodes/{uuid}/resource", put(update_node_resource))
        .route("/api/v1/nodes/{uuid}/resource", get(get_node_resource))
        .route("/api/v1/resources", get(list_node_resources))
        .route(
            "/api/v1/nodes/{uuid}/with-resource",
            get(get_node_with_resource),
        )
        // Task management endpoints / 任务管理端点
        .route("/api/v1/tasks", post(register_task))
        .route("/api/v1/tasks", get(list_tasks))
        .route("/api/v1/tasks/{task_id}", get(get_task))
        .route("/api/v1/tasks/{task_id}", delete(unregister_task))
        .route(
            "/api/v1/tasks/{task_id}/instances",
            get(list_task_instances),
        )
        .route(
            "/api/v1/instances/{instance_id}",
            get(get_instance),
        )
        .route(
            "/api/v1/instances/{instance_id}/executions",
            get(list_instance_executions),
        )
        .route("/api/v1/executions/{execution_id}", get(get_execution))
        // Stream session / WS proxy endpoints / 流会话与WS代理端点
        .route(
            "/api/v1/executions/{execution_id}/streams/session",
            post(create_stream_session),
        )
        .route(
            "/api/v1/executions/{execution_id}/streams/ws",
            get(stream_ws_proxy),
        )
        // Endpoint gateway / Endpoint 网关
        .route("/e/{endpoint}/ws", get(endpoint_ws_proxy))
        .route(
            "/api/v1/placement/invocations/place",
            post(place_invocation),
        )
        .route(
            "/api/v1/placement/invocations/report-outcome",
            post(report_invocation_outcome),
        )
        // API documentation endpoints / API文档端点
        .route("/api/openapi.json", get(openapi_spec))
        .route("/swagger-ui/", get(swagger_ui))
        .route("/swagger-ui/{*file}", get(swagger_ui_assets))
        // Health check endpoint / 健康检查端点
        .route("/health", get(health_check))
        .route("/api/v1/files/presign-upload", post(presign_upload))
        .route("/api/v1/files", get(list_files))
        .route("/api/v1/files", post(upload_file))
        .route("/api/v1/files/{id}", get(download_file))
        .route("/api/v1/files/{id}", delete(delete_file))
        .route("/api/v1/files/{id}/meta", get(get_file_meta))
        .with_state(state)
}
