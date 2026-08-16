use std::sync::Arc;
use tonic::{Request, Response, Status};

use crate::proto::spearlet::instance_service_server::InstanceService;
use crate::proto::spearlet::{
    DestroyInstanceRequest, DestroyInstanceResponse, ReconcileTaskAssignmentsNowRequest,
    ReconcileTaskAssignmentsNowResponse,
};
use crate::spearlet::execution::ExecutionError;
use crate::spearlet::function_service::FunctionServiceImpl;

pub struct InstanceServiceImpl {
    function_service: Arc<FunctionServiceImpl>,
}

impl InstanceServiceImpl {
    pub fn new(function_service: Arc<FunctionServiceImpl>) -> Self {
        Self { function_service }
    }

    fn map_destroy_instance_error(err: ExecutionError) -> Status {
        match err {
            ExecutionError::InstanceNotFound { id } => Status::not_found(format!("Instance not found: {}", id)),
            other => Status::internal(other.to_string()),
        }
    }
}

#[tonic::async_trait]
impl InstanceService for InstanceServiceImpl {
    async fn destroy_instance(
        &self,
        request: Request<DestroyInstanceRequest>,
    ) -> Result<Response<DestroyInstanceResponse>, Status> {
        let req = request.into_inner();
        let instance_id = req.instance_id;
        let execution_manager = self.function_service.get_execution_manager();
        let task_id = execution_manager
            .get_instance(&instance_id)
            .map(|instance| instance.task_id().to_string());
        let reason = if req.reason.is_empty() {
            None
        } else {
            Some(req.reason)
        };

        execution_manager
            .drain_and_destroy_instance(&instance_id, reason)
            .await
            .map_err(Self::map_destroy_instance_error)?;
        if let Some(task_id) = task_id {
            execution_manager
                .ensure_task_meets_desired_replicas(&task_id)
                .await
                .map_err(|e| Status::internal(e.to_string()))?;
        }

        Ok(Response::new(DestroyInstanceResponse {
            success: true,
            message: "destroy requested".to_string(),
        }))
    }

    async fn reconcile_task_assignments_now(
        &self,
        request: Request<ReconcileTaskAssignmentsNowRequest>,
    ) -> Result<Response<ReconcileTaskAssignmentsNowResponse>, Status> {
        let task_id = request.into_inner().task_id;
        let filter = if task_id.trim().is_empty() {
            None
        } else {
            Some(task_id.as_str())
        };
        self.function_service
            .get_execution_manager()
            .reconcile_assignments_from_sms(filter)
            .await
            .map_err(|error| Status::internal(error.to_string()))?;
        Ok(Response::new(ReconcileTaskAssignmentsNowResponse {
            accepted: true,
            message: "reconcile requested".to_string(),
        }))
    }
}

#[tonic::async_trait]
impl InstanceService for Arc<InstanceServiceImpl> {
    async fn destroy_instance(
        &self,
        request: Request<DestroyInstanceRequest>,
    ) -> Result<Response<DestroyInstanceResponse>, Status> {
        (**self).destroy_instance(request).await
    }

    async fn reconcile_task_assignments_now(
        &self,
        request: Request<ReconcileTaskAssignmentsNowRequest>,
    ) -> Result<Response<ReconcileTaskAssignmentsNowResponse>, Status> {
        (**self).reconcile_task_assignments_now(request).await
    }
}
