use crate::spearlet::execution::host_api::DefaultHostApi;
use std::collections::HashMap;
use std::future::Future;

pub(super) fn expand_template(
    s: &str,
    vars: &HashMap<String, String>,
    env: &HashMap<String, String>,
) -> String {
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    let bytes = s.as_bytes();
    while i < bytes.len() {
        if bytes[i] == b'$' && i + 1 < bytes.len() && bytes[i + 1] == b'{' {
            if let Some(end) = s[i + 2..].find('}') {
                let key = &s[i + 2..i + 2 + end];
                let repl = if let Some(rest) = key.strip_prefix("env:") {
                    env.get(rest)
                        .cloned()
                        .or_else(|| std::env::var(rest).ok())
                        .unwrap_or_default()
                } else {
                    vars.get(key).cloned().unwrap_or_default()
                };
                out.push_str(&repl);
                i = i + 2 + end + 1;
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

pub(super) fn expand_json_templates(
    v: serde_json::Value,
    vars: &HashMap<String, String>,
    env: &HashMap<String, String>,
) -> serde_json::Value {
    match v {
        serde_json::Value::String(s) => serde_json::Value::String(expand_template(&s, vars, env)),
        serde_json::Value::Array(a) => serde_json::Value::Array(
            a.into_iter()
                .map(|x| expand_json_templates(x, vars, env))
                .collect(),
        ),
        serde_json::Value::Object(o) => serde_json::Value::Object(
            o.into_iter()
                .map(|(k, v)| (k, expand_json_templates(v, vars, env)))
                .collect(),
        ),
        other => other,
    }
}

pub(super) fn extract_json_path(v: &serde_json::Value, path: &str) -> Option<String> {
    let mut cur = v;
    for part in path.split('.') {
        cur = cur.get(part)?;
    }
    match cur {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        serde_json::Value::Bool(b) => Some(b.to_string()),
        _ => Some(cur.to_string()),
    }
}

pub(super) fn build_ws_request_with_headers(
    ws_url: &str,
    headers_kv: &[(String, String)],
    vars: &HashMap<String, String>,
    env: &HashMap<String, String>,
) -> Result<tokio_tungstenite::tungstenite::handshake::client::Request, String> {
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;
    use tokio_tungstenite::tungstenite::http::header::{HeaderName, HeaderValue};

    let mut req = ws_url
        .into_client_request()
        .map_err(|e| format!("invalid ws url: {e}"))?;
    let headers = req.headers_mut();

    for (k, v) in headers_kv.iter() {
        let name = HeaderName::from_bytes(k.as_bytes())
            .map_err(|_| format!("invalid header name: {k}"))?;
        let value_s = expand_template(v, vars, env);
        let value =
            HeaderValue::from_str(&value_s).map_err(|_| format!("invalid header value for {k}"))?;
        headers.insert(name, value);
    }
    Ok(req)
}

impl DefaultHostApi {
    pub(super) fn block_on<F>(&self, fut: F) -> F::Output
    where
        F: Future,
    {
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            return handle.block_on(fut);
        }

        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(fut)
    }

    pub(super) fn spawn_background<F>(&self, fut: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(fut);
            return;
        }

        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build();
            if let Ok(rt) = rt {
                rt.block_on(fut);
            }
        });
    }
}
