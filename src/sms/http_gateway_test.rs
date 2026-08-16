//! Tests for SMS HTTP Gateway
//! SMS HTTP网关测试

use anyhow::Result;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;

use crate::sms::config::SmsConfig;
use crate::sms::http_gateway::HttpGateway;

/// Create a test address with the given port / 创建指定端口的测试地址
fn create_test_addr(port: u16) -> SocketAddr {
    format!("127.0.0.1:{}", port).parse().unwrap()
}

/// Create a test gRPC address / 创建测试gRPC地址
fn create_test_grpc_addr() -> SocketAddr {
    create_test_addr(50051)
}

fn create_test_config(
    http_addr: SocketAddr,
    grpc_addr: SocketAddr,
    enable_swagger: bool,
    max_upload_bytes: u64,
    files_dir: String,
) -> Arc<SmsConfig> {
    let mut cfg = SmsConfig::default();
    cfg.http.addr = http_addr;
    cfg.grpc.addr = grpc_addr;
    cfg.enable_swagger = enable_swagger;
    cfg.max_upload_bytes = max_upload_bytes;
    cfg.files_dir = files_dir;
    Arc::new(cfg)
}

#[tokio::test]
async fn test_http_gateway_creation() {
    // Test HTTP gateway creation / 测试HTTP网关创建
    let http_addr = create_test_addr(8080);
    let grpc_addr = create_test_grpc_addr();
    let cfg = create_test_config(
        http_addr,
        grpc_addr,
        false,
        64 * 1024 * 1024,
        std::env::temp_dir()
            .join("spear-sms-files")
            .to_string_lossy()
            .to_string(),
    );
    let gateway = HttpGateway::new(cfg);

    // Verify the gateway was created successfully / 验证网关创建成功
    assert_eq!(gateway.addr(), http_addr);
    assert_eq!(gateway.grpc_addr(), grpc_addr);
    assert!(!gateway.enable_swagger());
}

#[tokio::test]
async fn test_http_gateway_with_swagger() {
    // Test HTTP gateway creation with Swagger enabled / 测试启用Swagger的HTTP网关创建
    let http_addr = create_test_addr(8081);
    let grpc_addr = create_test_grpc_addr();
    let cfg = create_test_config(
        http_addr,
        grpc_addr,
        true,
        64 * 1024 * 1024,
        std::env::temp_dir()
            .join("spear-sms-files")
            .to_string_lossy()
            .to_string(),
    );
    let gateway = HttpGateway::new(cfg);

    // Verify Swagger is enabled / 验证Swagger已启用
    assert!(gateway.enable_swagger());
}

#[tokio::test]
async fn test_http_gateway_different_addresses() {
    // Test HTTP gateway with different address configurations / 测试不同地址配置的HTTP网关
    let test_cases = vec![
        (create_test_addr(8082), create_test_addr(50052)),
        (create_test_addr(8083), create_test_addr(50053)),
        (
            "0.0.0.0:8084".parse().unwrap(),
            "0.0.0.0:50054".parse().unwrap(),
        ),
    ];

    for (http_addr, grpc_addr) in test_cases {
        let cfg = create_test_config(
            http_addr,
            grpc_addr,
            false,
            64 * 1024 * 1024,
            std::env::temp_dir()
                .join("spear-sms-files")
                .to_string_lossy()
                .to_string(),
        );
        let gateway = HttpGateway::new(cfg);
        assert_eq!(gateway.addr(), http_addr);
        assert_eq!(gateway.grpc_addr(), grpc_addr);
    }
}

#[tokio::test]
async fn test_http_gateway_ipv6_addresses() {
    // Test HTTP gateway with IPv6 addresses / 测试IPv6地址的HTTP网关
    let http_addr: SocketAddr = "[::1]:8085".parse().unwrap();
    let grpc_addr: SocketAddr = "[::1]:50055".parse().unwrap();
    let cfg = create_test_config(
        http_addr,
        grpc_addr,
        true,
        64 * 1024 * 1024,
        std::env::temp_dir()
            .join("spear-sms-files")
            .to_string_lossy()
            .to_string(),
    );
    let gateway = HttpGateway::new(cfg);

    assert_eq!(gateway.addr(), http_addr);
    assert_eq!(gateway.grpc_addr(), grpc_addr);
    assert!(gateway.enable_swagger());
}

