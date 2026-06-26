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
    runtime::{ResourcePoolConfig, RuntimeConfig, RuntimeFactory, RuntimeManager},
    ExecutionError, InstancePool, InstancePoolConfig, InstanceScheduler, SchedulingPolicy,
    TaskExecutionManager, TaskExecutionManagerConfig, DEFAULT_ENTRY_FUNCTION_NAME,
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
        match status {
            "pending" => ExecutionStatus::Pending as i32,
            "running" => ExecutionStatus::Running as i32,
            "completed" => ExecutionStatus::Completed as i32,
            "failed" => ExecutionStatus::Failed as i32,
            "terminated" => ExecutionStatus::Terminated as i32,
            "timeout" => ExecutionStatus::Timeout as i32,
            _ => ExecutionStatus::Unspecified as i32,
        }
    }

    fn system_time_to_timestamp(t: SystemTime) -> Option<prost_types::Timestamp> {
        let d = t.duration_since(UNIX_EPOCH).ok()?;
        Some(prost_types::Timestamp {
            seconds: d.as_secs() as i64,
            nanos: d.subsec_nanos() as i32,
        })
    }

    async fn invoke_once(&self, mut req: InvokeRequest) -> Result<InvokeResponse, Status> {
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

        let instance_id = resp.instance_id.clone();
        let status = Self::to_proto_status(resp.status.as_str());
        let error = resp.error_message.clone().map(|m| ProtoError {
            code: "EXECUTION_ERROR".to_string(),
            message: m,
        });
        let completed = resp.is_completed();
        let timestamp = resp.timestamp;
        let output_data = resp.output_data;

        Ok(InvokeResponse {
            invocation_id,
            execution_id,
            instance_id,
            status,
            output: Some(Payload {
                content_type: input_ct,
                data: output_data,
            }),
            error,
            started_at: Self::system_time_to_timestamp(timestamp),
            completed_at: if completed {
                Self::system_time_to_timestamp(timestamp)
            } else {
                None
            },
        })
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
            .await
            .map_err(|e| Status::internal(e.to_string()))?
        else {
            return Err(Status::not_found("execution not found"));
        };

        let invocation_id = resp.invocation_id.clone();
        let task_id = resp.task_id.clone();
        let function_name = resp.function_name.clone();
        let instance_id = resp.instance_id.clone();

        let output = if req.include_output {
            Some(Payload {
                content_type: "application/octet-stream".to_string(),
                data: resp.output_data.clone(),
            })
        } else {
            Some(Payload {
                content_type: "application/octet-stream".to_string(),
                data: Vec::new(),
            })
        };

        let status = Self::to_proto_status(resp.status.as_str());
        let error = resp.error_message.clone().map(|m| ProtoError {
            code: "EXECUTION_ERROR".to_string(),
            message: m,
        });

        Ok(Response::new(Execution {
            invocation_id,
            execution_id: resp.execution_id.clone(),
            task_id,
            function_name,
            instance_id,
            status,
            output,
            error,
            started_at: Self::system_time_to_timestamp(resp.timestamp),
            completed_at: if resp.is_completed() {
                Self::system_time_to_timestamp(resp.timestamp)
            } else {
                None
            },
        }))
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
            .terminate_execution(&req.execution_id, reason)
            .await
            .map_err(|e| match e {
                ExecutionError::InvalidRequest { message }
                    if message.starts_with("execution not found:") =>
                {
                    Status::not_found(message)
                }
                _ => Status::internal(e.to_string()),
            })?;

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
            .map(|r| Execution {
                invocation_id: r.invocation_id.clone(),
                execution_id: r.execution_id.clone(),
                task_id: r.task_id.clone(),
                function_name: r.function_name.clone(),
                instance_id: r.instance_id.clone(),
                status: Self::to_proto_status(r.status.as_str()),
                output: Some(Payload {
                    content_type: "application/octet-stream".to_string(),
                    data: Vec::new(),
                }),
                error: r.error_message.clone().map(|m| ProtoError {
                    code: "EXECUTION_ERROR".to_string(),
                    message: m,
                }),
                started_at: Self::system_time_to_timestamp(r.timestamp),
                completed_at: if r.is_completed() {
                    Self::system_time_to_timestamp(r.timestamp)
                } else {
                    None
                },
            })
            .collect();

        Ok(Response::new(ListExecutionsResponse {
            executions,
            next_page_token: String::new(),
        }))
    }
}

#[tonic::async_trait]
impl InvocationService for Arc<FunctionServiceImpl> {
    async fn invoke(
        &self,
        request: Request<InvokeRequest>,
    ) -> Result<Response<InvokeResponse>, Status> {
        (**self).invoke(request).await
    }
}

#[tonic::async_trait]
impl ExecutionService for Arc<FunctionServiceImpl> {
    async fn get_execution(
        &self,
        request: Request<GetExecutionRequest>,
    ) -> Result<Response<Execution>, Status> {
        (**self).get_execution(request).await
    }

    async fn terminate_execution(
        &self,
        request: Request<TerminateExecutionRequest>,
    ) -> Result<Response<TerminateExecutionResponse>, Status> {
        (**self).terminate_execution(request).await
    }

    async fn list_executions(
        &self,
        request: Request<ListExecutionsRequest>,
    ) -> Result<Response<ListExecutionsResponse>, Status> {
        (**self).list_executions(request).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
