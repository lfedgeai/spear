use tonic::{Request, Response, Status};

use crate::proto::sms::{
    ai_backend_control_plane_service_server::AiBackendControlPlaneService as AiBackendControlPlaneServiceTrait,
    AiBackendDesiredState as ProtoAiBackendDesiredState, AiBackendNodeStatus as ProtoAiBackendNodeStatus,
    AiModelView as ProtoAiModelView, AiModelViewInstance as ProtoAiModelViewInstance,
    CreateAiBackendRequest, CreateAiBackendResponse, DeleteAiBackendPlacementRequest,
    DeleteAiBackendPlacementResponse, DeleteAiBackendRequest, DeleteAiBackendResponse,
    GetAiBackendRequest, GetAiBackendResponse, ListAiBackendAssignmentsRequest,
    ListAiBackendAssignmentsResponse, ListAiBackendNodeStatusesRequest,
    ListAiBackendNodeStatusesResponse, ListAiBackendPlacementsRequest,
    ListAiBackendPlacementsResponse, ListAiBackendsRequest, ListAiBackendsResponse,
    ListAiModelViewsRequest, ListAiModelViewsResponse, ReportAiBackendNodeStatusesRequest,
    ReportAiBackendNodeStatusesResponse, ResolvedAiBackendAssignment as ProtoResolvedAiBackendAssignment,
    SetAiBackendDesiredStateRequest, SetAiBackendDesiredStateResponse, UpdateAiBackendRequest,
    UpdateAiBackendResponse, UpsertAiBackendPlacementRequest, UpsertAiBackendPlacementResponse,
};
use crate::sms::ai_backends::model::{
    AiBackendDesiredStateModel, AiBackendNodeRuntimeStatusModel, AiModelView,
    AiModelViewInstance, ResolvedBackendAssignment,
};
use crate::sms::ai_backends::placement_service::{AiBackendPlacementService, UpsertPlacementInput};
use crate::sms::ai_backends::proto_conv::{
    domain_backend_from_proto, domain_desired_state_from_proto, domain_placement_from_proto,
    domain_status_from_proto, proto_backend_from_domain, proto_placement_from_domain,
    proto_status_from_domain,
};
use crate::sms::ai_backends::repository::AiBackendRepository;
use crate::sms::ai_backends::read_model::build_ai_model_views;
use crate::sms::ai_backends::service::{
    AiBackendService, CreateAiBackendInput, UpdateAiBackendInput,
};
use crate::sms::ai_backends::status_service::AiBackendStatusService;
use crate::sms::service::SmsServiceImpl;

impl SmsServiceImpl {
    /// Build the backend service on demand from the shared repository / 基于共享仓储按需构建 backend 服务
    fn ai_backend_service(&self) -> AiBackendService {
        AiBackendService::new(self.ai_backend_repository.clone())
    }

    /// Build the placement service on demand from the shared repository / 基于共享仓储按需构建 placement 服务
    fn ai_backend_placement_service(&self) -> AiBackendPlacementService {
        AiBackendPlacementService::new(self.ai_backend_repository.clone())
    }

    /// Build the status service on demand from the shared repository / 基于共享仓储按需构建状态服务
    fn ai_backend_status_service(&self) -> AiBackendStatusService {
        AiBackendStatusService::new(self.ai_backend_repository.clone())
    }

    /// Convert a proto backend payload into a create input / 把 proto backend 载荷转换为创建输入
    fn create_ai_backend_input_from_proto(
        &self,
        request: &CreateAiBackendRequest,
    ) -> Result<CreateAiBackendInput, Status> {
        let record = domain_backend_from_proto(
            request
                .backend
                .as_ref()
                .ok_or_else(|| Status::invalid_argument("missing backend payload"))?,
        )
        .map_err(Status::from)?;
        Ok(CreateAiBackendInput {
            display_name: record.display_name,
            provider: record.provider,
            model: record.model,
            hosting: record.hosting,
            backend_kind: record.backend_kind,
            management_mode: record.management_mode,
            credential_ref: record.credential_ref,
            spec: record.spec,
            labels: record.labels,
            metadata: record.metadata,
            desired_state: record.desired_state,
        })
    }

