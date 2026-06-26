//! Tests for registration module
//! 注册模块的测试

use std::sync::Arc;
use tokio::time::Instant;

use crate::spearlet::config::SpearletConfig;
use crate::spearlet::registration::{RegistrationService, RegistrationState};

/// Create test configuration / 创建测试配置
fn create_test_config() -> SpearletConfig {
    SpearletConfig {
        node_name: "test-node-001".to_string(),
        auto_register: true,
        sms_grpc_addr: "127.0.0.1:9000".to_string(),
        ..Default::default()
    }
}

#[tokio::test]
async fn test_registration_state_not_registered() {
    // Test NotRegistered state / 测试未注册状态
    let state = RegistrationState::NotRegistered;

    assert!(!state.is_registered());
    assert!(!state.is_failed());
    assert_eq!(state.status_description(), "Not registered");
}

#[tokio::test]
async fn test_registration_state_registering() {
    // Test Registering state / 测试注册中状态
    let state = RegistrationState::Registering;

    assert!(!state.is_registered());
    assert!(!state.is_failed());
    assert_eq!(state.status_description(), "Registering");
}

#[tokio::test]
async fn test_registration_state_registered() {
    // Test Registered state / 测试已注册状态
    let now = Instant::now();
    let state = RegistrationState::Registered {
        registered_at: now,
        last_heartbeat: now,
    };

    assert!(state.is_registered());
    assert!(!state.is_failed());
    assert_eq!(state.status_description(), "Registered");
}

#[tokio::test]
async fn test_registration_state_failed() {
    // Test Failed state / 测试失败状态
    let state = RegistrationState::Failed {
        error: "Connection failed".to_string(),
        last_attempt: Instant::now(),
    };

    assert!(!state.is_registered());
    assert!(state.is_failed());
    assert_eq!(state.status_description(), "Failed");
}

#[tokio::test]
async fn test_registration_service_creation() {
    // Test registration service creation / 测试注册服务创建
    let config = Arc::new(create_test_config());
    let service = RegistrationService::new(config.clone(), None);

    // Verify initial state / 验证初始状态
    let state = service.get_state().await;
    assert!(!state.is_registered());
    assert!(!state.is_failed());
    assert_eq!(state.status_description(), "Not registered");
}

#[tokio::test]
async fn test_registration_service_with_auto_register_disabled() {
    // Test registration service with auto-register disabled / 测试禁用自动注册的注册服务
    let mut config = create_test_config();
    config.auto_register = false;

    let service = RegistrationService::new(Arc::new(config), None);

    // Verify initial state / 验证初始状态
    let state = service.get_state().await;
    assert!(!state.is_registered());
    assert_eq!(state.status_description(), "Not registered");
}

#[tokio::test]
async fn test_registration_service_with_different_node_ids() {
    // Test registration service with different node IDs / 测试不同节点ID的注册服务
    let node_ids = vec![
        "node-001".to_string(),
        "spearlet-test".to_string(),
        "cluster-node-alpha".to_string(),
    ];

    for node_id in node_ids {
        let mut config = create_test_config();
        config.node_name = node_id.clone();

        let service = RegistrationService::new(Arc::new(config), None);
        let state = service.get_state().await;

        assert!(!state.is_registered());
        assert_eq!(state.status_description(), "Not registered");
    }
}

#[tokio::test]
async fn test_registration_service_with_different_sms_configs() {
    // Test registration service with different SMS configurations / 测试不同SMS配置的注册服务
    let sms_configs = vec![("127.0.0.1", 9000), ("localhost", 8080), ("0.0.0.0", 3000)];

    for (address, port) in sms_configs {
        let mut config = create_test_config();
        config.sms_grpc_addr = format!("{}:{}", address, port);

        let service = RegistrationService::new(Arc::new(config), None);
        let state = service.get_state().await;

        assert!(!state.is_registered());
        assert_eq!(state.status_description(), "Not registered");
    }
}

#[tokio::test]
async fn test_registration_state_transitions() {
    // Test registration state transitions / 测试注册状态转换
    let config = Arc::new(create_test_config());
    let service = RegistrationService::new(config, None);

    // Initial state should be NotRegistered / 初始状态应该是未注册
    let initial_state = service.get_state().await;
    assert!(!initial_state.is_registered());
    assert!(!initial_state.is_failed());

    // Note: We can't easily test actual state transitions without a real SMS server
    // 注意：没有真实的SMS服务器，我们无法轻松测试实际的状态转换
    // This would require integration tests with a mock SMS server
    // 这需要与模拟SMS服务器进行集成测试
}

