use tonic::{Request, Response, Status};
use tracing::{debug, warn};

use crate::proto::sms::{
    task_service_server::TaskService as TaskServiceTrait, CompleteTaskDeletionRequest,
    CompleteTaskDeletionResponse, DeleteTaskRequest, DeleteTaskResponse, GetTaskRequest,
    GetTaskResponse, ListTasksRequest, ListTasksResponse, RegisterTaskRequest,
    RegisterTaskResponse, ResolveEndpointRequest, ResolveEndpointResponse,
    Task, TaskEventKind, UnregisterTaskRequest, UnregisterTaskResponse, UpdateTaskResultRequest, UpdateTaskResultResponse,
    UpdateTaskStatusRequest, UpdateTaskStatusResponse,
};
use crate::sms::service::SmsServiceImpl;

impl SmsServiceImpl {
    /// Build a canonical SMS task from a register request / 从注册请求构建规范的 SMS task
    fn build_registered_task(req: RegisterTaskRequest) -> Task {
        let desired_replicas = req.desired_replicas.max(1);
        let scheduling_strategy = if req.scheduling_strategy
            == crate::proto::sms::TaskSchedulingStrategy::Unknown as i32
        {
            crate::proto::sms::TaskSchedulingStrategy::Spread as i32
        } else {
            req.scheduling_strategy
        };
        Task {
            task_id: uuid::Uuid::new_v4().to_string(),
            name: req.name,
            description: req.description,
            status: crate::proto::sms::TaskStatus::Registered as i32,
            priority: req.priority,
            endpoint: req.endpoint,
            version: req.version,
            capabilities: req.capabilities,
            registered_at: chrono::Utc::now().timestamp(),
            last_heartbeat: chrono::Utc::now().timestamp(),
            metadata: req.metadata,
            config: req.config,
            executable: req.executable,
            result_uris: Vec::new(),
            last_result_uri: String::new(),
            last_result_status: String::new(),
            last_completed_at: 0,
            last_result_metadata: std::collections::HashMap::new(),
            deletion_requested_at: 0,
            deletion_reason: String::new(),
            desired_replicas,
            scheduling_strategy,
        }
    }

    /// Publish task events through the unified event bus. / 通过 unified event bus 发布 task 事件。
    async fn publish_task_events(&self, task: &Task, kind: TaskEventKind) {
        if kind == TaskEventKind::Unknown {
            return;
        }
        if let Err(error) = self.unified_events.publish_task_event(task, kind).await {
            warn!(error = %error, task_id = %task.task_id, kind = kind as i32, "Publish task event failed");
        }
    }

    /// Persist task changes and fan out update events / 持久化 task 变更并扇出更新事件
    async fn persist_task_update(&self, task: Task) -> Result<Task, Status> {
        let mut task_service = self.task_service.write().await;
        task_service
            .register_task(task.clone())
            .await
            .map_err(|error| Status::internal(format!("Persist failed: {}", error)))?;
        drop(task_service);
        self.publish_task_events(&task, TaskEventKind::Update).await;
        Ok(task)
    }
}

#[tonic::async_trait]
impl TaskServiceTrait for SmsServiceImpl {
    /// Register a new task / 注册新任务
    async fn register_task(
        &self,
        request: Request<RegisterTaskRequest>,
    ) -> Result<Response<RegisterTaskResponse>, Status> {
        let task = Self::build_registered_task(request.into_inner());
        let mut task_service = self.task_service.write().await;
        match task_service.register_task(task.clone()).await {
            Ok(_) => {
                drop(task_service);
                debug!(task_id = %task.task_id, "RegisterTask: publishing create event");
                self.publish_task_events(&task, TaskEventKind::Create).await;
                if let Err(error) = self.reconcile_task_assignments(&task).await {
                    warn!(task_id = %task.task_id, error = %error, "RegisterTask: reconcile task assignments failed");
                }
                Ok(Response::new(RegisterTaskResponse {
                    success: true,
                    message: "Task registered successfully".to_string(),
                    task_id: task.task_id.clone(),
                    task: Some(task),
                }))
            }
            Err(error) => Ok(Response::new(RegisterTaskResponse {
                success: false,
                message: format!("Failed to register task: {}", error),
                task_id: String::new(),
                task: None,
            })),
        }
    }

    /// List tasks with optional filtering / 列出任务（可选过滤）
    async fn list_tasks(
        &self,
        request: Request<ListTasksRequest>,
    ) -> Result<Response<ListTasksResponse>, Status> {
        let req = request.into_inner();
        let task_service = self.task_service.read().await;

        let status_filter = if req.status_filter < 0 {
            None
        } else {
            Some(req.status_filter)
        };
        let priority_filter = if req.priority_filter < 0 {
            None
        } else {
            Some(req.priority_filter)
        };
        let limit = if req.limit <= 0 {
            None
        } else {
            Some(req.limit)
        };
        let offset = if req.offset < 0 {
            None
        } else {
            Some(req.offset)
        };

        match task_service
            .list_tasks_with_filters(status_filter, priority_filter, limit, offset)
            .await
        {
            Ok(tasks) => {
                let all_tasks = task_service.list_tasks().await.unwrap_or_default();
                Ok(Response::new(ListTasksResponse {
                    tasks: tasks.clone(),
                    total_count: all_tasks.len() as i32,
                }))
            }
            Err(error) => Err(Status::internal(format!("Failed to list tasks: {}", error))),
        }
    }

