//! Shared local-provider resolution helpers.
//! 共享的本地 provider 解析辅助。

/// Canonical local provider kind recognized by node-side controllers.
/// 节点侧控制器识别的规范本地 provider 类型。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocalProviderKind {
    LlamaCpp,
    Vllm,
    Unsupported(String),
}

/// Resolve a local provider kind from multiple candidate strings.
/// 从多个候选字符串解析本地 provider 类型。
pub fn resolve_local_provider_kind<'a, I>(candidates: I) -> LocalProviderKind
where
    I: IntoIterator<Item = &'a str>,
{
    let normalized = candidates
        .into_iter()
        .map(|value| value.trim().to_ascii_lowercase())
        .collect::<Vec<_>>();

    if normalized.iter().any(|value| is_llamacpp_kind(value)) {
        return LocalProviderKind::LlamaCpp;
    }
    if normalized.iter().any(|value| is_vllm_kind(value)) {
        return LocalProviderKind::Vllm;
    }

    LocalProviderKind::Unsupported(
        normalized
            .into_iter()
            .find(|value| !value.is_empty())
            .unwrap_or_else(|| "unspecified".to_string()),
    )
}

fn is_llamacpp_kind(kind: &str) -> bool {
    kind == "llamacpp" || kind == "llama_cpp" || kind == "llama.cpp"
}

fn is_vllm_kind(kind: &str) -> bool {
    kind == "vllm" || kind == "vllm-openai"
}

#[cfg(test)]
mod tests {
    use super::{resolve_local_provider_kind, LocalProviderKind};

    #[test]
    fn resolves_llamacpp_from_aliases() {
        assert_eq!(
            resolve_local_provider_kind(["llama.cpp"]),
            LocalProviderKind::LlamaCpp
        );
        assert_eq!(
            resolve_local_provider_kind(["llama_cpp"]),
            LocalProviderKind::LlamaCpp
        );
    }

    #[test]
    fn resolves_vllm_from_aliases() {
        assert_eq!(
            resolve_local_provider_kind(["vllm-openai"]),
            LocalProviderKind::Vllm
        );
    }

    #[test]
    fn reports_first_non_empty_unknown_value() {
        assert_eq!(
            resolve_local_provider_kind(["", "mystery-provider"]),
            LocalProviderKind::Unsupported("mystery-provider".to_string())
        );
    }
}