#[tokio::test]
async fn test_multiple_registration_services() {
    // Test creating multiple registration services / 测试创建多个注册服务
    let services = (0..3)
        .map(|i| {
            let mut config = create_test_config();
            config.node_name = format!("test-node-{:03}", i);
            RegistrationService::new(Arc::new(config), None)
        })
        .collect::<Vec<_>>();

    // All services should be created successfully / 所有服务都应该成功创建
    for service in services {
        let state = service.get_state().await;
        assert!(!state.is_registered());
        assert_eq!(state.status_description(), "Not registered");
    }
}

#[tokio::test]
async fn test_registration_service_disconnect() {
    // Test registration service disconnect / 测试注册服务断开连接
    let config = Arc::new(create_test_config());
    let service = RegistrationService::new(config, None);

    // Disconnect should not panic / 断开连接不应该panic
    service.disconnect().await;

    // State should still be accessible / 状态应该仍然可访问
    let state = service.get_state().await;
    assert!(!state.is_registered());
}

#[cfg(test)]
mod state_tests {
    use super::*;

    #[test]
    fn test_registration_state_clone() {
        // Test that RegistrationState can be cloned / 测试RegistrationState可以被克隆
        let original = RegistrationState::NotRegistered;
        let cloned = original.clone();

        assert_eq!(original.status_description(), cloned.status_description());
        assert_eq!(original.is_registered(), cloned.is_registered());
        assert_eq!(original.is_failed(), cloned.is_failed());
    }

    #[test]
    fn test_registration_state_debug() {
        // Test that RegistrationState implements Debug / 测试RegistrationState实现Debug
        let states = vec![
            RegistrationState::NotRegistered,
            RegistrationState::Registering,
            RegistrationState::Registered {
                registered_at: Instant::now(),
                last_heartbeat: Instant::now(),
            },
            RegistrationState::Failed {
                error: "Test error".to_string(),
                last_attempt: Instant::now(),
            },
        ];

        for state in states {
            let debug_str = format!("{:?}", state);
            assert!(!debug_str.is_empty());
        }
    }

