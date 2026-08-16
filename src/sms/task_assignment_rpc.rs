use std::collections::{HashMap, HashSet};

use tonic::{Request, Response, Status};
use tracing::warn;

use crate::proto::sms::{
    task_placement_assignment_service_server::TaskPlacementAssignmentService as TaskPlacementAssignmentServiceTrait,
    EventOp, ListNodeTaskAssignmentsRequest, ListNodeTaskAssignmentsResponse,
    ListTaskAssignmentsRequest, ListTaskAssignmentsResponse, Task, TaskPlacementAssignment,
};
use crate::proto::spearlet::{
    instance_service_client::InstanceServiceClient, ReconcileTaskAssignmentsNowRequest,
};
use crate::sms::service::SmsServiceImpl;
use crate::sms::services::task_assignment_service::AssignmentReplaceResult;

impl SmsServiceImpl {
    pub(crate) async fn publish_assignment_changes(
        &self,
        upserts: &[TaskPlacementAssignment],
        deletes: &[TaskPlacementAssignment],
    ) {
        for assignment in upserts {
            if let Err(e) = self
                .unified_events
                .publish_task_assignment_event(assignment, EventOp::Upsert)
                .await
            {
                warn!(
                    error = %e,
                    task_id = %assignment.task_id,
                    node_uuid = %assignment.node_uuid,
                    "Publish task assignment upsert event failed"
                );
            }
        }
        for assignment in deletes {
            if let Err(e) = self
                .unified_events
                .publish_task_assignment_event(assignment, EventOp::Delete)
                .await
            {
                warn!(
                    error = %e,
                    task_id = %assignment.task_id,
                    node_uuid = %assignment.node_uuid,
                    "Publish task assignment delete event failed"
                );
            }
        }
    }

    fn affected_assignment_nodes(result: &AssignmentReplaceResult) -> HashSet<String> {
        result
            .upserts
            .iter()
            .chain(result.deletes.iter())
            .map(|assignment| assignment.node_uuid.clone())
            .collect()
    }

    async fn apply_assignment_change_result(
        &self,
        task_id: &str,
        result: AssignmentReplaceResult,
        trigger_fast_path: bool,
    ) {
        self.publish_assignment_changes(&result.upserts, &result.deletes)
            .await;
        if trigger_fast_path {
            self.trigger_assignment_fast_path(task_id, Self::affected_assignment_nodes(&result))
                .await;
        }
    }

    pub(crate) async fn replace_task_assignments_and_publish(
        &self,
        task_id: &str,
        desired_by_node: HashMap<String, u32>,
        updated_at_ms: i64,
        trigger_fast_path: bool,
    ) {
        let result = self
            .task_assignment_service
            .replace_task_assignments(task_id, desired_by_node, updated_at_ms)
            .await;
        self.apply_assignment_change_result(task_id, result, trigger_fast_path)
            .await;
    }

    pub(crate) async fn remove_task_assignments_and_publish(
        &self,
        task_id: &str,
        updated_at_ms: i64,
    ) {
        let result = self
            .task_assignment_service
            .remove_task_assignments(task_id, updated_at_ms)
            .await;
        self.apply_assignment_change_result(task_id, result, false).await;
    }

    async fn trigger_assignment_fast_path(&self, task_id: &str, nodes: HashSet<String>) {
        for node_uuid in nodes {
            let node_service = self.node_service.read().await.clone();
            let Some(node) = node_service.get_node(&node_uuid).await.ok().flatten() else {
                continue;
            };
            let task_id = task_id.to_string();
            tokio::spawn(async move {
                let url = format!("http://{}:{}", node.ip_address, node.port);
                let channel = match tonic::transport::Channel::from_shared(url) {
                    Ok(endpoint) => endpoint.connect_lazy(),
                    Err(error) => {
                        warn!(error = %error, node_uuid = %node.uuid, "Invalid spearlet url for task assignment fast path");
                        return;
                    }
                };
                let mut client = InstanceServiceClient::new(channel);
                if let Err(error) = client
                    .reconcile_task_assignments_now(ReconcileTaskAssignmentsNowRequest { task_id })
                    .await
                {
                    warn!(error = %error, node_uuid = %node.uuid, "Task assignment fast path reconcile RPC failed");
                }
            });
        }
    }

    pub async fn reconcile_task_assignments(&self, task: &Task) -> Result<(), String> {
        let desired_by_node = self.compute_task_assignments(task).await?;
        let updated_at_ms = chrono::Utc::now().timestamp_millis();
        self.replace_task_assignments_and_publish(
            &task.task_id,
            desired_by_node,
            updated_at_ms,
            true,
        )
        .await;
        Ok(())
    }

    pub async fn reconcile_all_task_assignments(&self) -> Result<(), String> {
        let task_service = self.task_service.read().await.clone();
        let tasks = task_service.list_tasks().await.map_err(|e| e.to_string())?;
        for task in tasks {
            self.reconcile_task_assignments(&task).await?;
        }
        Ok(())
    }
}

#[tonic::async_trait]
impl TaskPlacementAssignmentServiceTrait for SmsServiceImpl {
    async fn list_node_task_assignments(
        &self,
        request: Request<ListNodeTaskAssignmentsRequest>,
    ) -> Result<Response<ListNodeTaskAssignmentsResponse>, Status> {
        let node_uuid = request.into_inner().node_uuid;
        if node_uuid.trim().is_empty() {
            return Err(Status::invalid_argument("node_uuid is required"));
        }
        let assignments = self.task_assignment_service.list_node_assignments(&node_uuid).await;
        Ok(Response::new(ListNodeTaskAssignmentsResponse { assignments }))
    }

    async fn list_task_assignments(
        &self,
        request: Request<ListTaskAssignmentsRequest>,
    ) -> Result<Response<ListTaskAssignmentsResponse>, Status> {
        let task_id = request.into_inner().task_id;
        if task_id.trim().is_empty() {
            return Err(Status::invalid_argument("task_id is required"));
        }
        let assignments = self.task_assignment_service.list_task_assignments(&task_id).await;
        Ok(Response::new(ListTaskAssignmentsResponse { assignments }))
    }
}
