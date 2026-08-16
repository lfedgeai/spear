use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::time::sleep;

use async_trait::async_trait;
use spear_next::proto::sms::{
    task_service_server::TaskServiceServer, GetTaskRequest, RegisterTaskRequest,
};
use spear_next::sms::service::SmsServiceImpl;
use spear_next::spearlet::execution::instance;
use spear_next::spearlet::execution::manager::{TaskExecutionManager, TaskExecutionManagerConfig};
use spear_next::spearlet::execution::runtime::{
    ExecutionContext as RtCtx, Runtime, RuntimeCapabilities, RuntimeExecutionResponse, RuntimeType,
};
use spear_next::spearlet::execution::DEFAULT_ENTRY_FUNCTION_NAME;

struct SuccessRuntime;

#[async_trait]
impl Runtime for SuccessRuntime {
    fn runtime_type(&self) -> RuntimeType {
        RuntimeType::Process
    }
    async fn create_instance(
        &self,
        config: &instance::InstanceConfig,
    ) -> spear_next::spearlet::execution::ExecutionResult<Arc<instance::TaskInstance>> {
        Ok(Arc::new(instance::TaskInstance::new(
            config.task_id.clone(),
            config.clone(),
        )))
    }
    async fn start_instance(
        &self,
        _instance: &Arc<instance::TaskInstance>,
    ) -> spear_next::spearlet::execution::ExecutionResult<()> {
        Ok(())
    }
    async fn stop_instance(
        &self,
        _instance: &Arc<instance::TaskInstance>,
    ) -> spear_next::spearlet::execution::ExecutionResult<()> {
        Ok(())
    }
    async fn execute(
        &self,
        _instance: &Arc<instance::TaskInstance>,
        _context: RtCtx,
    ) -> spear_next::spearlet::execution::ExecutionResult<RuntimeExecutionResponse> {
        Ok(RuntimeExecutionResponse::new_sync(
            "exec-1".to_string(),
            vec![],
            10,
        ))
    }
    async fn health_check(
        &self,
        _instance: &Arc<instance::TaskInstance>,
    ) -> spear_next::spearlet::execution::ExecutionResult<bool> {
        Ok(true)
    }
    async fn get_metrics(
        &self,
        _instance: &Arc<instance::TaskInstance>,
    ) -> spear_next::spearlet::execution::ExecutionResult<
        std::collections::HashMap<String, serde_json::Value>,
    > {
        Ok(std::collections::HashMap::new())
    }
    async fn scale_instance(
        &self,
        _instance: &Arc<instance::TaskInstance>,
        _new_limits: &instance::InstanceResourceLimits,
    ) -> spear_next::spearlet::execution::ExecutionResult<()> {
        Ok(())
    }
    async fn cleanup_instance(
        &self,
        _instance: &Arc<instance::TaskInstance>,
    ) -> spear_next::spearlet::execution::ExecutionResult<()> {
        Ok(())
    }
    fn validate_config(
        &self,
        _config: &instance::InstanceConfig,
    ) -> spear_next::spearlet::execution::ExecutionResult<()> {
        Ok(())
    }
    fn get_capabilities(&self) -> RuntimeCapabilities {
        RuntimeCapabilities::default()
    }
}

#[tokio::test]
async fn test_spearlet_writes_result_on_completion() {
    // Start gRPC server for SMS
    let sms = SmsServiceImpl::with_storage_config(&spear_next::config::base::StorageConfig {
        backend: "memory".to_string(),
        ..Default::default()
    })
    .await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let serve = tokio::spawn(async move {
        tonic::transport::Server::builder()
            .add_service(TaskServiceServer::new(sms))
            .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
            .await
            .unwrap();
    });

    // Register a task
    let channel = tonic::transport::Channel::from_shared(format!("http://{}", addr))
        .unwrap()
        .connect()
        .await
        .unwrap();
    let mut client =
        spear_next::proto::sms::task_service_client::TaskServiceClient::new(channel.clone());
    let reg = RegisterTaskRequest {
        name: "t".to_string(),
        description: "d".to_string(),
        priority: 2,
        endpoint: "t".to_string(),
        version: "v1".to_string(),
        capabilities: vec![],
        metadata: std::collections::HashMap::new(),
        config: std::collections::HashMap::new(),
        executable: None,
        desired_replicas: 1,
        scheduling_strategy: spear_next::proto::sms::TaskSchedulingStrategy::Spread as i32,
    };
    let task_id = client
        .register_task(reg)
        .await
        .unwrap()
        .into_inner()
        .task_id;

    // Setup spearlet manager with runtime and sms addr
    let mut rm = spear_next::spearlet::execution::runtime::RuntimeManager::new();
    rm.register_runtime(RuntimeType::Process, Box::new(SuccessRuntime))
        .unwrap();
    let rm = Arc::new(rm);
    let mut cfg = spear_next::spearlet::config::SpearletConfig::default();
    cfg.sms_grpc_addr = addr.to_string();
    let mgr = TaskExecutionManager::new(
        TaskExecutionManagerConfig::default(),
        rm,
        Arc::new(cfg),
        None,
    )
    .await
    .unwrap();

    // Pre-register task locally using SMS-like details
    let mut sms_task = spear_next::proto::sms::Task::default();
    sms_task.task_id = task_id.clone();
    sms_task.name = "t".to_string();
    sms_task.version = "v1".to_string();
    sms_task.metadata = std::collections::HashMap::new();
    sms_task.config = std::collections::HashMap::new();
    sms_task.executable = Some(spear_next::proto::sms::TaskExecutable {
        r#type: 5,
        uri: "file:///bin/foo".to_string(),
        name: String::new(),
        checksum_sha256: String::new(),
        args: vec![],
        env: std::collections::HashMap::new(),
    });
    let artifact = mgr
        .materialize_local_artifact_from_sms_snapshot(&sms_task)
        .await
        .unwrap();
    let _ = mgr
        .materialize_local_task_from_sms_snapshot_with_artifact(&sms_task, &artifact)
        .await
        .unwrap();
    let task = mgr.get_task_by_id(&task_id).unwrap();
    mgr.create_instance_for_task(&task).await.unwrap();

    let _ = mgr
        .submit_invocation(spear_next::proto::spearlet::InvokeRequest {
            invocation_id: "inv-1".to_string(),
            execution_id: "exec-1".to_string(),
            task_id: task_id.clone(),
            function_name: DEFAULT_ENTRY_FUNCTION_NAME.to_string(),
            input: None,
            headers: Default::default(),
            environment: Default::default(),
            timeout_ms: 0,
            session_id: String::new(),
            mode: spear_next::proto::spearlet::ExecutionMode::Sync as i32,
            force_new_instance: false,
            metadata: Default::default(),
        })
        .await
        .unwrap();

    // Wait for async publish
    sleep(Duration::from_millis(50)).await;

    // Verify task has last_result_* updated
    let got = client
        .get_task(GetTaskRequest {
            task_id: task_id.clone(),
        })
        .await
        .unwrap()
        .into_inner();
    let t = got.task.unwrap();
    assert_eq!(t.task_id, task_id);
    assert_eq!(t.last_result_status, "completed");
    assert!(t.last_completed_at > 0);

    serve.abort();
}
