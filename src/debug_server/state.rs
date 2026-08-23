use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Context;
use parking_lot::Mutex;
use serde::Serialize;
use serde_json::{Map, Value};

use super::config::{DebugServerConfig, DEFAULT_CLIENT, DEFAULT_STREAM};

#[derive(Clone)]
pub struct DebugServerState {
    config: DebugServerConfig,
    last_activity: Arc<Mutex<Instant>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClientStreams {
    pub name: String,
    pub streams: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Selection {
    pub session: String,
    pub client: String,
    pub stream: String,
}

impl DebugServerState {
    pub fn new(config: DebugServerConfig) -> anyhow::Result<Self> {
        fs::create_dir_all(&config.outdir)
            .with_context(|| format!("failed to create outdir {}", config.outdir))?;
        Ok(Self {
            config,
            last_activity: Arc::new(Mutex::new(Instant::now())),
        })
    }

    pub fn default_session(&self) -> &str {
        &self.config.session
    }

    pub fn touch(&self) {
        *self.last_activity.lock() = Instant::now();
    }

    pub fn idle_for(&self) -> Duration {
        Instant::now().saturating_duration_since(*self.last_activity.lock())
    }

    pub fn write_env(&self, host: &str) -> anyhow::Result<()> {
        let env_path = self
            .outdir()
            .join(format!("{}.env", self.default_session()));
        let content = format!(
            "DEBUG_SERVER_URL=http://{}:{}/event\nDEBUG_SESSION_ID={}\nDEBUG_CLIENT_NAME={}\nDEBUG_STREAM_NAME={}\n",
            host,
            self.config.port,
            self.default_session(),
            DEFAULT_CLIENT,
            DEFAULT_STREAM
        );
        fs::write(&env_path, content)
            .with_context(|| format!("failed to write {}", env_path.display()))
    }

    pub fn list_sessions(&self) -> anyhow::Result<Vec<String>> {
        let prefix = "trae-debug-log-";
        let mut sessions = BTreeSet::from([self.default_session().to_string()]);
        for entry in fs::read_dir(self.outdir())? {
            let entry = entry?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if !name.starts_with(prefix) || !name.ends_with(".ndjson") {
                continue;
            }
            let suffix = &name[prefix.len()..name.len() - ".ndjson".len()];
            let session = suffix
                .split("--")
                .next()
                .unwrap_or(self.default_session())
                .trim();
            sessions.insert(if session.is_empty() {
                self.default_session().to_string()
            } else {
                session.to_string()
            });
        }
        Ok(sessions.into_iter().collect())
    }

    pub fn client_streams(&self, session: &str) -> anyhow::Result<BTreeMap<String, Vec<String>>> {
        let prefix = format!("trae-debug-log-{}", session);
        let mut mapping = BTreeMap::<String, BTreeSet<String>>::new();
        mapping
            .entry(DEFAULT_CLIENT.to_string())
            .or_default()
            .insert(DEFAULT_STREAM.to_string());

        for entry in fs::read_dir(self.outdir())? {
            let entry = entry?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name == format!("{}.ndjson", prefix) {
                mapping
                    .entry(DEFAULT_CLIENT.to_string())
                    .or_default()
                    .insert(DEFAULT_STREAM.to_string());
                continue;
            }
            let marker = format!("{}--", prefix);
            if !name.starts_with(&marker) || !name.ends_with(".ndjson") {
                continue;
            }
            let suffix = &name[marker.len()..name.len() - ".ndjson".len()];
            let Some((client, stream)) = suffix.split_once("--") else {
                continue;
            };
            mapping
                .entry(non_empty(client, DEFAULT_CLIENT).to_string())
                .or_default()
                .insert(non_empty(stream, DEFAULT_STREAM).to_string());
        }

        Ok(mapping
            .into_iter()
            .map(|(client, streams)| (client, streams.into_iter().collect()))
            .collect())
    }

    pub fn resolve_selection(
        &self,
        session: Option<String>,
        client: Option<String>,
        stream: Option<String>,
    ) -> anyhow::Result<Selection> {
        let sessions = self.list_sessions()?;
        let session = normalize(session, self.default_session());
        let session = if sessions.contains(&session) {
            session
        } else {
            self.default_session().to_string()
        };

        let hierarchy = self.client_streams(&session)?;
        let client = normalize(client, DEFAULT_CLIENT);
        let client = if hierarchy.contains_key(&client) {
            client
        } else {
            DEFAULT_CLIENT.to_string()
        };

        let streams = hierarchy
            .get(&client)
            .cloned()
            .unwrap_or_else(|| vec![DEFAULT_STREAM.to_string()]);
        let stream = normalize(stream, DEFAULT_STREAM);
        let stream = if streams.contains(&stream) {
            stream
        } else {
            DEFAULT_STREAM.to_string()
        };

        Ok(Selection {
            session,
            client,
            stream,
        })
    }

    pub fn list_clients(&self, session: &str) -> anyhow::Result<Vec<String>> {
        Ok(self.client_streams(session)?.into_keys().collect())
    }

    pub fn client_stream_entries(&self, session: &str) -> anyhow::Result<Vec<ClientStreams>> {
        Ok(self
            .client_streams(session)?
            .into_iter()
            .map(|(name, streams)| ClientStreams { name, streams })
            .collect())
    }

    pub fn read_recent_lines(
        &self,
        session: &str,
        client: &str,
        stream: &str,
        limit: usize,
    ) -> anyhow::Result<Vec<String>> {
        let path = self.log_path(session, client, stream);
        match fs::read(&path) {
            Ok(bytes) => Ok(bytes
                .split(|byte| *byte == b'\n')
                .filter(|line| !line.is_empty())
                .rev()
                .take(limit)
                .map(|line| String::from_utf8_lossy(line).to_string())
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(err) => Err(err).with_context(|| format!("failed to read {}", path.display())),
        }
    }

    pub fn clear_logs(&self, session: &str, client: &str, stream: &str) -> anyhow::Result<()> {
        let path = self.log_path(session, client, stream);
        fs::write(&path, b"").with_context(|| format!("failed to clear {}", path.display()))
    }

    pub fn append_event(&self, mut event: Value) -> anyhow::Result<Selection> {
        let object = event
            .as_object_mut()
            .context("invalid json payload: expected object")?;

        let session = normalize(
            object
                .get("sessionId")
                .and_then(Value::as_str)
                .map(ToString::to_string),
            self.default_session(),
        );
        let (client, stream) = extract_target(object);

        if !object.contains_key("sessionId") {
            object.insert("sessionId".to_string(), Value::String(session.clone()));
        }
        if !object.contains_key("ts") {
            object.insert(
                "ts".to_string(),
                Value::Number(serde_json::Number::from(
                    chrono::Utc::now().timestamp_millis(),
                )),
            );
        }
        object.insert("clientName".to_string(), Value::String(client.clone()));
        object.insert("streamName".to_string(), Value::String(stream.clone()));

        let path = self.log_path(&session, &client, &stream);
        let line = serde_json::to_string(&Value::Object(object.clone()))? + "\n";
        fs::create_dir_all(self.outdir())?;
        use std::io::Write;
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .with_context(|| format!("failed to open {}", path.display()))?;
        file.write_all(line.as_bytes())
            .with_context(|| format!("failed to append {}", path.display()))?;

        Ok(Selection {
            session,
            client,
            stream,
        })
    }

    fn outdir(&self) -> &Path {
        Path::new(&self.config.outdir)
    }

    pub fn log_path(&self, session: &str, client: &str, stream: &str) -> PathBuf {
        if client == DEFAULT_CLIENT && stream == DEFAULT_STREAM {
            return self.legacy_log_path(session);
        }
        self.outdir()
            .join(format!(
                "trae-debug-log-{}--{}--{}.ndjson",
                session,
                safe_name(session, self.default_session()),
                format!(
                    "{}--{}",
                    safe_name(client, DEFAULT_CLIENT),
                    safe_name(stream, DEFAULT_STREAM)
                )
            ))
            .with_file_name(format!(
                "trae-debug-log-{}--{}--{}.ndjson",
                safe_name(session, self.default_session()),
                safe_name(client, DEFAULT_CLIENT),
                safe_name(stream, DEFAULT_STREAM)
            ))
    }

    fn legacy_log_path(&self, session: &str) -> PathBuf {
        self.outdir().join(format!(
            "trae-debug-log-{}.ndjson",
            safe_name(session, self.default_session())
        ))
    }
}

fn extract_target(object: &Map<String, Value>) -> (String, String) {
    let mut client = field_string(object, &["clientName"]);
    let mut stream = field_string(object, &["streamName", "stream"]);
    if let Some(data) = object.get("data").and_then(Value::as_object) {
        if client.is_none() {
            client = field_string(data, &["clientName"]);
        }
        if stream.is_none() {
            stream = field_string(data, &["streamName", "stream"]);
        }
    }
    (
        normalize(client, DEFAULT_CLIENT),
        normalize(stream, DEFAULT_STREAM),
    )
}

fn field_string(object: &Map<String, Value>, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| object.get(*key).and_then(Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn normalize(value: Option<String>, default: &str) -> String {
    value
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .unwrap_or_else(|| default.to_string())
}

fn non_empty<'a>(value: &'a str, default: &'a str) -> &'a str {
    if value.trim().is_empty() {
        default
    } else {
        value
    }
}

fn safe_name(value: &str, default: &str) -> String {
    let filtered = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-') {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    if filtered.is_empty() {
        default.to_string()
    } else {
        filtered
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::debug_server::config::DEFAULT_SESSION;

    fn test_state() -> DebugServerState {
        let tempdir = tempfile::tempdir().unwrap();
        let config = DebugServerConfig {
            host: "127.0.0.1".to_string(),
            port: 7777,
            session: DEFAULT_SESSION.to_string(),
            outdir: tempdir.path().to_string_lossy().to_string(),
            idle: 0,
        };
        let state = DebugServerState::new(config).unwrap();
        std::mem::forget(tempdir);
        state
    }

    #[test]
    fn append_event_uses_default_client_and_stream() {
        let state = test_state();
        let selection = state
            .append_event(serde_json::json!({ "msg": "hello" }))
            .unwrap();

        assert_eq!(selection.session, DEFAULT_SESSION);
        assert_eq!(selection.client, DEFAULT_CLIENT);
        assert_eq!(selection.stream, DEFAULT_STREAM);

        let lines = state
            .read_recent_lines(DEFAULT_SESSION, DEFAULT_CLIENT, DEFAULT_STREAM, 10)
            .unwrap();
        assert_eq!(lines.len(), 1);
        assert!(lines[0].contains("\"clientName\":\"default-client\""));
        assert!(lines[0].contains("\"streamName\":\"default-stream\""));
    }

    #[test]
    fn resolve_selection_falls_back_to_defaults() {
        let state = test_state();
        state
            .append_event(serde_json::json!({
                "sessionId": "session-b",
                "clientName": "client-b",
                "streamName": "stream-b",
                "msg": "event"
            }))
            .unwrap();

        let selection = state
            .resolve_selection(
                Some("missing-session".to_string()),
                Some("missing-client".to_string()),
                Some("missing-stream".to_string()),
            )
            .unwrap();

        assert_eq!(selection.session, DEFAULT_SESSION);
        assert_eq!(selection.client, DEFAULT_CLIENT);
        assert_eq!(selection.stream, DEFAULT_STREAM);

        let selection = state
            .resolve_selection(
                Some("session-b".to_string()),
                Some("client-b".to_string()),
                Some("stream-b".to_string()),
            )
            .unwrap();
        assert_eq!(selection.session, "session-b");
        assert_eq!(selection.client, "client-b");
        assert_eq!(selection.stream, "stream-b");
    }

    #[test]
    fn client_streams_group_by_session_client_and_stream() {
        let state = test_state();
        state
            .append_event(serde_json::json!({
                "sessionId": "session-x",
                "clientName": "client-a",
                "streamName": "stream-1",
                "msg": "one"
            }))
            .unwrap();
        state
            .append_event(serde_json::json!({
                "sessionId": "session-x",
                "clientName": "client-a",
                "streamName": "stream-2",
                "msg": "two"
            }))
            .unwrap();

        let hierarchy = state.client_streams("session-x").unwrap();
        assert_eq!(
            hierarchy.get("client-a"),
            Some(&vec!["stream-1".to_string(), "stream-2".to_string()])
        );
    }
}
