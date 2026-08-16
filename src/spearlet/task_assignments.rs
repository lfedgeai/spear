use std::sync::Arc;
use std::time::Duration;

use tonic::transport::Channel;
use tracing::warn;

use crate::proto::sms::{
    events_service_client::EventsServiceClient, subscribe_events_selector, ResourceType,
    SubscribeEventsRequest, SubscribeEventsSelector,
};
use crate::spearlet::config::SpearletConfig;
use crate::spearlet::execution::TaskExecutionManager;

#[derive(Clone)]
pub struct TaskAssignmentSubscriber {
    config: Arc<SpearletConfig>,
    sms_channel: Option<Channel>,
    manager: Arc<TaskExecutionManager>,
}

impl TaskAssignmentSubscriber {
    pub fn new(
        config: Arc<SpearletConfig>,
        sms_channel: Option<Channel>,
        manager: Arc<TaskExecutionManager>,
    ) -> Self {
        Self {
            config,
            sms_channel,
            manager,
        }
    }

    pub async fn start(self) {
        tokio::spawn(async move {
            let mut backoff_secs = 1u64;
            loop {
                if let Err(error) = self.manager.reconcile_assignments_from_sms(None).await {
                    warn!(error = %error, "Task assignment full reconcile failed");
                }
                match self.run_subscription_once().await {
                    Ok(()) => backoff_secs = 1,
                    Err(error) => {
                        warn!(error = %error, "Task assignment subscriber loop failed");
                        tokio::time::sleep(Duration::from_secs(backoff_secs)).await;
                        backoff_secs = (backoff_secs * 2).min(30);
                    }
                }
            }
        });
    }

    async fn run_subscription_once(&self) -> Result<(), String> {
        let channel = self
            .sms_channel
            .clone()
            .ok_or_else(|| "sms_grpc_addr is empty".to_string())?;
        let mut client = EventsServiceClient::new(channel);
        let node_uuid = self.config.compute_node_uuid();
        let mut stream = client
            .subscribe_events(SubscribeEventsRequest {
                selector: Some(SubscribeEventsSelector {
                    selector: Some(subscribe_events_selector::Selector::NodeUuid(node_uuid)),
                }),
                after_seq: 0,
                replay_limit: 256,
            })
            .await
            .map_err(|error| format!("subscribe_events failed: {}", error))?
            .into_inner();
        let mut ticker = tokio::time::interval(Duration::from_secs(15));
        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    self.manager
                        .reconcile_assignments_from_sms(None)
                        .await
                        .map_err(|error| error.to_string())?;
                }
                maybe_env = stream.message() => {
                    match maybe_env {
                        Ok(Some(env)) => {
                            if env.resource_type != ResourceType::TaskAssignment as i32 {
                                continue;
                            }
                            self.manager
                                .reconcile_assignments_from_sms(None)
                                .await
                                .map_err(|error| error.to_string())?;
                        }
                        Ok(None) => return Err("assignment event stream closed".to_string()),
                        Err(error) => return Err(format!("assignment event stream error: {}", error)),
                    }
                }
            }
        }
    }
}
