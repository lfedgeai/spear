use tonic::{Request, Response, Status};

use crate::proto::spearlet::instance_service_server::InstanceService;
use crate::proto::spearlet::{
    DestroyInstanceRequest, DestroyInstanceResponse, ReconcileTaskAssignmentsNowRequest,
    ReconcileTaskAssignmentsNowResponse,
};
use crate::spearlet::execution::ExecutionError;
use crate::spearlet::execution::TaskExecutionManager;
use crate::spearlet::function_service::FunctionServiceImpl;
use std::sync::Arc;

#[derive(Clone)]
pub struct InstanceServiceImpl {
    function_service: FunctionServiceImpl,
}

impl InstanceServiceImpl {
    pub fn new(function_service: FunctionServiceImpl) -> Self {
        Self { function_service }
    }

    fn execution_manager(&self) -> Arc<TaskExecutionManager> {
        self.function_service.get_execution_manager()
    }

    fn optional_non_empty(value: String) -> Option<String> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    }

    fn task_filter(task_id: String) -> Option<String> {
        Self::optional_non_empty(task_id)
    }

    fn map_destroy_instance_error(err: ExecutionError) -> Status {
        match err {
            ExecutionError::InstanceNotFound { id } => {
                Status::not_found(format!("Instance not found: {}", id))
            }
            other => Status::internal(other.to_string()),
        }
    }

    fn map_internal_error<E: std::fmt::Display>(error: E) -> Status {
        Status::internal(error.to_string())
    }

    async fn reconcile_task_after_destroy(
        &self,
        execution_manager: &Arc<TaskExecutionManager>,
        task_id: Option<String>,
    ) -> Result<(), Status> {
        let Some(task_id) = task_id else {
            return Ok(());
        };
        execution_manager
            .ensure_task_meets_desired_replicas(&task_id)
            .await
            .map_err(Self::map_internal_error)
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
        let execution_manager = self.execution_manager();
        let task_id = execution_manager
            .get_instance(&instance_id)
            .map(|instance| instance.task_id().to_string());
        let reason = Self::optional_non_empty(req.reason);

        execution_manager
            .drain_and_destroy_instance(&instance_id, reason)
            .await
            .map_err(Self::map_destroy_instance_error)?;
        self.reconcile_task_after_destroy(&execution_manager, task_id)
            .await?;

        Ok(Response::new(DestroyInstanceResponse {
            success: true,
            message: "destroy requested".to_string(),
        }))
    }

    async fn reconcile_task_assignments_now(
        &self,
        request: Request<ReconcileTaskAssignmentsNowRequest>,
    ) -> Result<Response<ReconcileTaskAssignmentsNowResponse>, Status> {
        let filter = Self::task_filter(request.into_inner().task_id);
        self.execution_manager()
            .reconcile_assignments_from_sms(filter.as_deref())
            .await
            .map_err(Self::map_internal_error)?;
        Ok(Response::new(ReconcileTaskAssignmentsNowResponse {
            accepted: true,
            message: "reconcile requested".to_string(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::InstanceServiceImpl;

    #[test]
    fn optional_non_empty_trims_blank_values() {
        assert_eq!(
            InstanceServiceImpl::optional_non_empty("".to_string()),
            None
        );
        assert_eq!(
            InstanceServiceImpl::optional_non_empty("   ".to_string()),
            None
        );
        assert_eq!(
            InstanceServiceImpl::optional_non_empty("  manual  ".to_string()),
            Some("manual".to_string())
        );
    }

    #[test]
    fn task_filter_reuses_optional_string_rules() {
        assert_eq!(InstanceServiceImpl::task_filter("".to_string()), None);
        assert_eq!(
            InstanceServiceImpl::task_filter(" task-a ".to_string()),
            Some("task-a".to_string())
        );
    }
}