    /// Convert a proto backend payload into an update input / 把 proto backend 载荷转换为更新输入
    fn update_ai_backend_input_from_proto(
        &self,
        request: &UpdateAiBackendRequest,
    ) -> Result<UpdateAiBackendInput, Status> {
        let record = domain_backend_from_proto(
            request
                .backend
                .as_ref()
                .ok_or_else(|| Status::invalid_argument("missing backend payload"))?,
        )
        .map_err(Status::from)?;
        Ok(UpdateAiBackendInput {
            backend_id: record.backend_id,
            display_name: record.display_name,
            provider: record.provider,
            model: record.model,
            hosting: record.hosting,
            backend_kind: record.backend_kind,
            management_mode: record.management_mode,
            credential_ref: record.credential_ref,
            spec: record.spec,
            labels: record.labels,
            metadata: record.metadata,
            desired_state: record.desired_state,
        })
    }
}

#[tonic::async_trait]
impl AiBackendControlPlaneServiceTrait for SmsServiceImpl {
    /// List all AI backends / 列出所有 AI backend
    async fn list_ai_backends(
        &self,
        request: Request<ListAiBackendsRequest>,
    ) -> Result<Response<ListAiBackendsResponse>, Status> {
        let req = request.into_inner();
        let service = self.ai_backend_service();
        let backends = service.list_backends().await.map_err(Status::from)?;
        let filtered = filter_ai_backends(&backends, &req);
        let total_count = filtered.len() as i32;
        let paged = paginate_slice(&filtered, req.limit, req.offset);
        Ok(Response::new(ListAiBackendsResponse {
            total_count,
            backends: paged.iter().map(|backend| proto_backend_from_domain(backend)).collect(),
        }))
    }

    /// Get one AI backend by id / 按 id 获取单个 AI backend
    async fn get_ai_backend(
        &self,
        request: Request<GetAiBackendRequest>,
    ) -> Result<Response<GetAiBackendResponse>, Status> {
        let req = request.into_inner();
        let service = self.ai_backend_service();
        let backend = service.get_backend(&req.backend_id).await.map_err(Status::from)?;
        Ok(Response::new(GetAiBackendResponse {
            found: backend.is_some(),
            backend: backend.as_ref().map(proto_backend_from_domain),
        }))
    }

    /// Create one AI backend / 创建单个 AI backend
    async fn create_ai_backend(
        &self,
        request: Request<CreateAiBackendRequest>,
    ) -> Result<Response<CreateAiBackendResponse>, Status> {
        let input = self.create_ai_backend_input_from_proto(&request.into_inner())?;
        let service = self.ai_backend_service();
        let backend = service.create_backend(input).await.map_err(Status::from)?;
        Ok(Response::new(CreateAiBackendResponse {
            backend: Some(proto_backend_from_domain(&backend)),
        }))
    }

    /// Update one AI backend / 更新单个 AI backend
    async fn update_ai_backend(
        &self,
        request: Request<UpdateAiBackendRequest>,
    ) -> Result<Response<UpdateAiBackendResponse>, Status> {
        let input = self.update_ai_backend_input_from_proto(&request.into_inner())?;
        let service = self.ai_backend_service();
        let backend = service.update_backend(input).await.map_err(Status::from)?;
        Ok(Response::new(UpdateAiBackendResponse {
            backend: Some(proto_backend_from_domain(&backend)),
        }))
    }

    /// Delete one AI backend / 删除单个 AI backend
    async fn delete_ai_backend(
        &self,
        request: Request<DeleteAiBackendRequest>,
    ) -> Result<Response<DeleteAiBackendResponse>, Status> {
        let req = request.into_inner();
        let service = self.ai_backend_service();
        let deleted = service.delete_backend(&req.backend_id).await.map_err(Status::from)?;
        Ok(Response::new(DeleteAiBackendResponse { deleted }))
    }

    /// Set one AI backend desired state / 设置单个 AI backend 的期望状态
    async fn set_ai_backend_desired_state(
        &self,
        request: Request<SetAiBackendDesiredStateRequest>,
    ) -> Result<Response<SetAiBackendDesiredStateResponse>, Status> {
        let req = request.into_inner();
        let desired_state = domain_desired_state_from_proto(req.desired_state).map_err(Status::from)?;
        let service = self.ai_backend_service();
        let backend = service
            .set_desired_state(&req.backend_id, desired_state)
            .await
            .map_err(Status::from)?;
        Ok(Response::new(SetAiBackendDesiredStateResponse {
            backend: Some(proto_backend_from_domain(&backend)),
        }))
    }

