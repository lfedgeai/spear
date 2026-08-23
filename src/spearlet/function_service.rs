//! Function service implementation for spearlet
//! spearlet的函数服务实现

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use tonic::transport::Channel;
use tonic::{Request, Response, Status};
use tracing::debug;
use uuid::Uuid;

use crate::proto::spearlet::{
    execution_service_server::ExecutionService, invocation_service_server::InvocationService,
    Error as ProtoError, Execution, ExecutionMode, ExecutionStatus, GetExecutionRequest,
    InvokeRequest, InvokeResponse, ListExecutionsRequest, ListExecutionsResponse, Payload,
    TerminateExecutionRequest, TerminateExecutionResponse,
};

use crate::spearlet::execution::{
    execution_status::ExecutionPublicStatus,
    runtime::{ResourcePoolConfig, RuntimeConfig, RuntimeFactory, RuntimeManager},
    ExecutionError, ExecutionResponse, InstancePool, InstancePoolConfig, InstanceScheduler,
    SchedulingPolicy, TaskExecutionManager, TaskExecutionManagerConfig,
    DEFAULT_ENTRY_FUNCTION_NAME,
};
use crate::spearlet::SpearletConfig;

/// Function service statistics / 函数服务统计信息
#[derive(Debug, Clone)]
pub struct FunctionServiceStats {
    pub task_count: usize,
    pub execution_count: usize,
    pub running_executions: usize,
    pub artifact_count: usize,
    pub instance_count: usize,
    pub successful_executions: usize,
    pub failed_executions: usize,
    pub average_response_time_ms: f64,
}

/// Function service implementation / 函数服务实现
#[derive(Clone)]
pub struct FunctionServiceImpl {
    /// Task execution manager / 任务执行管理器
    execution_manager: Arc<TaskExecutionManager>,
    /// Instance pool / 实例池
    instance_pool: Arc<InstancePool>,
    /// Service statistics / 服务统计信息
    stats: Arc<RwLock<FunctionServiceStats>>,
}

impl FunctionServiceImpl {
    /// Create new function service / 创建新的函数服务
    pub async fn new(
        config: Arc<SpearletConfig>,
        sms_channel: Option<Channel>,
    ) -> Result<Self, ExecutionError> {
        let mut rm = RuntimeManager::new();
        let global_environment = crate::spearlet::ai::collect_ai_global_environment(&config);
        let default_configs: Vec<RuntimeConfig> = RuntimeFactory::available_runtimes()
            .into_iter()
            .map(|rt| RuntimeConfig {
                runtime_type: rt,
                settings: HashMap::new(),
                global_environment: global_environment.clone(),
                spearlet_config: Some((*config).clone()),
                resource_pool: ResourcePoolConfig::default(),
            })
            .collect();
        rm.initialize_runtimes(default_configs)?;
        let runtime_manager = Arc::new(rm);

        // Create execution manager / 创建执行管理器
        let manager_config = TaskExecutionManagerConfig::default();
        let execution_manager =
            TaskExecutionManager::new(manager_config, runtime_manager, config.clone(), sms_channel)
                .await?;

        // Create instance pool / 创建实例池
        let pool_config = InstancePoolConfig::default();
        let scheduler = Arc::new(InstanceScheduler::new(SchedulingPolicy::RoundRobin));
        let instance_pool = InstancePool::new(pool_config, scheduler).await?;

        // Initialize statistics / 初始化统计信息
        let stats = Arc::new(RwLock::new(FunctionServiceStats {
            task_count: 0,
            execution_count: 0,
            running_executions: 0,
            artifact_count: 0,
            instance_count: 0,
            successful_executions: 0,
            failed_executions: 0,
            average_response_time_ms: 0.0,
        }));

        Ok(Self {
            execution_manager,
            instance_pool,
            stats,
        })
    }

    pub fn get_execution_manager(&self) -> Arc<TaskExecutionManager> {
        self.execution_manager.clone()
    }

    /// Generate execution ID / 生成执行ID
    fn generate_execution_id(&self) -> String {
        Uuid::new_v4().to_string()
    }

    fn to_proto_status(status: &str) -> i32 {
        ExecutionPublicStatus::from_public_str(status).to_spearlet_proto()
    }

    fn execution_error_to_proto(error_message: Option<String>) -> Option<ProtoError> {
        error_message.map(|message| ProtoError {
            code: "EXECUTION_ERROR".to_string(),
            message,
        })
    }

    fn system_time_to_timestamp(t: SystemTime) -> Option<prost_types::Timestamp> {
        let d = t.duration_since(UNIX_EPOCH).ok()?;
        Some(prost_types::Timestamp {
            seconds: d.as_secs() as i64,
            nanos: d.subsec_nanos() as i32,
        })
    }

