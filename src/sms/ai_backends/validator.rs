//! Validation helpers for unified AI backends / 统一 AI backend 的校验辅助

use crate::ai_backend_types::{AllowedHosting, CanonicalBackendKind};
use crate::sms::ai_backends::model::{
    AiBackendHostingModel, AiBackendManagementModeModel, AiBackendPlacementRecordModel,
    AiBackendRecordModel, AiBackendSpecModel,
};
use crate::sms::services::error::SmsError;

/// Validate a backend record before persistence / 在持久化前校验 backend 记录
pub fn validate_backend_record(record: &AiBackendRecordModel) -> Result<(), SmsError> {
    validate_non_empty(&record.backend_id, "backend_id")?;
    validate_non_empty(&record.display_name, "display_name")?;
    validate_non_empty(&record.provider, "provider")?;
    validate_non_empty(&record.model, "model")?;
    validate_non_empty(&record.backend_kind, "backend_kind")?;
    validate_backend_spec(&record.spec)?;
    validate_metadata_is_object(&record.metadata)?;
    validate_hosting_management_mode(record)?;
    validate_backend_kind_for_hosting(record)?;
    validate_record_spec_consistency(record)?;
    Ok(())
}

/// Validate a placement record before persistence / 在持久化前校验 placement 记录
pub fn validate_placement_record(record: &AiBackendPlacementRecordModel) -> Result<(), SmsError> {
    validate_non_empty(&record.placement_id, "placement_id")?;
    validate_non_empty(&record.backend_id, "backend_id")?;
    validate_non_empty(&record.node_uuid, "node_uuid")?;
    Ok(())
}

/// Validate the backend execution spec / 校验 backend 执行规格
pub fn validate_backend_spec(spec: &AiBackendSpecModel) -> Result<(), SmsError> {
    validate_non_empty(&spec.name, "spec.name")?;
    validate_non_empty(&spec.kind, "spec.kind")?;
    validate_non_empty(&spec.provider, "spec.provider")?;
    validate_non_empty(&spec.model, "spec.model")?;
    if spec.operations.is_empty() {
        return Err(SmsError::InvalidRequest(
            "spec.operations must not be empty".to_string(),
        ));
    }
    if !spec.deployment_id.trim().is_empty() {
        return Err(SmsError::InvalidRequest(
            "spec.deployment_id is legacy and must be empty".to_string(),
        ));
    }
    Ok(())
}

/// Ensure JSON metadata remains object-shaped / 确保 JSON metadata 保持对象结构
pub fn validate_metadata_is_object(metadata: &serde_json::Value) -> Result<(), SmsError> {
    if metadata.is_object() {
        Ok(())
    } else {
        Err(SmsError::InvalidRequest(
            "metadata must be a JSON object".to_string(),
        ))
    }
}

/// Validate a required string field / 校验必填字符串字段
fn validate_non_empty(value: &str, field_name: &str) -> Result<(), SmsError> {
    if value.trim().is_empty() {
        return Err(SmsError::InvalidRequest(format!(
            "{field_name} must not be empty"
        )));
    }
    Ok(())
}

fn validate_hosting_management_mode(record: &AiBackendRecordModel) -> Result<(), SmsError> {
    let matches = matches!(
        (record.hosting, record.management_mode),
        (
            AiBackendHostingModel::Remote,
            AiBackendManagementModeModel::SmsRemote
        ) | (
            AiBackendHostingModel::Local,
            AiBackendManagementModeModel::SmsLocal
        )
    );
    if matches {
        Ok(())
    } else {
        Err(SmsError::InvalidRequest(
            "hosting and management_mode must match".to_string(),
        ))
    }
}

fn validate_backend_kind_for_hosting(record: &AiBackendRecordModel) -> Result<(), SmsError> {
    let kind = CanonicalBackendKind::parse(&record.backend_kind);

    match record.hosting {
        AiBackendHostingModel::Remote if kind.allowed_hosting() == AllowedHosting::LocalOnly => {
            Err(SmsError::InvalidRequest(format!(
                "backend_kind {} is local-only and cannot be used with remote hosting",
                record.backend_kind
            )))
        }
        AiBackendHostingModel::Local if kind.allowed_hosting() == AllowedHosting::RemoteOnly => {
            Err(SmsError::InvalidRequest(format!(
                "backend_kind {} is remote-only and cannot be used with local hosting",
                record.backend_kind
            )))
        }
        _ => Ok(()),
    }
}