    /// Get task details by ID / 根据ID获取任务详情
    async fn get_task(
        &self,
        request: Request<GetTaskRequest>,
    ) -> Result<Response<GetTaskResponse>, Status> {
        let req = request.into_inner();
        let task_service = self.task_service.read().await;
        match task_service.get_task(&req.task_id).await {
            Ok(Some(task)) => Ok(Response::new(GetTaskResponse {
                found: true,
                task: Some(task),
            })),
            Ok(None) => Ok(Response::new(GetTaskResponse {
                found: false,
                task: None,
            })),
            Err(error) => Err(Status::internal(format!("Failed to get task: {}", error))),
        }
    }

    /// Resolve task by endpoint / 通过 endpoint 解析任务
    async fn resolve_endpoint(
        &self,
        request: Request<ResolveEndpointRequest>,
    ) -> Result<Response<ResolveEndpointResponse>, Status> {
        let req = request.into_inner();
        let task_service = self.task_service.read().await;
        match task_service.resolve_routable_task_by_endpoint(&req.endpoint).await {
            Ok(Some(task)) => Ok(Response::new(ResolveEndpointResponse {
                found: true,
                task: Some(task),
            })),
            Ok(None) => Ok(Response::new(ResolveEndpointResponse {
                found: false,
                task: None,
            })),
            Err(error) => Err(Status::internal(format!(
                "Failed to resolve endpoint: {}",
                error
            ))),
        }
    }

    /// Unregister a task / 注销任务
    async fn unregister_task(
        &self,
        request: Request<UnregisterTaskRequest>,
    ) -> Result<Response<UnregisterTaskResponse>, Status> {
        let req = request.into_inner();
        let mut task_service = self.task_service.write().await;
        match task_service.remove_task(&req.task_id).await {
            Ok(_) => {
                self.remove_task_assignments_and_publish(
                    &req.task_id,
                    chrono::Utc::now().timestamp_millis(),
                )
                    .await;
                Ok(Response::new(UnregisterTaskResponse {
                    success: true,
                    message: "Task unregistered successfully".to_string(),
                    task_id: req.task_id.clone(),
                }))
            }
            Err(error) => Ok(Response::new(UnregisterTaskResponse {
                success: false,
                message: format!("Failed to unregister task: {}", error),
                task_id: req.task_id.clone(),
            })),
        }
    }

    /// Delete a task with runtime cleanup / 删除任务并回收运行态
    async fn delete_task(
        &self,
        request: Request<DeleteTaskRequest>,
    ) -> Result<Response<DeleteTaskResponse>, Status> {
        let req = request.into_inner();
        if req.task_id.is_empty() {
            return Err(Status::invalid_argument("task_id is required"));
        }

        let mut task_service = self.task_service.write().await;
        let existing = task_service
            .get_task(&req.task_id)
            .await
            .map_err(|error| Status::internal(format!("Failed to get task: {}", error)))?;
        let Some(task) = existing else {
            return Ok(Response::new(DeleteTaskResponse {
                success: true,
                message: "Task already deleted".to_string(),
                task_id: req.task_id,
                task: None,
            }));
        };

        if task.status == crate::proto::sms::TaskStatus::Deleting as i32 {
            return Ok(Response::new(DeleteTaskResponse {
                success: true,
                message: "Task deletion already in progress".to_string(),
                task_id: task.task_id.clone(),
                task: Some(task),
            }));
        }

        let requested_at = chrono::Utc::now().timestamp();
        let mut deleting_task = task_service
            .mark_task_deleting(&req.task_id, &req.reason, requested_at)
            .await
            .map_err(|error| Status::internal(format!("Failed to mark task deleting: {}", error)))?
            .ok_or_else(|| Status::not_found("Task not found"))?;
        if req.force {
            deleting_task
                .metadata
                .insert("delete_force".to_string(), "true".to_string());
            task_service
                .register_task(deleting_task.clone())
                .await
                .map_err(|error| {
                    Status::internal(format!("Failed to persist delete force flag: {}", error))
                })?;
        }
        drop(task_service);

        self.publish_task_events(&deleting_task, TaskEventKind::Cancel)
            .await;
        if let Err(error) = self.reconcile_task_assignments(&deleting_task).await {
            warn!(task_id = %deleting_task.task_id, error = %error, "DeleteTask: reconcile task assignments failed");
        }

        Ok(Response::new(DeleteTaskResponse {
            success: true,
            message: "Task deletion requested".to_string(),
            task_id: deleting_task.task_id.clone(),
            task: Some(deleting_task),
        }))
    }