    fn normalize_invoke_request(&self, mut req: InvokeRequest) -> Result<InvokeRequest, Status> {
        if req.invocation_id.is_empty() {
            req.invocation_id = Uuid::new_v4().to_string();
        }
        if req.execution_id.is_empty() {
            req.execution_id = self.generate_execution_id();
        }
        if req.task_id.is_empty() {
            return Err(Status::invalid_argument("task_id is required"));
        }
        if req.function_name.is_empty() {
            req.function_name = DEFAULT_ENTRY_FUNCTION_NAME.to_string();
        }
        if req.mode == 0 {
            req.mode = ExecutionMode::Sync as i32;
        }
        Ok(req)
    }

    fn payload_with_content_type(
        content_type: String,
        data: Vec<u8>,
        include_output: bool,
    ) -> Option<Payload> {
        Some(Payload {
            content_type,
            data: if include_output { data } else { Vec::new() },
        })
    }

    fn invoke_response_from_execution(
        &self,
        invocation_id: String,
        execution_id: String,
        input_content_type: String,
        execution: ExecutionResponse,
    ) -> InvokeResponse {
        let completed = execution.is_completed();
        let timestamp = execution.timestamp;
        InvokeResponse {
            invocation_id,
            execution_id,
            instance_id: execution.instance_id,
            status: Self::to_proto_status(execution.status.as_str()),
            output: Self::payload_with_content_type(
                input_content_type,
                execution.output_data,
                true,
            ),
            error: Self::execution_error_to_proto(execution.error_message),
            started_at: Self::system_time_to_timestamp(timestamp),
            completed_at: if completed {
                Self::system_time_to_timestamp(timestamp)
            } else {
                None
            },
        }
    }

    fn execution_to_proto(
        execution: ExecutionResponse,
        include_output: bool,
        output_content_type: &str,
    ) -> Execution {
        let completed = execution.is_completed();
        let timestamp = execution.timestamp;
        Execution {
            invocation_id: execution.invocation_id,
            execution_id: execution.execution_id,
            task_id: execution.task_id,
            function_name: execution.function_name,
            instance_id: execution.instance_id,
            status: Self::to_proto_status(execution.status.as_str()),
            output: Self::payload_with_content_type(
                output_content_type.to_string(),
                execution.output_data,
                include_output,
            ),
            error: Self::execution_error_to_proto(execution.error_message),
            started_at: Self::system_time_to_timestamp(timestamp),
            completed_at: if completed {
                Self::system_time_to_timestamp(timestamp)
            } else {
                None
            },
        }
    }

    fn map_terminate_execution_error(error: ExecutionError) -> Status {
        match error {
            ExecutionError::InvalidRequest { message }
                if message.starts_with("execution not found:") =>
            {
                Status::not_found(message)
            }
            other => Status::internal(other.to_string()),
        }
    }

    async fn invoke_once(&self, req: InvokeRequest) -> Result<InvokeResponse, Status> {
        let req = self.normalize_invoke_request(req)?;
        let execution_id = req.execution_id.clone();
        let invocation_id = req.invocation_id.clone();
        let input_ct = req
            .input
            .as_ref()
            .map(|p| p.content_type.clone())
            .unwrap_or_else(|| "application/octet-stream".to_string());

        let resp = self
            .execution_manager
            .submit_invocation(req)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(self.invoke_response_from_execution(invocation_id, execution_id, input_ct, resp))
    }

    /// Get service statistics / 获取服务统计信息
    pub async fn get_stats(&self) -> FunctionServiceStats {
        // Update statistics from execution manager and instance pool
        // 从执行管理器和实例池更新统计信息
        let execution_stats = self.execution_manager.get_statistics();
        let pool_metrics = self.instance_pool.get_global_metrics();

        let mut stats = self.stats.write().await;
        stats.task_count = self.execution_manager.list_tasks().len();
        stats.artifact_count = self.execution_manager.list_artifacts().len();
        stats.instance_count = pool_metrics.total_instances as usize;
        // Use active_instances instead of non-existent active_executions
        // 使用 active_instances 而不是不存在的 active_executions
        stats.running_executions = execution_stats.active_instances as usize;
        stats.successful_executions = execution_stats.successful_executions as usize;
        stats.failed_executions = execution_stats.failed_executions as usize;
        stats.average_response_time_ms = pool_metrics.average_response_time_ms;

        stats.clone()
    }
}

impl From<Arc<FunctionServiceImpl>> for FunctionServiceImpl {
    fn from(value: Arc<FunctionServiceImpl>) -> Self {
        value.as_ref().clone()
    }
}

