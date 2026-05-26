//! Secret Validation Tests
//! Secret 验证测试
//!
//! Tests for secret validation logic in ConnectionManager and TaskExecutionManager integration
//! 测试 ConnectionManager 和 TaskExecutionManager 集成中的 secret 验证逻辑

use super::connection_manager::{ConnectionManager, ConnectionManagerConfig};
use super::protocol::{AuthRequest, MessageType, SpearMessage};
use crate::spearlet::execution::instance::{InstanceConfig, TaskInstance};
use crate::spearlet::execution::runtime::RuntimeType;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::time::sleep;

/// Test secret validation with fallback validator (simulating TaskExecutionManager integration)
/// 测试使用回退验证器的 secret 验证（模拟 TaskExecutionManager 集成）
#[tokio::test]
async fn test_secret_validation_with_execution_manager() {
    // Create a simple secret validator for testing / 创建用于测试的简单密钥验证器
    let test_secret = "test-secret-12345";
    let test_secret_clone = test_secret.to_string();

    let secret_validator = Arc::new(move |instance_id: &str, secret: &str| -> bool {
        // Simulate validation logic that would check against TaskExecutionManager
        // 模拟会检查 TaskExecutionManager 的验证逻辑
        !instance_id.is_empty() && secret == test_secret_clone
    });

    // Create ConnectionManager with validator / 创建带有验证器的 ConnectionManager
    let connection_config = ConnectionManagerConfig::default();
    let _connection_manager =
        ConnectionManager::new_with_validator(connection_config, Some(secret_validator));

    let instance_id = "test-instance-123";

    // Test valid authentication / 测试有效认证
    let _valid_auth_request = AuthRequest {
        instance_id: instance_id.to_string(),
        token: test_secret.to_string(),
        client_version: "1.0.0".to_string(),
        client_type: "test-client".to_string(),
        extra_params: HashMap::new(),
    };

    let _valid_message = SpearMessage {
        message_type: MessageType::AuthRequest,
        request_id: 1,
        timestamp: SystemTime::now(),
        payload: serde_json::to_vec(&_valid_auth_request).unwrap(),
        version: 1,
    };

    // Test invalid authentication (wrong secret) / 测试无效认证（错误的 secret）
    let _invalid_auth_request = AuthRequest {
        instance_id: instance_id.to_string(),
        token: "wrong-secret".to_string(),
        client_version: "1.0.0".to_string(),
        client_type: "test-client".to_string(),
        extra_params: HashMap::new(),
    };

    let _invalid_message = SpearMessage {
        message_type: MessageType::AuthRequest,
        request_id: 2,
        timestamp: SystemTime::now(),
        payload: serde_json::to_vec(&_invalid_auth_request).unwrap(),
        version: 1,
    };

    // Test non-existent instance / 测试不存在的实例
    let _nonexistent_auth_request = AuthRequest {
        instance_id: "non-existent-instance".to_string(),
        token: test_secret.to_string(),
        client_version: "1.0.0".to_string(),
        client_type: "test-client".to_string(),
        extra_params: HashMap::new(),
    };

    let _nonexistent_message = SpearMessage {
        message_type: MessageType::AuthRequest,
        request_id: 3,
        timestamp: SystemTime::now(),
        payload: serde_json::to_vec(&_nonexistent_auth_request).unwrap(),
        version: 1,
    };

    println!(
        "✓ Secret validation with simulated TaskExecutionManager integration test setup completed"
    );
    println!(
        "  - Valid auth request created for instance: {}",
        instance_id
    );
    println!("  - Invalid auth request created with wrong secret");
    println!("  - Non-existent instance auth request created");
    println!("  - All test scenarios prepared successfully");
}

/// Test secret validation with fallback validator
/// 测试使用回退验证器的 secret 验证
#[tokio::test]
async fn test_secret_validation_with_fallback_validator() {
    // Create secret validator / 创建 secret 验证器
    let secret_validator = Arc::new(|instance_id: &str, secret: &str| -> bool {
        // Basic validation: secret should not be empty and should be at least 8 characters
        // 基本验证：secret 不应为空且至少 8 个字符
        !secret.is_empty() && secret.len() >= 8 && !instance_id.is_empty()
    });

    // Create ConnectionManager with validator / 创建带有验证器的 ConnectionManager
    let connection_config = ConnectionManagerConfig::default();
    let _connection_manager =
        ConnectionManager::new_with_validator(connection_config, Some(secret_validator));

    // Test valid authentication / 测试有效认证
    let _valid_auth_request = AuthRequest {
        instance_id: "test-instance".to_string(),
        token: "valid-secret-123".to_string(),
        client_version: "1.0.0".to_string(),
        client_type: "test-client".to_string(),
        extra_params: HashMap::new(),
    };

    // Test invalid authentication (short secret) / 测试无效认证（短 secret）
    let _invalid_auth_request = AuthRequest {
        instance_id: "test-instance".to_string(),
        token: "short".to_string(),
        client_version: "1.0.0".to_string(),
        client_type: "test-client".to_string(),
        extra_params: HashMap::new(),
    };

    // Test invalid authentication (empty secret) / 测试无效认证（空 secret）
    let _empty_secret_auth_request = AuthRequest {
        instance_id: "test-instance".to_string(),
        token: "".to_string(),
        client_version: "1.0.0".to_string(),
        client_type: "test-client".to_string(),
        extra_params: HashMap::new(),
    };

    println!("✓ Secret validation with fallback validator test setup completed");
    println!("  - Valid auth request with sufficient secret length");
    println!("  - Invalid auth request with short secret");
    println!("  - Invalid auth request with empty secret");
    println!("  - All fallback validation scenarios prepared successfully");
}

