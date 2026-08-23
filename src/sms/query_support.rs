use crate::proto::sms::{
    execution_index_service_client::ExecutionIndexServiceClient, InstanceSummary,
    ListTaskInstancesRequest,
};
use crate::sms::config::SmsConfig;

pub(crate) const MAX_TASK_INSTANCE_SCAN_PAGES: usize = 10;

pub(crate) fn instance_projection_stale_after_ms(config: &SmsConfig) -> i64 {
    (config.heartbeat_timeout as i64)
        .saturating_mul(2_000)
        .max(1)
}

pub(crate) fn instance_is_active_and_fresh(
    config: &SmsConfig,
    status: i32,
    last_seen_ms: i64,
    now_ms: i64,
) -> bool {
    crate::sms::instance_execution_index::is_instance_active_and_fresh(
        status,
        last_seen_ms,
        now_ms,
        instance_projection_stale_after_ms(config),
    )
}

// Several admin/runtime paths need a bounded scan across the task instance pages.
// Keep the pagination guard and token handling in one place so callers focus on
// filtering/aggregation rather than reimplementing the scan loop.
pub(crate) async fn collect_task_instances_bounded(
    client: &mut ExecutionIndexServiceClient<tonic::transport::Channel>,
    task_id: &str,
    page_size: i32,
) -> Result<Vec<InstanceSummary>, tonic::Status> {
    let mut page_token = String::new();
    let mut out = Vec::new();

    for _ in 0..MAX_TASK_INSTANCE_SCAN_PAGES {
        let resp = client
            .list_task_instances(tonic::Request::new(ListTaskInstancesRequest {
                task_id: task_id.to_string(),
                limit: page_size,
                page_token: page_token.clone(),
            }))
            .await?
            .into_inner();

        out.extend(resp.instances);
        if resp.next_page_token.is_empty() {
            break;
        }
        page_token = resp.next_page_token;
    }

    Ok(out)
}
