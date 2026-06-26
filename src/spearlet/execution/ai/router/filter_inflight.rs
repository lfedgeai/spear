use std::collections::HashMap;
use std::time::Duration;

use crate::proto::spearlet::FilterResponse;
use crate::spearlet::execution::ai::ir::{CanonicalError, Operation};

type FilterResult = Result<FilterResponse, CanonicalError>;

/// Process-local registry for inflight router-filter waiters.
/// 进程内 router-filter inflight waiter 注册表。
pub struct InflightRegistry {
    waiters: parking_lot::Mutex<HashMap<String, std::sync::mpsc::Sender<FilterResult>>>,
}

/// Blocking wait handle for one router-filter correlation id.
/// 单个 router-filter correlation id 的阻塞等待句柄。
pub struct WaitHandle {
    correlation_id: String,
    rx: std::sync::mpsc::Receiver<FilterResult>,
}

impl InflightRegistry {
    pub fn new() -> Self {
        Self {
            waiters: parking_lot::Mutex::new(HashMap::new()),
        }
    }

    pub fn register(&self, correlation_id: String) -> WaitHandle {
        let (tx, rx) = std::sync::mpsc::channel::<FilterResult>();
        self.waiters.lock().insert(correlation_id.clone(), tx);
        WaitHandle { correlation_id, rx }
    }

    pub fn finish_with_response(&self, resp: FilterResponse) {
        let cid = resp.correlation_id.clone();
        let tx = self.waiters.lock().remove(&cid);
        if let Some(tx) = tx {
            let _ = tx.send(Ok(resp));
        }
    }

    pub fn finish_with_error(&self, correlation_id: &str, err: CanonicalError) {
        let tx = self.waiters.lock().remove(correlation_id);
        if let Some(tx) = tx {
            let _ = tx.send(Err(err));
        }
    }

    pub fn cancel(&self, correlation_id: &str) {
        self.waiters.lock().remove(correlation_id);
    }
}

impl WaitHandle {
    pub fn correlation_id(&self) -> &str {
        &self.correlation_id
    }

    pub fn wait(
        self,
        timeout: Duration,
        operation: &Operation,
    ) -> Result<FilterResponse, CanonicalError> {
        self.rx.recv_timeout(timeout).map_err(|_| CanonicalError {
            code: "router_filter_timeout".to_string(),
            message: "router filter decision timed out".to_string(),
            retryable: true,
            operation: Some(operation.clone()),
        })?
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wait_handle_receives_completed_response() {
        let registry = InflightRegistry::new();
        let handle = registry.register("cid-1".to_string());
        registry.finish_with_response(FilterResponse {
            correlation_id: "cid-1".to_string(),
            decision_id: "d1".to_string(),
            decisions: vec![],
            final_action: None,
            debug: HashMap::new(),
        });
        let resp = handle
            .wait(Duration::from_millis(10), &Operation::ChatCompletions)
            .expect("response should be delivered");
        assert_eq!(resp.correlation_id, "cid-1");
    }
}