#[tonic::async_trait]
impl InvocationService for FunctionServiceImpl {
    async fn invoke(
        &self,
        request: Request<InvokeRequest>,
    ) -> Result<Response<InvokeResponse>, Status> {
        debug!("received invoke request");
        Ok(Response::new(self.invoke_once(request.into_inner()).await?))
    }
}

#[tonic::async_trait]
impl ExecutionService for FunctionServiceImpl {
    async fn get_execution(
        &self,
        request: Request<GetExecutionRequest>,
    ) -> Result<Response<Execution>, Status> {
        let req = request.into_inner();
        let Some(resp) = self
            .execution_manager
            .get_execution_status(&req.execution_id)
        else {
            return Err(Status::not_found("execution not found"));
        };
        Ok(Response::new(Self::execution_to_proto(
            resp,
            req.include_output,
            "application/octet-stream",
        )))
    }

    async fn terminate_execution(
        &self,
        request: Request<TerminateExecutionRequest>,
    ) -> Result<Response<TerminateExecutionResponse>, Status> {
        let req = request.into_inner();
        let reason = if req.reason.is_empty() {
            None
        } else {
            Some(req.reason)
        };
        self.execution_manager
            .request_execution_termination(&req.execution_id, reason)
            .await
            .map_err(Self::map_terminate_execution_error)?;

        Ok(Response::new(TerminateExecutionResponse {
            success: true,
            final_status: ExecutionStatus::Terminated as i32,
            message: "terminate requested".to_string(),
        }))
    }

    async fn list_executions(
        &self,
        request: Request<ListExecutionsRequest>,
    ) -> Result<Response<ListExecutionsResponse>, Status> {
        let req = request.into_inner();
        let limit = if req.limit == 0 { 50 } else { req.limit } as usize;
        let items = self.execution_manager.list_executions(
            if req.task_id.is_empty() {
                None
            } else {
                Some(req.task_id.as_str())
            },
            if req.invocation_id.is_empty() {
                None
            } else {
                Some(req.invocation_id.as_str())
            },
            limit,
        );

        let executions = items
            .into_iter()
            .map(|execution| Self::execution_to_proto(execution, false, "application/octet-stream"))
            .collect();

        Ok(Response::new(ListExecutionsResponse {
            executions,
            next_page_token: String::new(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::time::SystemTime;

    #[tokio::test]
    async fn test_function_service_initializes_runtimes() {
        let svc =
            FunctionServiceImpl::new(Arc::new(crate::spearlet::SpearletConfig::default()), None)
                .await
                .unwrap();
        let mgr = svc.get_execution_manager();
        let types = mgr.list_runtime_types();
        assert!(types.contains(&crate::spearlet::execution::RuntimeType::Process));
        assert!(types.contains(&crate::spearlet::execution::RuntimeType::Wasm));
        assert!(types.contains(&crate::spearlet::execution::RuntimeType::Kubernetes));
    }

    #[tokio::test]
    async fn test_normalize_invoke_request_applies_defaults() {
        let service =
            FunctionServiceImpl::new(Arc::new(crate::spearlet::SpearletConfig::default()), None)
                .await
                .unwrap();

        let normalized = service
            .normalize_invoke_request(InvokeRequest {
                invocation_id: String::new(),
                execution_id: String::new(),
                task_id: "task-a".to_string(),
                function_name: String::new(),
                input: None,
                headers: HashMap::new(),
                environment: HashMap::new(),
                mode: 0,
                session_id: String::new(),
                force_new_instance: false,
                metadata: HashMap::new(),
                timeout_ms: 0,
            })
            .unwrap();

        assert!(!normalized.invocation_id.is_empty());
        assert!(!normalized.execution_id.is_empty());
        assert_eq!(
            normalized.function_name,
            DEFAULT_ENTRY_FUNCTION_NAME.to_string()
        );
        assert_eq!(normalized.mode, ExecutionMode::Sync as i32);
    }

    #[test]
    fn test_execution_to_proto_hides_output_when_not_requested() {
        let execution = ExecutionResponse {
            execution_id: "exec-1".to_string(),
            invocation_id: "inv-1".to_string(),
            task_id: "task-1".to_string(),
            function_name: "run".to_string(),
            instance_id: "inst-1".to_string(),
            output_data: b"payload".to_vec(),
            status: "completed".to_string(),
            error_message: None,
            execution_time_ms: 12,
            metadata: HashMap::new(),
            timestamp: SystemTime::now(),
        };

        let proto =
            FunctionServiceImpl::execution_to_proto(execution, false, "application/octet-stream");

        assert_eq!(proto.output.unwrap().data, Vec::<u8>::new());
        assert_eq!(proto.status, ExecutionStatus::Completed as i32);
    }
}