    /// List backend placements / 列出 backend placement
    async fn list_ai_backend_placements(
        &self,
        request: Request<ListAiBackendPlacementsRequest>,
    ) -> Result<Response<ListAiBackendPlacementsResponse>, Status> {
        let req = request.into_inner();
        let placements = if !req.backend_id.trim().is_empty() {
            self.ai_backend_repository
                .list_placements_by_backend(&req.backend_id)
                .await
                .map_err(Status::from)?
        } else if !req.node_uuid.trim().is_empty() {
            self.ai_backend_repository
                .list_placements_by_node(&req.node_uuid)
                .await
                .map_err(Status::from)?
        } else {
            self.ai_backend_repository
                .list_placements()
                .await
                .map_err(Status::from)?
        };

        Ok(Response::new(ListAiBackendPlacementsResponse {
            total_count: placements.len() as i32,
            placements: placements.iter().map(proto_placement_from_domain).collect(),
        }))
    }

    /// Create or update one backend placement / 创建或更新单个 backend placement
    async fn upsert_ai_backend_placement(
        &self,
        request: Request<UpsertAiBackendPlacementRequest>,
    ) -> Result<Response<UpsertAiBackendPlacementResponse>, Status> {
        let req = request.into_inner();
        let placement = domain_placement_from_proto(
            req.placement
                .as_ref()
                .ok_or_else(|| Status::invalid_argument("missing placement payload"))?,
        )
        .map_err(Status::from)?;
        let service = self.ai_backend_placement_service();
        let placement = service
            .upsert_placement(UpsertPlacementInput {
                placement_id: non_empty_option(placement.placement_id),
                backend_id: placement.backend_id,
                node_uuid: placement.node_uuid,
                desired_state: placement.desired_state,
                weight_override: placement.weight_override,
                priority_override: placement.priority_override,
            })
            .await
            .map_err(Status::from)?;
        Ok(Response::new(UpsertAiBackendPlacementResponse {
            placement: Some(proto_placement_from_domain(&placement)),
        }))
    }

    /// Delete one backend placement / 删除单个 backend placement
    async fn delete_ai_backend_placement(
        &self,
        request: Request<DeleteAiBackendPlacementRequest>,
    ) -> Result<Response<DeleteAiBackendPlacementResponse>, Status> {
        let req = request.into_inner();
        let service = self.ai_backend_placement_service();
        let deleted = service
            .delete_placement(&req.placement_id)
            .await
            .map_err(Status::from)?;
        Ok(Response::new(DeleteAiBackendPlacementResponse { deleted }))
    }

    /// List resolved node assignments / 列出节点解析后的 assignment
    async fn list_ai_backend_assignments(
        &self,
        request: Request<ListAiBackendAssignmentsRequest>,
    ) -> Result<Response<ListAiBackendAssignmentsResponse>, Status> {
        let req = request.into_inner();
        let service = self.ai_backend_placement_service();
        let assignments = service
            .list_node_assignments(&req.node_uuid)
            .await
            .map_err(Status::from)?;
        Ok(Response::new(ListAiBackendAssignmentsResponse {
            total_count: assignments.len() as i32,
            assignments: assignments.iter().map(proto_assignment_from_domain).collect(),
        }))
    }

    /// List backend node statuses / 列出 backend 节点状态
    async fn list_ai_backend_node_statuses(
        &self,
        request: Request<ListAiBackendNodeStatusesRequest>,
    ) -> Result<Response<ListAiBackendNodeStatusesResponse>, Status> {
        let req = request.into_inner();
        let statuses = if !req.backend_id.trim().is_empty() {
            self.ai_backend_repository
                .list_node_statuses_by_backend(&req.backend_id)
                .await
                .map_err(Status::from)?
        } else {
            self.ai_backend_repository
                .list_node_statuses()
                .await
                .map_err(Status::from)?
        };
        Ok(Response::new(ListAiBackendNodeStatusesResponse {
            total_count: statuses.len() as i32,
            statuses: statuses.iter().map(proto_status_from_domain).collect(),
        }))
    }

