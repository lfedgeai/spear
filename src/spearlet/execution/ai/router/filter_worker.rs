use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;
use tonic::transport::{Channel, Endpoint};

use crate::proto::spearlet::{
    router_filter_service_client::RouterFilterServiceClient, FilterRequest,
};
use crate::spearlet::config::RouterGrpcFilterStreamConfig;
use crate::spearlet::execution::ai::ir::CanonicalError;
use crate::spearlet::execution::ai::router::filter_inflight::InflightRegistry;

#[derive(Debug)]
pub struct FilterJob {
    pub req: FilterRequest,
}

/// Start the async router-filter worker on the current runtime or a fallback thread.
/// 在当前 runtime 或回退线程上启动异步 router-filter worker。
pub fn start_background_worker(
    config: RouterGrpcFilterStreamConfig,
    inflight: Arc<InflightRegistry>,
    rx: mpsc::UnboundedReceiver<FilterJob>,
) {
    let worker = FilterWorker::new(config, inflight);
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        handle.spawn(async move {
            worker.run(rx).await;
        });
    } else {
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("build router filter background runtime");
            rt.block_on(async move {
                worker.run(rx).await;
            });
        });
    }
}

struct FilterWorker {
    config: RouterGrpcFilterStreamConfig,
    inflight: Arc<InflightRegistry>,
}

impl FilterWorker {
    fn new(config: RouterGrpcFilterStreamConfig, inflight: Arc<InflightRegistry>) -> Self {
        Self { config, inflight }
    }

    async fn run(self, mut rx: mpsc::UnboundedReceiver<FilterJob>) {
        let mut client: Option<RouterFilterServiceClient<Channel>> = None;
        while let Some(job) = rx.recv().await {
            if !self.config.enabled {
                self.inflight.finish_with_error(
                    job.req.correlation_id.as_str(),
                    CanonicalError {
                        code: "router_filter_unavailable".to_string(),
                        message: "router filter disabled".to_string(),
                        retryable: true,
                        operation: None,
                    },
                );
                continue;
            }

            if client.is_none() {
                let connect_timeout = Duration::from_millis(job.req.decision_timeout_ms.max(1) as u64)
                    .min(Duration::from_millis(200));
                client = self.build_client(connect_timeout).await.ok();
            }
            let Some(mut c) = client.clone() else {
                self.inflight.finish_with_error(
                    job.req.correlation_id.as_str(),
                    CanonicalError {
                        code: "router_filter_unavailable".to_string(),
                        message: "router filter client unavailable".to_string(),
                        retryable: true,
                        operation: None,
                    },
                );
                continue;
            };

            let timeout = Duration::from_millis(job.req.decision_timeout_ms.max(1) as u64);
            let cid = job.req.correlation_id.clone();
            let result = tokio::time::timeout(timeout, c.filter(job.req)).await;
            match result {
                Err(_) => {
                    self.inflight.finish_with_error(
                        cid.as_str(),
                        CanonicalError {
                            code: "router_filter_timeout".to_string(),
                            message: "router filter decision timed out".to_string(),
                            retryable: true,
                            operation: None,
                        },
                    );
                }
                Ok(Err(e)) => {
                    client = None;
                    self.inflight.finish_with_error(
                        cid.as_str(),
                        CanonicalError {
                            code: "router_filter_unavailable".to_string(),
                            message: format!("router filter rpc error: {}", e.message()),
                            retryable: true,
                            operation: None,
                        },
                    );
                }
                Ok(Ok(resp)) => {
                    self.inflight.finish_with_response(resp.into_inner());
                }
            }
        }
    }

    async fn build_client(
        &self,
        connect_timeout: Duration,
    ) -> Result<RouterFilterServiceClient<Channel>, CanonicalError> {
        let addr = self.config.addr.trim();
        if addr.is_empty() {
            return Err(CanonicalError {
                code: "router_filter_unavailable".to_string(),
                message: "router filter addr is empty".to_string(),
                retryable: true,
                operation: None,
            });
        }
        let ep = Endpoint::from_shared(format!("http://{}", addr))
            .map_err(|e| CanonicalError {
                code: "router_filter_unavailable".to_string(),
                message: format!("invalid router filter addr: {}", e),
                retryable: true,
                operation: None,
            })?
            .tcp_nodelay(true)
            .connect_timeout(connect_timeout);

        let channel = ep.connect().await.map_err(|e| CanonicalError {
            code: "router_filter_unavailable".to_string(),
            message: format!("router filter connect error: {}", e),
            retryable: true,
            operation: None,
        })?;
        Ok(RouterFilterServiceClient::new(channel))
    }
}