fn validate_record_spec_consistency(record: &AiBackendRecordModel) -> Result<(), SmsError> {
    if record.provider.trim() != record.spec.provider.trim() {
        return Err(SmsError::InvalidRequest(
            "provider and spec.provider must match".to_string(),
        ));
    }
    if record.model.trim() != record.spec.model.trim() {
        return Err(SmsError::InvalidRequest(
            "model and spec.model must match".to_string(),
        ));
    }
    if record.backend_kind.trim() != record.spec.kind.trim() {
        return Err(SmsError::InvalidRequest(
            "backend_kind and spec.kind must match".to_string(),
        ));
    }

    let record_credential = record.credential_ref.as_deref().unwrap_or("").trim();
    let spec_credential = record.spec.credential_ref.trim();
    if record_credential != spec_credential {
        return Err(SmsError::InvalidRequest(
            "credential_ref and spec.credential_ref must match".to_string(),
        ));
    }

    if matches!(record.hosting, AiBackendHostingModel::Remote)
        && record.spec.base_url.trim().is_empty()
    {
        return Err(SmsError::InvalidRequest(
            "remote backends require spec.base_url".to_string(),
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sms::ai_backends::model::{
        AiBackendDesiredStateModel, AiBackendHostingModel, AiBackendManagementModeModel,
    };
    use std::collections::BTreeMap;

    fn sample_spec() -> AiBackendSpecModel {
        AiBackendSpecModel {
            name: "backend-1".to_string(),
            kind: "openai-compatible".to_string(),
            operations: vec!["chat".to_string()],
            features: vec!["stream".to_string()],
            transports: vec!["http".to_string()],
            weight: 1,
            priority: 0,
            base_url: "https://example.com".to_string(),
            provider: "openai".to_string(),
            model: "gpt-4.1".to_string(),
            credential_ref: "cred-1".to_string(),
            origin: 0,
            deployment_id: String::new(),
        }
    }

    #[test]
    fn rejects_non_object_metadata() {
        let record = AiBackendRecordModel {
            backend_id: "backend-1".to_string(),
            display_name: "Backend 1".to_string(),
            provider: "openai".to_string(),
            model: "gpt-4.1".to_string(),
            hosting: AiBackendHostingModel::Remote,
            backend_kind: "openai-compatible".to_string(),
            desired_state: AiBackendDesiredStateModel::Enabled,
            management_mode: AiBackendManagementModeModel::SmsRemote,
            credential_ref: Some("cred-1".to_string()),
            spec: sample_spec(),
            labels: BTreeMap::new(),
            metadata: serde_json::Value::String("bad".to_string()),
            generation: 1,
            created_at_ms: 1,
            updated_at_ms: 1,
        };

        let err = validate_backend_record(&record).expect_err("metadata should fail");
        assert!(matches!(err, SmsError::InvalidRequest(_)));
    }

    #[test]
    fn rejects_hosting_management_mode_mismatch() {
        let record = AiBackendRecordModel {
            backend_id: "backend-1".to_string(),
            display_name: "Backend 1".to_string(),
            provider: "openai".to_string(),
            model: "gpt-4.1".to_string(),
            hosting: AiBackendHostingModel::Remote,
            backend_kind: "openai_chat_completion".to_string(),
            desired_state: AiBackendDesiredStateModel::Enabled,
            management_mode: AiBackendManagementModeModel::SmsLocal,
            credential_ref: Some("cred-1".to_string()),
            spec: AiBackendSpecModel {
                kind: "openai_chat_completion".to_string(),
                operations: vec!["chat_completions".to_string()],
                provider: "openai".to_string(),
                model: "gpt-4.1".to_string(),
                base_url: "https://api.openai.com/v1".to_string(),
                credential_ref: "cred-1".to_string(),
                ..sample_spec()
            },
            labels: BTreeMap::new(),
            metadata: serde_json::json!({}),
            generation: 1,
            created_at_ms: 1,
            updated_at_ms: 1,
        };

        let err =
            validate_backend_record(&record).expect_err("mismatched management_mode should fail");
        assert!(matches!(err, SmsError::InvalidRequest(_)));
    }

    #[test]
    fn rejects_remote_with_local_only_backend_kind() {
        let record = AiBackendRecordModel {
            backend_id: "backend-1".to_string(),
            display_name: "Backend 1".to_string(),
            provider: "llamacpp".to_string(),
            model: "qwen".to_string(),
            hosting: AiBackendHostingModel::Remote,
            backend_kind: "llamacpp".to_string(),
            desired_state: AiBackendDesiredStateModel::Enabled,
            management_mode: AiBackendManagementModeModel::SmsRemote,
            credential_ref: None,
            spec: AiBackendSpecModel {
                kind: "llamacpp".to_string(),
                operations: vec!["chat_completions".to_string()],
                provider: "llamacpp".to_string(),
                model: "qwen".to_string(),
                base_url: "https://example.com".to_string(),
                credential_ref: String::new(),
                ..sample_spec()
            },
            labels: BTreeMap::new(),
            metadata: serde_json::json!({}),
            generation: 1,
            created_at_ms: 1,
            updated_at_ms: 1,
        };

        let err = validate_backend_record(&record)
            .expect_err("remote hosting should reject local-only kind");
        assert!(matches!(err, SmsError::InvalidRequest(_)));
    }

    #[test]
    fn rejects_remote_with_local_only_backend_kind_alias() {
        let record = AiBackendRecordModel {
            backend_id: "backend-1".to_string(),
            display_name: "Backend 1".to_string(),
            provider: "llamacpp".to_string(),
            model: "qwen".to_string(),
            hosting: AiBackendHostingModel::Remote,
            backend_kind: "llama.cpp".to_string(),
            desired_state: AiBackendDesiredStateModel::Enabled,
            management_mode: AiBackendManagementModeModel::SmsRemote,
            credential_ref: None,
            spec: AiBackendSpecModel {
                kind: "llama.cpp".to_string(),
                operations: vec!["chat_completions".to_string()],
                provider: "llamacpp".to_string(),
                model: "qwen".to_string(),
                base_url: "https://example.com".to_string(),
                credential_ref: String::new(),
                ..sample_spec()
            },
            labels: BTreeMap::new(),
            metadata: serde_json::json!({}),
            generation: 1,
            created_at_ms: 1,
            updated_at_ms: 1,
        };

        let err = validate_backend_record(&record)
            .expect_err("remote hosting should reject local-only alias kind");
        assert!(matches!(err, SmsError::InvalidRequest(_)));
    }

    #[test]
    fn allows_local_ollama_chat_backend_kind() {
        let record = AiBackendRecordModel {
            backend_id: "backend-1".to_string(),
            display_name: "Backend 1".to_string(),
            provider: "ollama".to_string(),
            model: "llama3.1:8b".to_string(),
            hosting: AiBackendHostingModel::Local,
            backend_kind: "ollama_chat".to_string(),
            desired_state: AiBackendDesiredStateModel::Enabled,
            management_mode: AiBackendManagementModeModel::SmsLocal,
            credential_ref: None,
            spec: AiBackendSpecModel {
                kind: "ollama_chat".to_string(),
                operations: vec!["chat_completions".to_string()],
                provider: "ollama".to_string(),
                model: "llama3.1:8b".to_string(),
                base_url: "http://127.0.0.1:11434".to_string(),
                credential_ref: String::new(),
                ..sample_spec()
            },
            labels: BTreeMap::new(),
            metadata: serde_json::json!({}),
            generation: 1,
            created_at_ms: 1,
            updated_at_ms: 1,
        };

        validate_backend_record(&record).expect("local ollama_chat should be allowed");
    }

    #[test]
    fn rejects_non_empty_legacy_deployment_id() {
        let mut spec = sample_spec();
        spec.deployment_id = "legacy-deployment".to_string();

        let err = validate_backend_spec(&spec).expect_err("legacy deployment_id should fail");
        assert!(matches!(err, SmsError::InvalidRequest(_)));
    }

    #[test]
    fn rejects_record_spec_provider_mismatch() {
        let record = AiBackendRecordModel {
            backend_id: "backend-1".to_string(),
            display_name: "Backend 1".to_string(),
            provider: "openai".to_string(),
            model: "gpt-4.1".to_string(),
            hosting: AiBackendHostingModel::Remote,
            backend_kind: "openai_chat_completion".to_string(),
            desired_state: AiBackendDesiredStateModel::Enabled,
            management_mode: AiBackendManagementModeModel::SmsRemote,
            credential_ref: Some("cred-1".to_string()),
            spec: AiBackendSpecModel {
                kind: "openai_chat_completion".to_string(),
                operations: vec!["chat_completions".to_string()],
                provider: "azure-openai".to_string(),
                model: "gpt-4.1".to_string(),
                base_url: "https://api.openai.com/v1".to_string(),
                credential_ref: "cred-1".to_string(),
                ..sample_spec()
            },
            labels: BTreeMap::new(),
            metadata: serde_json::json!({}),
            generation: 1,
            created_at_ms: 1,
            updated_at_ms: 1,
        };

        let err = validate_backend_record(&record).expect_err("provider mismatch should fail");
        assert!(matches!(err, SmsError::InvalidRequest(_)));
    }
}
