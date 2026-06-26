use spear_next::spearlet::config::{AiBackendConfig, AiCredentialConfig, SpearletConfig};
use spear_next::spearlet::execution::runtime::wasm_hostcalls::build_spear_import_with_api;
use spear_next::spearlet::execution::runtime::{ResourcePoolConfig, RuntimeConfig, RuntimeType};
use std::collections::HashMap;
use wasmedge_sdk::config::{CommonConfigOptions, ConfigBuilder};
use wasmedge_sdk::vm::SyncInst;
use wasmedge_sdk::wasi::WasiModule;
use wasmedge_sdk::{params, Module, Store, Vm};

mod common;

#[test]
fn test_wasm_to_openai_chat_completion_e2e() {
    let resolved = match common::resolve_live_chat_backend() {
        Some(v) => v,
        None => {
            eprintln!(
                "skipped: missing ai backend config/env for wasm e2e (need chat_completions + http)"
            );
            return;
        }
    };

    let model = std::env::var("OPENAI_MODEL").unwrap_or_else(|_| "gpt-4o-mini".to_string());
    let model_param_json = format!(r#"{{"key":"model","value":"{}"}}"#, model);
    let model_param_wat = model_param_json.replace('"', "\\\"");
    let model_param_len = model_param_json.as_bytes().len();

    let mut cfg = SpearletConfig::default();
    cfg.ai.credentials.push(AiCredentialConfig {
        name: "openai_default".to_string(),
        kind: "env".to_string(),
        api_key_env: resolved.api_key_env.clone(),
    });
    cfg.ai.backends.push(AiBackendConfig {
        name: "openai".to_string(),
        kind: "openai_chat_completion".to_string(),
        base_url: resolved.base_url,
        hosting: None,
        model: None,
        credential_ref: Some("openai_default".to_string()),
        provider: None,
        origin: None,
        deployment_id: None,
        weight: 100,
        priority: 0,
        ops: vec!["chat_completions".to_string()],
        features: vec![
            "supports_tools".to_string(),
            "supports_json_schema".to_string(),
            "supports_stream".to_string(),
        ],
        transports: vec!["http".to_string()],
    });

    let mut global_env = HashMap::new();
    global_env.insert(resolved.api_key_env, resolved.api_key);

    let runtime_config = RuntimeConfig {
        runtime_type: RuntimeType::Wasm,
        settings: HashMap::new(),
        global_environment: global_env,
        spearlet_config: Some(cfg),
        resource_pool: ResourcePoolConfig::default(),
    };

    let wat = format!(
        r#"(module
          (type $create_t (func (result i32)))
          (type $write_msg_t (func (param i32 i32 i32 i32 i32) (result i32)))
          (type $ctl_t (func (param i32 i32 i32 i32) (result i32)))
          (type $send_t (func (param i32 i32) (result i32)))
          (type $recv_t (func (param i32 i32 i32) (result i32)))

          (import "spear" "cchat_create" (func $cchat_create (type $create_t)))
          (import "spear" "cchat_write_msg" (func $cchat_write_msg (type $write_msg_t)))
          (import "spear" "cchat_ctl" (func $cchat_ctl (type $ctl_t)))
          (import "spear" "cchat_send" (func $cchat_send (type $send_t)))
          (import "spear" "cchat_recv" (func $cchat_recv (type $recv_t)))

          (memory (export "memory") 1)

          (data (i32.const 0) "user")
          (data (i32.const 16) "Reply with exactly: pong")

          (data (i32.const 64) "{}")
          (data (i32.const 256) "\00\00\00\00")

          (func $find_pong (param $ptr i32) (param $len i32) (result i32)
            (local $i i32)
            (local $end i32)
            (if (i32.lt_s (local.get $len) (i32.const 4))
              (then (return (i32.const -1)))
            )
            (local.set $i (i32.const 0))
            (local.set $end (i32.sub (local.get $len) (i32.const 4)))
            (loop $loop
              (if (i32.gt_s (local.get $i) (local.get $end))
                (then (return (i32.const -1)))
              )
              (if (i32.eq (i32.load8_u (i32.add (local.get $ptr) (local.get $i))) (i32.const 112))
                (then
                  (if (i32.eq (i32.load8_u (i32.add (local.get $ptr) (i32.add (local.get $i) (i32.const 1)))) (i32.const 111))
                    (then
                      (if (i32.eq (i32.load8_u (i32.add (local.get $ptr) (i32.add (local.get $i) (i32.const 2)))) (i32.const 110))
                        (then
                          (if (i32.eq (i32.load8_u (i32.add (local.get $ptr) (i32.add (local.get $i) (i32.const 3)))) (i32.const 103))
                            (then (return (i32.const 0)))
                          )
                        )
                      )
                    )
                  )
                )
              )
              (local.set $i (i32.add (local.get $i) (i32.const 1)))
              (br $loop)
            )
            (i32.const -1)
          )

          (func (export "run") (result i32)
            (local $fd i32)
            (local $resp_fd i32)
            (local $rc i32)
            (local $len i32)
            (local $found i32)

            (local.set $fd (call $cchat_create))
            (drop (call $cchat_write_msg (local.get $fd) (i32.const 0) (i32.const 4) (i32.const 16) (i32.const 24)))

            (i32.store (i32.const 256) (i32.const {model_json_len}))
            (drop (call $cchat_ctl (local.get $fd) (i32.const 1) (i32.const 64) (i32.const 256)))

            (local.set $resp_fd (call $cchat_send (local.get $fd) (i32.const 0)))
            (if (i32.lt_s (local.get $resp_fd) (i32.const 0))
              (then (return (local.get $resp_fd)))
            )

            (i32.store (i32.const 1020) (i32.const 60000))
            (local.set $rc (call $cchat_recv (local.get $resp_fd) (i32.const 2048) (i32.const 1020)))
            (if (i32.lt_s (local.get $rc) (i32.const 0))
              (then (return (local.get $rc)))
            )

            (local.set $len (i32.load (i32.const 1020)))
            (local.set $found (call $find_pong (i32.const 2048) (local.get $len)))
            (if (i32.eq (local.get $found) (i32.const 0))
              (then (return (i32.const 0)))
            )
            (i32.const -100)
          )
        )"#,
        model_param_wat,
        model_json_len = model_param_len
    );

    let wasm_bytes = wasmedge_sdk::wat2wasm(wat.as_bytes()).unwrap();

    let c = ConfigBuilder::new(CommonConfigOptions::default())
        .build()
        .unwrap();
    let mut wasi_module = WasiModule::create(None, None, None).unwrap();
    let mut instances: HashMap<String, &mut dyn SyncInst> = HashMap::new();
    instances.insert(wasi_module.name().to_string(), wasi_module.as_mut());

    let mcp_task_policy = std::sync::Arc::new(
        spear_next::spearlet::mcp::task_subset::parse_task_config(&HashMap::new()),
    );
    let mut spear_import = build_spear_import_with_api(
        runtime_config,
        "task-e2e".to_string(),
        mcp_task_policy,
        "inst-e2e".to_string(),
    )
    .unwrap();
    let spear_inst: &mut dyn SyncInst = &mut spear_import;
    instances.insert("spear".to_string(), spear_inst);

    let store = Store::new(Some(&c), instances).unwrap();
    let mut vm = Vm::new(store);
    let module = Module::from_bytes(None, wasm_bytes).unwrap();
    vm.register_module(Some("extern"), module).unwrap();

    let values = vm.run_func(Some("extern"), "run", params!()).unwrap();
    let rc = values.get(0).map(|v| v.to_i32()).unwrap_or(-999);
    assert_eq!(rc, 0);
}