    /// Report node statuses for one node / 为单个节点上报状态
    async fn report_ai_backend_node_statuses(
        &self,
        request: Request<ReportAiBackendNodeStatusesRequest>,
    ) -> Result<Response<ReportAiBackendNodeStatusesResponse>, Status> {
        let req = request.into_inner();
        let service = self.ai_backend_status_service();
        let statuses = req
            .statuses
            .iter()
            .map(domain_status_from_proto)
            .collect::<Result<Vec<_>, _>>()
            .map_err(Status::from)?;
        service
            .report_statuses(&req.node_uuid, statuses)
            .await
            .map_err(Status::from)?;
        Ok(Response::new(ReportAiBackendNodeStatusesResponse {}))
    }

    /// List aggregated AI model views / 列出聚合后的 AI model 视图
    async fn list_ai_model_views(
        &self,
        request: Request<ListAiModelViewsRequest>,
    ) -> Result<Response<ListAiModelViewsResponse>, Status> {
        let req = request.into_inner();
        let backends = self
            .ai_backend_repository
            .list_backends()
            .await
            .map_err(Status::from)?;
        let placements = self
            .ai_backend_repository
            .list_placements()
            .await
            .map_err(Status::from)?;
        let statuses = self
            .ai_backend_repository
            .list_node_statuses()
            .await
            .map_err(Status::from)?;
        let views = build_ai_model_views(&backends, &placements, &statuses);
        let filtered = filter_ai_model_views(&views, &req);
        let total_count = filtered.len() as i32;
        let paged = paginate_slice(&filtered, req.limit, req.offset);
        Ok(Response::new(ListAiModelViewsResponse {
            total_count,
            views: paged.iter().map(|view| proto_ai_model_view_from_domain(view)).collect(),
        }))
    }
}

/// Convert an optional string into None when empty / 把可选字符串在为空时转换为 None
fn non_empty_option(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn normalize_filter(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn paginate_slice<T>(items: &[T], limit: i32, offset: i32) -> &[T] {
    let safe_offset = offset.max(0) as usize;
    if safe_offset >= items.len() {
        return &items[0..0];
    }
    let safe_limit = if limit <= 0 {
        items.len().saturating_sub(safe_offset)
    } else {
        limit as usize
    };
    let end = (safe_offset + safe_limit).min(items.len());
    &items[safe_offset..end]
}

fn filter_ai_backends<'a>(
    backends: &'a [crate::sms::ai_backends::model::AiBackendRecordModel],
    req: &ListAiBackendsRequest,
) -> Vec<&'a crate::sms::ai_backends::model::AiBackendRecordModel> {
    let q = normalize_filter(&req.q);
    let hosting = normalize_filter(&req.hosting);
    let desired_state = normalize_filter(&req.desired_state);
    let provider = normalize_filter(&req.provider);
    let model = normalize_filter(&req.model);

    backends
        .iter()
        .filter(|backend| {
            if !hosting.is_empty()
                && normalize_filter(match backend.hosting {
                    crate::sms::ai_backends::model::AiBackendHostingModel::Remote => "remote",
                    crate::sms::ai_backends::model::AiBackendHostingModel::Local => "local",
                }) != hosting
            {
                return false;
            }

            if !desired_state.is_empty()
                && normalize_filter(match backend.desired_state {
                    crate::sms::ai_backends::model::AiBackendDesiredStateModel::Enabled => "enabled",
                    crate::sms::ai_backends::model::AiBackendDesiredStateModel::Disabled => "disabled",
                }) != desired_state
            {
                return false;
            }

            if !provider.is_empty() && normalize_filter(&backend.provider) != provider {
                return false;
            }
            if !model.is_empty() && normalize_filter(&backend.model) != model {
                return false;
            }

            if q.is_empty() {
                return true;
            }
            let haystack = format!(
                "{} {} {} {} {}",
                backend.display_name, backend.backend_id, backend.provider, backend.model, backend.backend_kind
            );
            normalize_filter(&haystack).contains(&q)
        })
        .collect()
}