    #[test]
    fn test_registration_state_status_descriptions() {
        // Test all status descriptions / 测试所有状态描述
        let states_and_descriptions = vec![
            (RegistrationState::NotRegistered, "Not registered"),
            (RegistrationState::Registering, "Registering"),
            (
                RegistrationState::Registered {
                    registered_at: Instant::now(),
                    last_heartbeat: Instant::now(),
                },
                "Registered",
            ),
            (
                RegistrationState::Failed {
                    error: "Error".to_string(),
                    last_attempt: Instant::now(),
                },
                "Failed",
            ),
        ];

        for (state, expected_description) in states_and_descriptions {
            assert_eq!(state.status_description(), expected_description);
        }
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;

    #[tokio::test]
    async fn test_registration_service_lifecycle() {
        // Test complete registration service lifecycle / 测试完整的注册服务生命周期
        let config = Arc::new(create_test_config());
        let service = RegistrationService::new(config.clone(), None);

        // 1. Initial state / 初始状态
        let initial_state = service.get_state().await;
        assert!(!initial_state.is_registered());
        assert!(!initial_state.is_failed());

        // 2. Service should be created with correct config / 服务应该使用正确配置创建
        // Note: We can't test actual network operations without a real SMS server
        // 注意：没有真实的SMS服务器，我们无法测试实际的网络操作

        // 3. Disconnect / 断开连接
        service.disconnect().await;

        // 4. State should still be accessible after disconnect / 断开连接后状态应该仍然可访问
        let final_state = service.get_state().await;
        assert!(!final_state.is_registered());
    }

    #[tokio::test]
    async fn test_registration_service_with_various_configs() {
        // Test registration service with various configurations / 测试各种配置的注册服务
        let configs = vec![
            SpearletConfig {
                node_name: "prod-node-001".to_string(),
                auto_register: true,
                sms_grpc_addr: "sms.example.com:443".to_string(),
                ..Default::default()
            },
            SpearletConfig {
                node_name: "dev-node-test".to_string(),
                auto_register: false,
                sms_grpc_addr: "localhost:8080".to_string(),
                ..Default::default()
            },
        ];

        for config in configs {
            let service = RegistrationService::new(Arc::new(config), None);
            let state = service.get_state().await;

            assert!(!state.is_registered());
            assert_eq!(state.status_description(), "Not registered");

            // Test disconnect / 测试断开连接
            service.disconnect().await;
        }
    }
}
#[tokio::test]
async fn test_connect_fails_with_unreachable_sms() {
    use super::RegistrationService;
    use crate::config::base::ServerConfig;
    use crate::spearlet::config::SpearletConfig;
    use crate::spearlet::sms_connector::sms_channel_lazy;
    use std::sync::Arc;

    let cfg = SpearletConfig {
        node_name: "test-node".to_string(),
        max_blocking_threads: 512,
        grpc: ServerConfig {
            addr: "127.0.0.1:0".parse().unwrap(),
            ..Default::default()
        },
        http: crate::spearlet::config::HttpConfig::default(),
        storage: crate::spearlet::config::StorageConfig::default(),
        local_models_dir: String::new(),
        logging: crate::config::base::LogConfig::default(),
        sms_grpc_addr: "127.0.0.1:65535".to_string(),
        sms_http_addr: "127.0.0.1:8080".to_string(),
        auto_register: false,
        heartbeat_interval: 5,
        cleanup_interval: 60,
        sms_connect_timeout_ms: 15000,
        sms_connect_retry_ms: 500,
        reconnect_total_timeout_ms: 300000,
        ai: crate::spearlet::config::AiConfig::default(),
    };

    let cfg = Arc::new(cfg);
    let svc = RegistrationService::new(cfg.clone(), sms_channel_lazy(&cfg).ok());
    let _ = svc.connect_to_sms().await;
    let res = svc.force_register().await;
    assert!(res.is_err());
}

#[tokio::test]
async fn test_connect_and_register_to_sms() {
    use super::RegistrationService;
    use crate::config::base::ServerConfig;
    use crate::sms::config::SmsConfig;
    use crate::sms::grpc_server::GrpcServer as SmsGrpcServer;
    use crate::sms::service::SmsServiceImpl;
    use crate::sms::services::node_service::NodeService;
    use crate::sms::services::resource_service::ResourceService;
    use crate::spearlet::config::SpearletConfig;
    use crate::spearlet::sms_connector::sms_channel_lazy;
    use std::net::SocketAddr;
    use std::sync::Arc;
    use tokio::sync::RwLock;

    // Pick a free port for SMS gRPC / 为SMS gRPC选择一个空闲端口
    let sock = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = sock.local_addr().unwrap().port();
    drop(sock);
    let sms_addr: SocketAddr = format!("127.0.0.1:{}", port).parse().unwrap();

    // Initialize SMS service / 初始化SMS服务
    let node_service = Arc::new(RwLock::new(NodeService::new()));
    let resource_service = Arc::new(ResourceService::new_with_memory());
    let sms_cfg = Arc::new(SmsConfig::default());
    let sms_service = SmsServiceImpl::new(node_service, resource_service, sms_cfg).await;

    // Start SMS gRPC with shutdown channel / 启动SMS gRPC并带关闭通道
    let sms_server = SmsGrpcServer::new(sms_addr, sms_service);
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let server_handle = tokio::spawn(async move {
        let _ = sms_server
            .start_with_shutdown(async move {
                let _ = shutdown_rx.await;
            })
            .await;
    });

    // Prepare Spearlet config / 准备Spearlet配置
    let mut spear_cfg = SpearletConfig::default();
    spear_cfg.grpc = ServerConfig {
        addr: "127.0.0.1:0".parse().unwrap(),
        ..Default::default()
    };
    spear_cfg.http.server = ServerConfig {
        addr: "127.0.0.1:0".parse().unwrap(),
        ..Default::default()
    };
    spear_cfg.sms_grpc_addr = format!("127.0.0.1:{}", port);
    spear_cfg.auto_register = false;

    let spear_cfg = Arc::new(spear_cfg);
    let reg = RegistrationService::new(spear_cfg.clone(), sms_channel_lazy(&spear_cfg).ok());
    // Wait server to start / 等待服务器启动
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    // Connect to SMS / 连接到SMS
    let res = reg.connect_to_sms().await;
    assert!(res.is_ok());

    // Force register / 强制注册
    let reg_res = reg.force_register().await;
    assert!(reg_res.is_ok());

    // Cleanup / 清理
    let _ = shutdown_tx.send(());
    let _ = server_handle.await;
}
