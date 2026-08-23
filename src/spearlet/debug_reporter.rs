#[cfg(feature = "debug-reporter")]
use serde_json::Value;

#[cfg(feature = "debug-reporter")]
mod imp {
    use super::Value;
    use std::sync::OnceLock;

    const LOG_HYPOTHESIS_ID: &str = "LOG";

    #[derive(Clone, Debug)]
    struct DebugReporterConfig {
        url: String,
        session_id: String,
        run_id: String,
        client_name: String,
        stream_name: String,
    }

    static CFG: OnceLock<Option<DebugReporterConfig>> = OnceLock::new();

    fn cfg() -> &'static Option<DebugReporterConfig> {
        CFG.get_or_init(|| {
            let url = std::env::var("DEBUG_SERVER_URL").unwrap_or_default();
            if url.trim().is_empty() {
                return None;
            }
            let session_id = read_debug_name("DEBUG_SESSION_ID", "default-session");
            let run_id = std::env::var("DEBUG_RUN_ID").unwrap_or_else(|_| "runtime".to_string());
            let client_name = read_debug_name("DEBUG_CLIENT_NAME", "default-client");
            let stream_name = read_debug_name("DEBUG_STREAM_NAME", "default-stream");
            Some(DebugReporterConfig {
                url,
                session_id,
                run_id,
                client_name,
                stream_name,
            })
        })
    }

    fn read_debug_name(env_name: &str, default: &str) -> String {
        std::env::var(env_name)
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| default.to_string())
    }

    fn build_event_payload(
        cfg: &DebugReporterConfig,
        hypothesis_id: &'static str,
        location: &'static str,
        msg: String,
        data: Value,
        stream_name: Option<&str>,
    ) -> Value {
        serde_json::json!({
            "sessionId": cfg.session_id,
            "runId": cfg.run_id,
            "clientName": cfg.client_name,
            "streamName": stream_name.unwrap_or(&cfg.stream_name),
            "hypothesisId": hypothesis_id,
            "location": location,
            "msg": msg,
            "data": data,
            "ts": chrono::Utc::now().timestamp_millis(),
        })
    }

    fn report_event_with_stream(
        hypothesis_id: &'static str,
        location: &'static str,
        msg: String,
        data: Value,
        stream_name: Option<&str>,
    ) {
        let Some(cfg) = cfg().as_ref() else {
            return;
        };
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            return;
        };

        let url = cfg.url.clone();
        let payload = build_event_payload(cfg, hypothesis_id, location, msg, data, stream_name);

        handle.spawn(async move {
            let _ = reqwest::Client::new().post(url).json(&payload).send().await;
        });
    }

    pub fn report_termination(
        location: &'static str,
        snapshot: &crate::spearlet::execution::host_api::termination::TerminationSnapshot,
        msg: String,
    ) {
        report_event_with_stream(
            "A",
            location,
            msg,
            serde_json::json!({
                "scope": format!("{:?}", snapshot.scope),
                "message": snapshot.message.clone().unwrap_or_default(),
            }),
            None,
        );
    }

    /// Mirror runtime logs to the debug server while preserving instance/execution context.
    /// 在保留 instance/execution 上下文的前提下，把运行时日志镜像到 debug server。
    pub fn report_log(
        location: &'static str,
        source: &'static str,
        level: &str,
        message: &str,
        task_id: Option<&str>,
        execution_id: Option<&str>,
        instance_id: Option<&str>,
    ) {
        let stream_name = match source {
            "wasm" => "wasm-log",
            _ => "runtime-log",
        };
        report_event_with_stream(
            LOG_HYPOTHESIS_ID,
            location,
            message.to_string(),
            serde_json::json!({
                "eventType": "log",
                "source": source,
                "level": level,
                "taskId": task_id,
                "executionId": execution_id,
                "instanceId": instance_id,
            }),
            Some(stream_name),
        );
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn build_event_payload_uses_override_stream_for_logs() {
            let cfg = DebugReporterConfig {
                url: "http://127.0.0.1:7777/event".to_string(),
                session_id: "default-session".to_string(),
                run_id: "runtime".to_string(),
                client_name: "default-client".to_string(),
                stream_name: "default-stream".to_string(),
            };

            let payload = build_event_payload(
                &cfg,
                LOG_HYPOTHESIS_ID,
                "wasm_log_write",
                "hello".to_string(),
                serde_json::json!({
                    "eventType": "log",
                    "source": "wasm",
                    "level": "info",
                    "executionId": "exec-1",
                    "instanceId": "inst-1",
                }),
                Some("wasm-log"),
            );

            assert_eq!(payload["clientName"], "default-client");
            assert_eq!(payload["streamName"], "wasm-log");
            assert_eq!(payload["data"]["eventType"], "log");
            assert_eq!(payload["data"]["executionId"], "exec-1");
            assert_eq!(payload["data"]["instanceId"], "inst-1");
        }
    }
}

#[cfg(not(feature = "debug-reporter"))]
mod imp {
    pub fn report_termination(
        _: &'static str,
        _: &crate::spearlet::execution::host_api::termination::TerminationSnapshot,
        _: String,
    ) {
    }

    pub fn report_log(
        _: &'static str,
        _: &'static str,
        _: &str,
        _: &str,
        _: Option<&str>,
        _: Option<&str>,
        _: Option<&str>,
    ) {
    }
}

#[allow(unused_imports)]
pub use imp::{report_log, report_termination};