fn filter_ai_model_views<'a>(
    views: &'a [AiModelView],
    req: &ListAiModelViewsRequest,
) -> Vec<&'a AiModelView> {
    let q = normalize_filter(&req.q);
    let hosting = normalize_filter(&req.hosting);
    let status = normalize_filter(&req.status);
    let provider = normalize_filter(&req.provider);
    let model = normalize_filter(&req.model);

    views
        .iter()
        .filter(|view| {
            if !hosting.is_empty()
                && normalize_filter(match view.hosting {
                    crate::sms::ai_backends::model::AiBackendHostingModel::Remote => "remote",
                    crate::sms::ai_backends::model::AiBackendHostingModel::Local => "local",
                }) != hosting
            {
                return false;
            }
            if !provider.is_empty() && normalize_filter(&view.provider) != provider {
                return false;
            }
            if !model.is_empty() && normalize_filter(&view.model) != model {
                return false;
            }
            if !status.is_empty() {
                let computed = if view.ready_nodes > 0 { "available" } else { "unavailable" };
                if computed != status {
                    return false;
                }
            }
            if q.is_empty() {
                return true;
            }
            normalize_filter(&format!("{} {}", view.provider, view.model)).contains(&q)
        })
        .collect()
}

/// Convert one resolved assignment into proto / 把单个解析后的 assignment 转换为 proto
fn proto_assignment_from_domain(
    assignment: &ResolvedBackendAssignment,
) -> ProtoResolvedAiBackendAssignment {
    ProtoResolvedAiBackendAssignment {
        backend: Some(proto_backend_from_domain(&assignment.backend)),
        placement: Some(proto_placement_from_domain(&assignment.placement)),
    }
}

/// Convert one read-model row into proto / 把单个只读模型行转换为 proto
fn proto_ai_model_view_from_domain(view: &AiModelView) -> ProtoAiModelView {
    ProtoAiModelView {
        provider: view.provider.clone(),
        model: view.model.clone(),
        hosting: match view.hosting {
            crate::sms::ai_backends::model::AiBackendHostingModel::Remote => {
                crate::proto::sms::AiBackendHosting::Remote as i32
            }
            crate::sms::ai_backends::model::AiBackendHostingModel::Local => {
                crate::proto::sms::AiBackendHosting::Local as i32
            }
        },
        backend_ids: view.backend_ids.clone(),
        operations: view.operations.clone(),
        features: view.features.clone(),
        transports: view.transports.clone(),
        enabled_nodes: view.enabled_nodes as i32,
        ready_nodes: view.ready_nodes as i32,
        total_nodes: view.total_nodes as i32,
        instances: view.instances.iter().map(proto_ai_model_view_instance_from_domain).collect(),
    }
}

/// Convert one read-model instance into proto / 把单个只读模型实例转换为 proto
fn proto_ai_model_view_instance_from_domain(
    instance: &AiModelViewInstance,
) -> ProtoAiModelViewInstance {
    ProtoAiModelViewInstance {
        backend_id: instance.backend_id.clone(),
        node_uuid: instance.node_uuid.clone(),
        placement_state: proto_desired_state_from_model(instance.placement_state) as i32,
        backend_state: proto_desired_state_from_model(instance.backend_state) as i32,
        runtime_status: instance
            .runtime_status
            .map(|status| proto_runtime_status_from_model(status) as i32),
        runtime_backend_name: instance.runtime_backend_name.clone(),
        endpoint: instance.endpoint.clone(),
        available: instance.available,
    }
}

/// Convert a domain desired-state enum into proto / 把领域期望状态枚举转换为 proto
fn proto_desired_state_from_model(state: AiBackendDesiredStateModel) -> ProtoAiBackendDesiredState {
    match state {
        AiBackendDesiredStateModel::Enabled => ProtoAiBackendDesiredState::Enabled,
        AiBackendDesiredStateModel::Disabled => ProtoAiBackendDesiredState::Disabled,
    }
}

/// Convert a domain runtime-status enum into proto / 把领域运行时状态枚举转换为 proto
fn proto_runtime_status_from_model(
    status: AiBackendNodeRuntimeStatusModel,
) -> ProtoAiBackendNodeStatus {
    match status {
        AiBackendNodeRuntimeStatusModel::Pending => ProtoAiBackendNodeStatus::Pending,
        AiBackendNodeRuntimeStatusModel::Reconciling => ProtoAiBackendNodeStatus::Reconciling,
        AiBackendNodeRuntimeStatusModel::Ready => ProtoAiBackendNodeStatus::Ready,
        AiBackendNodeRuntimeStatusModel::Degraded => ProtoAiBackendNodeStatus::Degraded,
        AiBackendNodeRuntimeStatusModel::Error => ProtoAiBackendNodeStatus::Error,
        AiBackendNodeRuntimeStatusModel::Disabled => ProtoAiBackendNodeStatus::Disabled,
    }
}