    /// Complete a pending task deletion / 完成待处理的任务删除
    async fn complete_task_deletion(
        &self,
        request: Request<CompleteTaskDeletionRequest>,
    ) -> Result<Response<CompleteTaskDeletionResponse>, Status> {
        let req = request.into_inner();
        if req.task_id.is_empty() {
            return Err(Status::invalid_argument("task_id is required"));
        }

        let now_ms = chrono::Utc::now().timestamp_millis();
        let (instances, _) = self
            .instance_execution_index
            .list_task_instances(&req.task_id, now_ms, 1, "")
            .await
            .map_err(|error| {
                Status::internal(format!("Failed to inspect task instances: {}", error))
            })?;
        if !instances.is_empty() {
            return Ok(Response::new(CompleteTaskDeletionResponse {
                success: false,
                message: format!(
                    "Task deletion pending: {} active instance(s) still registered",
                    instances.len()
                ),
                task_id: req.task_id,
            }));
        }

        let mut task_service = self.task_service.write().await;
        match task_service.complete_task_deletion(&req.task_id).await {
            Ok(true) => {
                self.remove_task_assignments_and_publish(&req.task_id, now_ms)
                    .await;
                Ok(Response::new(CompleteTaskDeletionResponse {
                    success: true,
                    message: "Task deleted successfully".to_string(),
                    task_id: req.task_id,
                }))
            }
            Ok(false) => Ok(Response::new(CompleteTaskDeletionResponse {
                success: true,
                message: "Task already deleted".to_string(),
                task_id: req.task_id,
            })),
            Err(error) => Ok(Response::new(CompleteTaskDeletionResponse {
                success: false,
                message: format!("Failed to complete task deletion: {}", error),
                task_id: req.task_id,
            })),
        }
    }

    /// Update task status (observed state) / 更新任务状态（观测态）
    async fn update_task_status(
        &self,
        request: Request<UpdateTaskStatusRequest>,
    ) -> Result<Response<UpdateTaskStatusResponse>, Status> {
        let req = request.into_inner();
        debug!(
            task_id = %req.task_id,
            node_uuid = %req.node_uuid,
            status = req.status,
            status_version = req.status_version,
            updated_at = req.updated_at,
            reason = %req.reason,
            "UpdateTaskStatus: request received"
        );
        if req.task_id.is_empty() {
            return Err(Status::invalid_argument("task_id is required"));
        }

        let task_service = self.task_service.read().await;
        let existing = task_service
            .get_task(&req.task_id)
            .await
            .map_err(|error| Status::internal(format!("Failed to get task: {}", error)))?;
        drop(task_service);

        match existing {
            Some(mut task) => {
                let old_status = task.status;
                let preserve_deleting =
                    old_status == crate::proto::sms::TaskStatus::Deleting as i32;
                if !preserve_deleting {
                    task.status = req.status;
                }
                task.last_heartbeat = if req.updated_at > 0 {
                    req.updated_at
                } else {
                    chrono::Utc::now().timestamp()
                };
                debug!(
                    task_id = %task.task_id,
                    old_status,
                    new_status = task.status,
                    last_heartbeat = task.last_heartbeat,
                    "UpdateTaskStatus: applied state change"
                );
                let task = self.persist_task_update(task).await?;
                Ok(Response::new(UpdateTaskStatusResponse {
                    success: true,
                    message: "Task status updated".to_string(),
                    task: Some(task),
                }))
            }
            None => Ok(Response::new(UpdateTaskStatusResponse {
                success: false,
                message: "Task not found".to_string(),
                task: None,
            })),
        }
    }

    /// Update task result fields / 更新任务结果字段
    async fn update_task_result(
        &self,
        request: Request<UpdateTaskResultRequest>,
    ) -> Result<Response<UpdateTaskResultResponse>, Status> {
        let req = request.into_inner();
        if req.task_id.is_empty() {
            return Err(Status::invalid_argument("task_id is required"));
        }

        let task_service = self.task_service.read().await;
        let existing = task_service
            .get_task(&req.task_id)
            .await
            .map_err(|error| Status::internal(format!("Failed to get task: {}", error)))?;
        drop(task_service);

        match existing {
            Some(mut task) => {
                if !req.result_uri.is_empty() {
                    task.last_result_uri = req.result_uri.clone();
                    if !task.result_uris.contains(&req.result_uri) {
                        task.result_uris.push(req.result_uri.clone());
                    }
                }
                task.last_result_status = req.result_status.clone();
                task.last_completed_at = if req.completed_at > 0 {
                    req.completed_at
                } else {
                    chrono::Utc::now().timestamp()
                };
                task.last_result_metadata = req.result_metadata.clone();

                let task = self.persist_task_update(task).await?;
                Ok(Response::new(UpdateTaskResultResponse {
                    success: true,
                    message: "Task result updated".to_string(),
                    task: Some(task),
                }))
            }
            None => Ok(Response::new(UpdateTaskResultResponse {
                success: false,
                message: "Task not found".to_string(),
                task: None,
            })),
        }
    }
}
