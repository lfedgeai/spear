use spear_next::proto::sms::{
    node_service_server::NodeServiceServer, placement_service_server::PlacementServiceServer,
    task_service_client::TaskServiceClient, task_service_server::TaskServiceServer,
    RegisterTaskRequest, TaskExecutable,
};
use spear_next::proto::spearlet::{ExecutionMode, InvokeRequest, Payload};
use spear_next::sms::service::SmsServiceImpl;
use spear_next::spearlet::execution::instance::{
    InstanceConfig, InstanceResourceLimits, TaskInstance,
};
use spear_next::spearlet::execution::manager::{TaskExecutionManager, TaskExecutionManagerConfig};
use spear_next::spearlet::execution::runtime::{
    self, Runtime, RuntimeCapabilities, RuntimeExecutionResponse, RuntimeType,
};
use spear_next::spearlet::execution::DEFAULT_ENTRY_FUNCTION_NAME;
use std::sync::Arc;
use tokio::net::TcpListener;
use tonic::transport::Server;

struct DummyRuntime {
    ty: RuntimeType,
}

#[tonic::async_trait]
impl Runtime for DummyRuntime {
    fn runtime_type(&self) -> RuntimeType {
        self.ty
    }

    async fn create_instance(
        &self,
        config: &InstanceConfig,
    ) -> spear_next::spearlet::execution::ExecutionResult<Arc<TaskInstance>> {
        Ok(Arc::new(TaskInstance::new(
            config.task_id.clone(),
            config.clone(),
        )))
    }

    async fn start_instance(
        &self,
        _instance: &Arc<TaskInstance>,
    ) -> spear_next::spearlet::execution::ExecutionResult<()> {
        Ok(())
    }

    async fn stop_instance(
        &self,
        _instance: &Arc<TaskInstance>,
    ) -> spear_next::spearlet::execution::ExecutionResult<()> {
        Ok(())
    }

    async fn execute(
        &self,
        _instance: &Arc<TaskInstance>,
        context: runtime::ExecutionContext,
    ) -> spear_next::spearlet::execution::ExecutionResult<RuntimeExecutionResponse> {
        Ok(RuntimeExecutionResponse::new_sync(
            context.execution_id,
            b"ok".to_vec(),
            1,
        ))
    }

    async fn health_check(
        &self,
        _instance: &Arc<TaskInstance>,
    ) -> spear_next::spearlet::execution::ExecutionResult<bool> {
        Ok(true)
    }

    async fn get_metrics(
        &self,
        _instance: &Arc<TaskInstance>,
    ) -> spear_next::spearlet::execution::ExecutionResult<
        std::collections::HashMap<String, serde_json::Value>,
    > {
        Ok(std::collections::HashMap::new())
    }

    async fn scale_instance(
        &self,
        _instance: &Arc<TaskInstance>,
        _new_limits: &InstanceResourceLimits,
    ) -> spear_next::spearlet::execution::ExecutionResult<()> {
        Ok(())
    }

    async fn cleanup_instance(
        &self,
        _instance: &Arc<TaskInstance>,
    ) -> spear_next::spearlet::execution::ExecutionResult<()> {
        Ok(())
    }

    fn validate_config(
        &self,
        _config: &InstanceConfig,
    ) -> spear_next::spearlet::execution::ExecutionResult<()> {
        Ok(())
    }

    fn get_capabilities(&self) -> RuntimeCapabilities {
        RuntimeCapabilities::default()
    }
}

async fn start_sms_grpc() -> (tokio::task::JoinHandle<()>, String) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let sms_service =
        SmsServiceImpl::with_storage_config(&spear_next::config::base::StorageConfig {
            backend: "memory".to_string(),
            ..Default::default()
        })
        .await;

    let handle = tokio::spawn(async move {
        let svc_node = sms_service.clone();
        let svc_task = sms_service.clone();
        Server::builder()
            .add_service(NodeServiceServer::new(svc_node))
            .add_service(TaskServiceServer::new(svc_task))
            .add_service(PlacementServiceServer::new(sms_service))
            .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
            .await
            .unwrap();
    });

    (handle, format!("127.0.0.1:{}", addr.port()))
}

#[tokio::test]
async fn test_spearlet_fetches_task_from_sms_when_missing_locally() {
    let (sms_handle, sms_addr) = start_sms_grpc().await;

    let mut task_client = TaskServiceClient::connect(format!("http://{}", sms_addr))
        .await
        .unwrap();

    let register = task_client
        .register_task(RegisterTaskRequest {
            name: "sms-task".to_string(),
            description: "d".to_string(),
            priority: spear_next::proto::sms::TaskPriority::Normal as i32,
            endpoint: "sms-task".to_string(),
            version: "v1".to_string(),
            capabilities: vec!["c".to_string()],
            metadata: Default::default(),
            config: Default::default(),
            executable: Some(TaskExecutable {
                r#type: 5,
                uri: "file:///bin/foo".to_string(),
                ..Default::default()
            }),
            desired_replicas: 1,
            scheduling_strategy: spear_next::proto::sms::TaskSchedulingStrategy::Spread as i32,
        })
        .await
        .unwrap()
        .into_inner();
    assert!(register.success);
    let task_id = register.task_id;

    let mut rm = runtime::RuntimeManager::new();
    rm.register_runtime(
        RuntimeType::Process,
        Box::new(DummyRuntime {
            ty: RuntimeType::Process,
        }),
    )
    .unwrap();

    let mut cfg = spear_next::spearlet::config::SpearletConfig::default();
    cfg.sms_grpc_addr = sms_addr;
    let manager = TaskExecutionManager::new(
        TaskExecutionManagerConfig::default(),
        Arc::new(rm),
        Arc::new(cfg),
        None,
    )
    .await
    .unwrap();
    manager
        .reconcile_task_replica_assignment(&task_id, 1)
        .await
        .unwrap();

    let resp = manager
        .submit_invocation(InvokeRequest {
            invocation_id: "inv-1".to_string(),
            execution_id: "exec-1".to_string(),
            task_id: task_id.clone(),
            function_name: DEFAULT_ENTRY_FUNCTION_NAME.to_string(),
            input: Some(Payload {
                content_type: "application/octet-stream".to_string(),
                data: Vec::new(),
            }),
            headers: Default::default(),
            environment: Default::default(),
            timeout_ms: 0,
            session_id: String::new(),
            mode: ExecutionMode::Sync as i32,
            force_new_instance: false,
            metadata: Default::default(),
        })
        .await
        .unwrap();

    assert_eq!(resp.status, "completed");
    assert!(manager.get_task(&task_id).is_some());

    sms_handle.abort();
}
