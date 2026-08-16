#[cfg(feature = "debug-reporter")]
use serde_json::Value;

#[cfg(feature = "debug-reporter")]
mod imp {
    use super::Value;
    use std::sync::OnceLock;

    #[derive(Clone)]
    struct DebugReporterConfig {
        url: String,
        session_id: String,
        run_id: String,
    }

    static CFG: OnceLock<Option<DebugReporterConfig>> = OnceLock::new();

    fn cfg() -> &'static Option<DebugReporterConfig> {
        CFG.get_or_init(|| {
            let url = std::env::var("DEBUG_SERVER_URL").unwrap_or_default();
            if url.trim().is_empty() {
                return None;
            }
            let session_id = std::env::var("DEBUG_SESSION_ID")
                .unwrap_or_else(|_| "debug-session".to_string());
            let run_id = std::env::var("DEBUG_RUN_ID").unwrap_or_else(|_| "runtime".to_string());
            Some(DebugReporterConfig {
                url,
                session_id,
                run_id,
            })
        })
    }

    fn report_event(
        hypothesis_id: &'static str,
        location: &'static str,
        msg: String,
        data: Value,
    ) {
        let Some(cfg) = cfg().as_ref() else {
            return;
        };
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            return;
        };

        let url = cfg.url.clone();
        let session_id = cfg.session_id.clone();
        let run_id = cfg.run_id.clone();

        handle.spawn(async move {
            let _ = reqwest::Client::new()
                .post(url)
                .json(&serde_json::json!({
                    "sessionId": session_id,
                    "runId": run_id,
                    "hypothesisId": hypothesis_id,
                    "location": location,
                    "msg": msg,
                    "data": data,
                    "ts": chrono::Utc::now().timestamp_millis(),
                }))
                .send()
                .await;
        });
    }

    pub fn report_termination(
        location: &'static str,
        snapshot: &crate::spearlet::execution::host_api::termination::TerminationSnapshot,
        msg: String,
    ) {
        report_event(
            "A",
            location,
            msg,
            serde_json::json!({
                "scope": format!("{:?}", snapshot.scope),
                "message": snapshot.message.clone().unwrap_or_default(),
            }),
        );
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

}

pub use imp::report_termination;