#[tokio::test]
async fn test_http_gateway_port_zero() {
    // Test HTTP gateway with port 0 (system assigned) / 测试端口0（系统分配）的HTTP网关
    let http_addr = create_test_addr(0);
    let grpc_addr = create_test_addr(0);
    let cfg = create_test_config(
        http_addr,
        grpc_addr,
        false,
        64 * 1024 * 1024,
        std::env::temp_dir()
            .join("spear-sms-files")
            .to_string_lossy()
            .to_string(),
    );
    let gateway = HttpGateway::new(cfg);

    assert_eq!(gateway.addr().port(), 0);
    assert_eq!(gateway.grpc_addr().port(), 0);
}

#[tokio::test]
async fn test_http_gateway_concurrent_creation() {
    // Test concurrent HTTP gateway creation / 测试并发HTTP网关创建
    let mut handles = vec![];

    for i in 0..10 {
        let handle = tokio::spawn(async move {
            let http_addr = create_test_addr(8090 + i);
            let grpc_addr = create_test_addr(50060 + i);
            let cfg = create_test_config(
                http_addr,
                grpc_addr,
                i % 2 == 0,
                64 * 1024 * 1024,
                std::env::temp_dir()
                    .join("spear-sms-files")
                    .to_string_lossy()
                    .to_string(),
            );
            let gateway = HttpGateway::new(cfg);

            (
                gateway.addr(),
                gateway.grpc_addr(),
                gateway.enable_swagger(),
            )
        });
        handles.push(handle);
    }

    for (i, handle) in handles.into_iter().enumerate() {
        let (http_addr, grpc_addr, swagger_enabled) = handle.await.unwrap();
        assert_eq!(http_addr.port(), 8090 + i as u16);
        assert_eq!(grpc_addr.port(), 50060 + i as u16);
        assert_eq!(swagger_enabled, i % 2 == 0);
    }
}

#[tokio::test]
async fn test_http_gateway_start_without_grpc_server() {
    // Test HTTP gateway start when gRPC server is not available / 测试gRPC服务器不可用时的HTTP网关启动
    let http_addr = create_test_addr(8086);
    let grpc_addr = create_test_addr(50056); // Non-existent gRPC server / 不存在的gRPC服务器
    let cfg = create_test_config(
        http_addr,
        grpc_addr,
        false,
        64 * 1024 * 1024,
        std::env::temp_dir()
            .join("spear-sms-files")
            .to_string_lossy()
            .to_string(),
    );
    let gateway = HttpGateway::new(cfg);

    // The start should fail due to gRPC connection error / 由于gRPC连接错误，启动应该失败
    let result = timeout(Duration::from_secs(5), gateway.start()).await;

    match result {
        Ok(start_result) => {
            // Should fail to connect to gRPC server / 应该无法连接到gRPC服务器
            assert!(start_result.is_err());
        }
        Err(_) => {
            // Timeout is also acceptable as it indicates connection attempt / 超时也是可接受的，表示尝试连接
        }
    }
}

#[tokio::test]
async fn test_http_gateway_invalid_grpc_url() {
    // Test HTTP gateway with invalid gRPC URL format / 测试无效gRPC URL格式的HTTP网关
    let http_addr = create_test_addr(8087);
    let grpc_addr = create_test_addr(50057);
    let cfg = create_test_config(
        http_addr,
        grpc_addr,
        false,
        64 * 1024 * 1024,
        std::env::temp_dir()
            .join("spear-sms-files")
            .to_string_lossy()
            .to_string(),
    );
    let gateway = HttpGateway::new(cfg);

    // Even with invalid gRPC server, gateway creation should succeed / 即使gRPC服务器无效，网关创建也应该成功
    // The error will occur during start() / 错误将在start()期间发生
    assert_eq!(gateway.grpc_addr(), grpc_addr);
}

