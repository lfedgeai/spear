use std::sync::Arc;
use tonic::{Request, Response, Status};

use crate::proto::spearlet::instance_service_server::InstanceService;
use crate::proto::spearlet::{DestroyInstanceRequest, DestroyInstanceResponse};
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
        let reason = if req.reason.is_empty() {
            None
        } else {
            Some(req.reason)
        };

        self.function_service
            .get_execution_manager()
            .destroy_instance(&instance_id, reason)
            .await
            .map_err(Self::map_destroy_instance_error)?;

        Ok(Response::new(DestroyInstanceResponse {
            success: true,
            message: "destroy requested".to_string(),
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
}
