use spear_next::spearlet::config::{AppConfig, SpearletConfig};

#[allow(dead_code)]
pub struct ResolvedBackend {
    pub name: String,
    pub base_url: String,
    pub api_key_env: String,
    pub api_key: String,
}

fn live_tests_enabled() -> bool {
    std::env::var("SPEAR_RUN_LIVE_TESTS")
        .ok()
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

fn load_spearlet_config() -> Option<SpearletConfig> {
    let path = std::env::var("SPEAR_TEST_CONFIG")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| format!("{}/config/spearlet/config.toml", env!("CARGO_MANIFEST_DIR")));

    let s = std::fs::read_to_string(&path).ok()?;
    let cfg: AppConfig = toml::from_str(&s).ok()?;
    Some(cfg.spearlet)
}

fn resolve_backend(op: &str, transport: &str, kinds: &[&str]) -> Option<ResolvedBackend> {
    let cfg = load_spearlet_config()?;

    let backend = cfg.ai.backends.iter().find(|b| {
        b.ops.iter().any(|x| x == op)
            && b.transports.iter().any(|x| x == transport)
            && kinds.iter().any(|k| *k == b.kind)
    })?;

    let cred_name = backend.credential_ref.as_ref()?;
    let cred =
        cfg.ai.credentials.iter().find(|c| {
            c.name == *cred_name && c.kind == "env" && !c.api_key_env.trim().is_empty()
        })?;

    let env_name = cred.api_key_env.trim().to_string();
    let api_key = std::env::var(&env_name).ok()?.trim().to_string();
    if api_key.is_empty() {
        return None;
    }

    let base_url = backend.base_url.trim().to_string();
    if base_url.is_empty() {
        return None;
    }

    Some(ResolvedBackend {
        name: backend.name.clone(),
        base_url,
        api_key_env: env_name,
        api_key,
    })
}

#[allow(dead_code)]
pub fn resolve_live_chat_backend() -> Option<ResolvedBackend> {
    if !live_tests_enabled() {
        return None;
    }
    resolve_backend("chat_completions", "http", &["openai_chat_completion"])
}

#[allow(dead_code)]
pub fn resolve_realtime_asr_backend() -> Option<ResolvedBackend> {
    if !live_tests_enabled() {
        return None;
    }
    resolve_backend("speech_to_text", "websocket", &["openai_realtime_ws"])
}