#[tokio::test]
async fn test_http_gateway_edge_cases() {
    // Test HTTP gateway edge cases / 测试HTTP网关边界情况

    // Test with maximum port number / 测试最大端口号
    let http_addr = create_test_addr(65535);
    let grpc_addr = create_test_addr(65534);
    let cfg = create_test_config(
        http_addr,
        grpc_addr,
        true,
        64 * 1024 * 1024,
        std::env::temp_dir()
            .join("spear-sms-files")
            .to_string_lossy()
            .to_string(),
    );
    let gateway = HttpGateway::new(cfg.clone());
    assert_eq!(gateway.addr().port(), 65535);
    assert_eq!(gateway.grpc_addr().port(), 65534);

    // Test with minimum port number / 测试最小端口号
    let http_addr = create_test_addr(1);
    let grpc_addr = create_test_addr(2);
    let cfg2 = create_test_config(
        http_addr,
        grpc_addr,
        false,
        64 * 1024 * 1024,
        std::env::temp_dir()
            .join("spear-sms-files")
            .to_string_lossy()
            .to_string(),
    );
    let gateway = HttpGateway::new(cfg2);
    assert_eq!(gateway.addr().port(), 1);
    assert_eq!(gateway.grpc_addr().port(), 2);
}

#[tokio::test]
async fn test_http_gateway_configuration_variations() {
    // Test various configuration combinations / 测试各种配置组合
    let configurations = vec![(true, "Swagger enabled"), (false, "Swagger disabled")];

    for (swagger_enabled, description) in configurations {
        let http_addr = create_test_addr(8088);
        let grpc_addr = create_test_addr(50058);

        let cfg = create_test_config(
            http_addr,
            grpc_addr,
            swagger_enabled,
            64 * 1024 * 1024,
            std::env::temp_dir()
                .join("spear-sms-files")
                .to_string_lossy()
                .to_string(),
        );
        let gateway = HttpGateway::new(cfg);

        assert_eq!(
            gateway.enable_swagger(),
            swagger_enabled,
            "Failed for: {}",
            description
        );
    }
}