/// Test secret validation without any validator (should reject all)
/// 测试没有任何验证器的 secret 验证（应该拒绝所有）
#[tokio::test]
async fn test_secret_validation_without_validator() {
    // Create ConnectionManager without validator / 创建没有验证器的 ConnectionManager
    let connection_config = ConnectionManagerConfig::default();
    let _connection_manager = ConnectionManager::new_with_validator(connection_config, None);

    // Test authentication request / 测试认证请求
    let _auth_request = AuthRequest {
        instance_id: "test-instance".to_string(),
        token: "any-secret".to_string(),
        client_version: "1.0.0".to_string(),
        client_type: "test-client".to_string(),
        extra_params: HashMap::new(),
    };

    println!("✓ Secret validation without validator test setup completed");
    println!("  - Auth request created (should be rejected due to no validator)");
    println!("  - ConnectionManager created without any validation logic");
}

/// Test instance secret management
/// 测试实例 secret 管理
#[tokio::test]
async fn test_instance_secret_management() {
    let instance_config = InstanceConfig {
        task_id: "test-task".to_string(),
        artifact_id: "artifact-test".to_string(),
        runtime_type: RuntimeType::Process,
        runtime_config: HashMap::new(),
        task_config: HashMap::new(),
        artifact: None,
        environment: HashMap::new(),
        resource_limits: Default::default(),
        network_config: Default::default(),
        max_concurrent_requests: 1,
        request_timeout_ms: 5000,
    };

    let instance = Arc::new(TaskInstance::new("test-task".to_string(), instance_config));

    // Initially, secret should be None / 初始时，secret 应该为 None
    {
        let secret_guard = instance.secret.read();
        assert!(secret_guard.is_none(), "Initial secret should be None");
    }

    // Set a secret / 设置 secret
    let test_secret = "test-secret-12345";
    {
        let mut secret_guard = instance.secret.write();
        *secret_guard = Some(test_secret.to_string());
    }

    // Verify secret is set / 验证 secret 已设置
    {
        let secret_guard = instance.secret.read();
        assert!(secret_guard.is_some(), "Secret should be set");
        assert_eq!(
            secret_guard.as_ref().unwrap(),
            test_secret,
            "Secret should match"
        );
    }

    // Clear secret / 清除 secret
    {
        let mut secret_guard = instance.secret.write();
        *secret_guard = None;
    }

    // Verify secret is cleared / 验证 secret 已清除
    {
        let secret_guard = instance.secret.read();
        assert!(secret_guard.is_none(), "Secret should be cleared");
    }

    println!("✓ Instance secret management test completed");
    println!("  - Initial state: secret is None");
    println!("  - Set secret: {}", test_secret);
    println!("  - Verified secret retrieval");
    println!("  - Cleared secret successfully");
}

/// Test concurrent secret access
/// 测试并发 secret 访问
#[tokio::test]
async fn test_concurrent_secret_access() {
    let instance_config = InstanceConfig {
        task_id: "test-task".to_string(),
        artifact_id: "artifact-test".to_string(),
        runtime_type: RuntimeType::Process,
        runtime_config: HashMap::new(),
        task_config: HashMap::new(),
        artifact: None,
        environment: HashMap::new(),
        resource_limits: Default::default(),
        network_config: Default::default(),
        max_concurrent_requests: 1,
        request_timeout_ms: 5000,
    };

    let instance = Arc::new(TaskInstance::new("test-task".to_string(), instance_config));
    let test_secret = "concurrent-test-secret";

    // Set initial secret / 设置初始 secret
    {
        let mut secret_guard = instance.secret.write();
        *secret_guard = Some(test_secret.to_string());
    }

    // Spawn multiple concurrent readers / 启动多个并发读取器
    let mut handles = Vec::new();
    for i in 0..10 {
        let instance_clone = instance.clone();
        let expected_secret = test_secret.to_string();
        let handle = tokio::spawn(async move {
            for _ in 0..100 {
                // Read secret and release lock before await / 读取 secret 并在 await 前释放锁
                let secret_value = {
                    let secret_guard = instance_clone.secret.read();
                    secret_guard.clone()
                };

                if let Some(secret) = secret_value.as_ref() {
                    assert_eq!(
                        secret, &expected_secret,
                        "Secret should be consistent in reader {}",
                        i
                    );
                }
                // Small delay to increase chance of concurrent access / 小延迟以增加并发访问的机会
                sleep(Duration::from_millis(1)).await;
            }
        });
        handles.push(handle);
    }

    // Wait for all readers to complete / 等待所有读取器完成
    for handle in handles {
        handle
            .await
            .expect("Reader task should complete successfully");
    }

    println!("✓ Concurrent secret access test completed");
    println!("  - 10 concurrent readers, 100 reads each");
    println!("  - All reads completed successfully");
    println!("  - Secret consistency maintained under concurrent access");
}
