use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::state::DebugServerState;
use super::ui::{render_index, DebugPageContext};

pub fn router(state: Arc<DebugServerState>) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/health", get(health))
        .route("/clients", get(clients))
        .route("/streams", get(streams))
        .route("/logs", get(logs).delete(delete_logs))
        .route("/event", post(event))
        .with_state(state)
}

#[derive(Debug, Default, Deserialize)]
struct SelectQuery {
    session: Option<String>,
    client: Option<String>,
    stream: Option<String>,
    limit: Option<usize>,
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    ok: bool,
    session: String,
}

#[derive(Debug, Serialize)]
struct ClientsResponse {
    session: String,
    clients: Vec<String>,
}

#[derive(Debug, Serialize)]
struct StreamsResponse {
    session: String,
    sessions: Vec<String>,
    client: String,
    streams: Vec<String>,
    clients: Vec<super::state::ClientStreams>,
}

#[derive(Debug, Serialize)]
struct LogsResponse {
    session: String,
    client: String,
    stream: String,
    lines: Vec<String>,
}

async fn index(
    State(state): State<Arc<DebugServerState>>,
    Query(query): Query<SelectQuery>,
) -> Result<Html<String>, (StatusCode, String)> {
    state.touch();
    let limit = query.limit.unwrap_or(200);
    let selection = state
        .resolve_selection(query.session, query.client, query.stream)
        .map_err(internal_error)?;
    let sessions = state.list_sessions().map_err(internal_error)?;
    let clients = state
        .list_clients(&selection.session)
        .map_err(internal_error)?;
    let streams = state
        .client_streams(&selection.session)
        .map_err(internal_error)?
        .remove(&selection.client)
        .unwrap_or_default();
    let lines = state
        .read_recent_lines(
            &selection.session,
            &selection.client,
            &selection.stream,
            limit,
        )
        .map_err(internal_error)?;
    Ok(Html(render_index(&DebugPageContext {
        session: selection.session,
        client: selection.client,
        stream: selection.stream,
        limit,
        sessions,
        clients,
        streams,
        lines,
    })))
}

async fn health(State(state): State<Arc<DebugServerState>>) -> Json<HealthResponse> {
    state.touch();
    Json(HealthResponse {
        ok: true,
        session: state.default_session().to_string(),
    })
}

async fn clients(
    State(state): State<Arc<DebugServerState>>,
    Query(query): Query<SelectQuery>,
) -> Result<Json<ClientsResponse>, (StatusCode, String)> {
    state.touch();
    let selection = state
        .resolve_selection(query.session, None, None)
        .map_err(internal_error)?;
    Ok(Json(ClientsResponse {
        session: selection.session.clone(),
        clients: state
            .list_clients(&selection.session)
            .map_err(internal_error)?,
    }))
}

async fn streams(
    State(state): State<Arc<DebugServerState>>,
    Query(query): Query<SelectQuery>,
) -> Result<Json<StreamsResponse>, (StatusCode, String)> {
    state.touch();
    let selection = state
        .resolve_selection(query.session, query.client, None)
        .map_err(internal_error)?;
    let sessions = state.list_sessions().map_err(internal_error)?;
    let hierarchy = state
        .client_stream_entries(&selection.session)
        .map_err(internal_error)?;
    let streams = hierarchy
        .iter()
        .find(|entry| entry.name == selection.client)
        .map(|entry| entry.streams.clone())
        .unwrap_or_default();
    Ok(Json(StreamsResponse {
        session: selection.session,
        sessions,
        client: selection.client,
        streams,
        clients: hierarchy,
    }))
}

async fn logs(
    State(state): State<Arc<DebugServerState>>,
    Query(query): Query<SelectQuery>,
) -> Result<Json<LogsResponse>, (StatusCode, String)> {
    state.touch();
    let limit = query.limit.unwrap_or(200);
    let selection = state
        .resolve_selection(query.session, query.client, query.stream)
        .map_err(internal_error)?;
    let lines = state
        .read_recent_lines(
            &selection.session,
            &selection.client,
            &selection.stream,
            limit,
        )
        .map_err(internal_error)?;
    Ok(Json(LogsResponse {
        session: selection.session,
        client: selection.client,
        stream: selection.stream,
        lines,
    }))
}

async fn delete_logs(
    State(state): State<Arc<DebugServerState>>,
    Query(query): Query<SelectQuery>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    state.touch();
    let selection = state
        .resolve_selection(query.session, query.client, query.stream)
        .map_err(internal_error)?;
    state
        .clear_logs(&selection.session, &selection.client, &selection.stream)
        .map_err(internal_error)?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn event(
    State(state): State<Arc<DebugServerState>>,
    Json(payload): Json<Value>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    state.touch();
    state.append_event(payload).map_err(internal_error)?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

fn internal_error(error: impl std::fmt::Display) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::body::{to_bytes, Body};
    use axum::http::Request;
    use tower::util::ServiceExt;

    use super::*;
    use crate::debug_server::config::DebugServerConfig;
    use crate::debug_server::state::DebugServerState;

    fn test_router() -> Router {
        let tempdir = tempfile::tempdir().unwrap();
        let config = DebugServerConfig {
            host: "127.0.0.1".to_string(),
            port: 7777,
            session: "default-session".to_string(),
            outdir: tempdir.path().to_string_lossy().to_string(),
            idle: 0,
        };
        let state = Arc::new(DebugServerState::new(config).unwrap());
        std::mem::forget(tempdir);
        router(state)
    }

    #[tokio::test]
    async fn logs_endpoint_returns_selected_stream() {
        let app = test_router();

        let response = app
            .clone()
            .oneshot(
                Request::post("/event")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"sessionId":"session-b","clientName":"client-b","streamName":"stream-b","msg":"hello"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let response = app
            .oneshot(
                Request::get("/logs?session=session-b&client=client-b&stream=stream-b")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["session"], "session-b");
        assert_eq!(json["client"], "client-b");
        assert_eq!(json["stream"], "stream-b");
        assert_eq!(json["lines"].as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn index_page_contains_dropdown_selectors() {
        let app = test_router();
        let response = app
            .oneshot(Request::get("/").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let html = String::from_utf8(body.to_vec()).unwrap();
        assert!(html.contains("session-select"));
        assert!(html.contains("client-select"));
        assert!(html.contains("stream-select"));
    }
}