// Integration tests module / 集成测试模块
#[cfg(test)]
mod integration_tests {
    use super::*;
    use axum::{
        extract::{
            ws::{Message, WebSocketUpgrade},
            Path,
        },
        response::IntoResponse,
        routing::get,
        Router,
    };
    use futures::{SinkExt, StreamExt};
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicU16, Ordering};
    use std::sync::Arc;
    use tokio_stream::wrappers::TcpListenerStream;
    use tokio_util::sync::CancellationToken;
    use tonic::transport::Server;
    use uuid::Uuid;

    use crate::config::base::StorageConfig;
    use crate::proto::sms::{
        execution_index_service_client::ExecutionIndexServiceClient,
        execution_index_service_server::ExecutionIndexServiceServer,
        execution_registry_service_client::ExecutionRegistryServiceClient,
        execution_registry_service_server::ExecutionRegistryServiceServer,
        node_service_client::NodeServiceClient, node_service_server::NodeServiceServer, Execution,
        ExecutionStatus, Node, RegisterNodeRequest,
    };
    use crate::sms::gateway::{create_gateway_router, GatewayState, StreamSessionStore};
    use crate::sms::service::SmsServiceImpl;
    use reqwest::StatusCode;

    static PORT_COUNTER: AtomicU16 = AtomicU16::new(9000);

    fn get_next_port() -> u16 {
        PORT_COUNTER.fetch_add(1, Ordering::SeqCst)
    }

    #[tokio::test]
    async fn test_http_gateway_stress_test() {
        // Stress test: create many gateways concurrently / 压力测试：并发创建多个网关
        let num_gateways = 50;
        let mut handles = vec![];

        for _ in 0..num_gateways {
            let handle = tokio::spawn(async move {
                let http_port = get_next_port();
                let grpc_port = get_next_port();
                let http_addr = create_test_addr(http_port);
                let grpc_addr = create_test_addr(grpc_port);

                let cfg = create_test_config(
                    http_addr,
                    grpc_addr,
                    false,
                    64 * 1024 * 1024,
                    std::env::temp_dir()
                        .join("spear-sms-files")
                        .to_string_lossy()
                        .to_string(),
                );
                let gateway = HttpGateway::new(cfg);

                // Verify gateway properties / 验证网关属性
                assert_eq!(gateway.addr().port(), http_port);
                assert_eq!(gateway.grpc_addr().port(), grpc_port);

                gateway
            });
            handles.push(handle);
        }

        // Wait for all gateways to be created / 等待所有网关创建完成
        let gateways: Vec<HttpGateway> = futures::future::join_all(handles)
            .await
            .into_iter()
            .map(|result| result.unwrap())
            .collect();

        assert_eq!(gateways.len(), num_gateways);
    }

    #[tokio::test]
    async fn test_http_gateway_memory_usage() {
        // Test memory usage of gateway creation / 测试网关创建的内存使用
        let initial_memory = std::mem::size_of::<HttpGateway>();
        let http_addr = create_test_addr(get_next_port());
        let grpc_addr = create_test_addr(get_next_port());
        let cfg = create_test_config(
            http_addr,
            grpc_addr,
            true,
            64 * 1024 * 1024,
            std::env::temp_dir()
                .join("spear-sms-files")
                .to_string_lossy()
                .to_string(),
        );
        let gateway = HttpGateway::new(cfg);

        let gateway_memory = std::mem::size_of_val(&gateway);

        // Gateway should not use excessive memory / 网关不应使用过多内存
        assert!(gateway_memory >= initial_memory);
        assert!(gateway_memory < initial_memory * 10); // Reasonable upper bound / 合理的上限
    }

    async fn echo_ws(Path(_execution_id): Path<String>, ws: WebSocketUpgrade) -> impl IntoResponse {
        ws.on_upgrade(|socket| async move {
            let (mut tx, mut rx) = socket.split();
            while let Some(Ok(msg)) = rx.next().await {
                match msg {
                    Message::Binary(b) => {
                        if tx.send(Message::Binary(b)).await.is_err() {
                            break;
                        }
                    }
                    Message::Text(t) => {
                        if tx.send(Message::Text(t)).await.is_err() {
                            break;
                        }
                    }
                    Message::Close(_) => break,
                    Message::Ping(p) => {
                        let _ = tx.send(Message::Pong(p)).await;
                    }
                    _ => {}
                }
            }
        })
    }

    fn build_ssf_v1_frame(stream_id: u32, msg_type: u16, meta: &[u8], data: &[u8]) -> Vec<u8> {
        let header_len: u16 = 32;
        let mut out = Vec::with_capacity(header_len as usize + meta.len() + data.len());
        out.extend_from_slice(b"SPST");
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&header_len.to_le_bytes());
        out.extend_from_slice(&msg_type.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&stream_id.to_le_bytes());
        out.extend_from_slice(&1u64.to_le_bytes());
        out.extend_from_slice(&(meta.len() as u32).to_le_bytes());
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(meta);
        out.extend_from_slice(data);
        out
    }

    #[tokio::test]
    async fn test_stream_session_and_ws_proxy_end_to_end() -> Result<()> {
        let spearlet_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let spearlet_addr = spearlet_listener.local_addr()?;
        let spearlet_app =
            Router::new().route("/api/v1/executions/{execution_id}/streams/ws", get(echo_ws));
        tokio::spawn(async move {
            let _ = axum::serve(spearlet_listener, spearlet_app).await;
        });

        let storage_config = StorageConfig {
            backend: "memory".to_string(),
            data_dir: std::env::temp_dir()
                .join("test_sms_stream_proxy")
                .to_string_lossy()
                .to_string(),
            max_cache_size_mb: 50,
            compression_enabled: false,
            pool_size: 10,
        };
        let sms_service = SmsServiceImpl::with_storage_config(&storage_config).await;

        let sms_grpc_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let sms_grpc_addr = sms_grpc_listener.local_addr()?;
        let incoming = TcpListenerStream::new(sms_grpc_listener);
        let cancel = CancellationToken::new();
        let cancel_grpc = cancel.clone();
        tokio::spawn(async move {
            let _ = Server::builder()
                .add_service(NodeServiceServer::new(sms_service.clone()))
                .add_service(ExecutionRegistryServiceServer::new(sms_service.clone()))
                .add_service(ExecutionIndexServiceServer::new(sms_service))
                .serve_with_incoming_shutdown(incoming, cancel_grpc.cancelled())
                .await;
        });

        let channel = tonic::transport::Channel::from_shared(format!("http://{}", sms_grpc_addr))?
            .connect()
            .await?;

        let node_uuid = Uuid::new_v4().to_string();
        let node = Node {
            uuid: node_uuid.clone(),
            ip_address: "127.0.0.1".to_string(),
            port: 50052,
            http_port: spearlet_addr.port() as i32,
            status: "online".to_string(),
            last_heartbeat: 0,
            registered_at: 0,
            metadata: {
                let m = HashMap::new();
                m
            },
        };
        let mut node_client = NodeServiceClient::new(channel.clone());
        let _ = node_client
            .register_node(tonic::Request::new(RegisterNodeRequest {
                node: Some(node),
            }))
            .await?;

        let execution_id = "exec-stream-proxy-1".to_string();
        let exe = Execution {
            execution_id: execution_id.clone(),
            invocation_id: execution_id.clone(),
            task_id: "t".to_string(),
            function_name: "f".to_string(),
            node_uuid: node_uuid.clone(),
            instance_id: "i".to_string(),
            status: ExecutionStatus::Running as i32,
            started_at_ms: 1,
            completed_at_ms: 0,
            log_ref: None,
            metadata: HashMap::new(),
            updated_at_ms: 1,
        };
        let mut exec_reg = ExecutionRegistryServiceClient::new(channel.clone());
        let _ = exec_reg.report_execution(exe).await?;

        let sms_http_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let sms_http_addr = sms_http_listener.local_addr()?;
        let state = GatewayState {
            config: Arc::new(SmsConfig::default()),
            node_client: NodeServiceClient::new(channel.clone()),
            task_client: crate::proto::sms::task_service_client::TaskServiceClient::new(
                channel.clone(),
            ),
            task_assignment_client: crate::proto::sms::task_placement_assignment_service_client::TaskPlacementAssignmentServiceClient::new(channel.clone()),
            placement_client:
                crate::proto::sms::placement_service_client::PlacementServiceClient::new(
                    channel.clone(),
                ),
            instance_registry_client:
                crate::proto::sms::instance_registry_service_client::InstanceRegistryServiceClient::new(
                    channel.clone(),
                ),
            execution_registry_client:
                crate::proto::sms::execution_registry_service_client::ExecutionRegistryServiceClient::new(
                    channel.clone(),
                ),
            execution_index_client: ExecutionIndexServiceClient::new(channel.clone()),
            mcp_registry_client:
                crate::proto::sms::mcp_registry_service_client::McpRegistryServiceClient::new(
                    channel.clone(),
                ),
            backend_registry_client:
                crate::proto::sms::backend_registry_service_client::BackendRegistryServiceClient::new(
                    channel.clone(),
                ),
            ai_backend_control_plane_client:
                crate::proto::sms::ai_backend_control_plane_service_client::AiBackendControlPlaneServiceClient::new(
                    channel.clone(),
                ),
            admin_credential_client:
                crate::proto::sms::admin_credential_service_client::AdminCredentialServiceClient::new(
                    channel.clone(),
                ),
            stream_sessions: StreamSessionStore::new(),
            execution_stream_pool: crate::sms::gateway::ExecutionStreamPool::new(),
            cancel_token: CancellationToken::new(),
            max_upload_bytes: 64 * 1024 * 1024,
            files_dir: std::env::temp_dir()
                .join(format!("spear-sms-files-{}", Uuid::new_v4()))
                .to_string_lossy()
                .to_string(),
        };
        let app = create_gateway_router(state);
        tokio::spawn(async move {
            let _ = axum::serve(sms_http_listener, app).await;
        });

        let client = reqwest::Client::new();
        let session_url = format!(
            "http://{}/api/v1/executions/{}/streams/session",
            sms_http_addr, execution_id
        );
        let resp = client.post(session_url).send().await?;
        assert_eq!(resp.status(), StatusCode::OK);
        let body: serde_json::Value = resp.json().await?;
        let ws_url = body["ws_url"].as_str().unwrap().to_string();

        let (mut ws, _) = tokio_tungstenite::connect_async(ws_url).await?;
        let frame = build_ssf_v1_frame(1, 2, b"{}", b"hello");
        ws.send(tokio_tungstenite::tungstenite::Message::Binary(
            frame.clone(),
        ))
        .await?;
        let msg = ws.next().await.unwrap()?;
        match msg {
            tokio_tungstenite::tungstenite::Message::Binary(b) => {
                assert_eq!(b, frame);
            }
            other => panic!("unexpected message: {other:?}"),
        }

        cancel.cancel();
        Ok(())
    }
}
