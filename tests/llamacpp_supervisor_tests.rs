use std::collections::{HashMap, HashSet};

use reqwest::Client;

use spear_next::spearlet::config::SpearletConfig;
use spear_next::spearlet::local_models::llamacpp::LlamaCppSupervisor;

#[tokio::test]
async fn test_llamacpp_supervisor_raw_mode_start_and_stop() {
    let tmp = tempfile::TempDir::new().unwrap();
    let mut cfg = SpearletConfig::default();
    cfg.storage.data_dir = tmp.path().to_string_lossy().to_string();
    let sup = LlamaCppSupervisor::new(&cfg);
    let http = Client::builder()
        .timeout(std::time::Duration::from_secs(1))
        .build()
        .unwrap();

    let mut params = HashMap::new();
    params.insert("server_mode".to_string(), "raw".to_string());
    params.insert("server_cmd".to_string(), "sleep".to_string());
    params.insert("server_cmd_args".to_string(), "60".to_string());
    params.insert("ready_probe".to_string(), "none".to_string());

    let b = sup
        .ensure_server(&http, "d1", "k1", "m1", &params)
        .await
        .unwrap();
    let spec = b.spec.as_ref().unwrap();
    assert_eq!(spec.provider, "llamacpp");
    assert_eq!(
        spec.hosting,
        spear_next::proto::sms::BackendHosting::NodeLocal as i32
    );
    assert!(sup.get_backend("d1").await.is_some());

    let live: HashSet<String> = HashSet::new();
    sup.stop_removed(&live).await;
    assert!(sup.get_backend("d1").await.is_none());
}

#[tokio::test]
async fn test_llamacpp_supervisor_restarts_on_spec_change() {
    let tmp = tempfile::TempDir::new().unwrap();
    let mut cfg = SpearletConfig::default();
    cfg.storage.data_dir = tmp.path().to_string_lossy().to_string();
    let sup = LlamaCppSupervisor::new(&cfg);
    let http = Client::builder()
        .timeout(std::time::Duration::from_secs(1))
        .build()
        .unwrap();

    let mut params = HashMap::new();
    params.insert("server_mode".to_string(), "raw".to_string());
    params.insert("server_cmd".to_string(), "sleep".to_string());
    params.insert("server_cmd_args".to_string(), "60".to_string());
    params.insert("ready_probe".to_string(), "none".to_string());

    let b1 = sup
        .ensure_server(&http, "d1", "k1", "m1", &params)
        .await
        .unwrap();
    let b2 = sup
        .ensure_server(&http, "d1", "k2", "m1", &params)
        .await
        .unwrap();
    assert_ne!(
        b1.spec.as_ref().unwrap().base_url,
        b2.spec.as_ref().unwrap().base_url
    );

    sup.stop_all().await;
    assert!(sup.get_backend("d1").await.is_none());
}
