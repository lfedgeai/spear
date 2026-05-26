//! Rust WASM sample: interactive user stream chat completion via Boa JS runtime
//! Rust WASM 示例：通过 Boa JS 运行时实现基于 user stream 的交互式对话（Chat Completion）

#![deny(unsafe_op_in_unsafe_fn)]

use boa_engine::builtins::promise::PromiseState;
use boa_engine::js_string;
use boa_engine::module::ModuleLoader;
use boa_engine::object::builtins::JsPromise;
use boa_engine::Source;
use std::rc::Rc;

const DEFAULT_ENTRY: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/entry.mjs"));

fn main() {
    let loader = Rc::new(spear_boa::BuiltinModuleRegistry::new());
    let mut context = spear_boa::build_context(loader.clone()).expect("build context");
    spear_boa::init_tool_runtime(&mut context, spear_boa::DEFAULT_TOOL_SLOTS);
    spear_boa::install_native_bindings(&mut context);

    // `SPEAR_JS_ENTRY` optionally points to a JS module file (WASI preopen required).
    // `SPEAR_JS_ENTRY` 可选指定 JS 模块文件路径（需要 WASI 预打开目录）。
    let src = std::env::var("SPEAR_JS_ENTRY").ok();
    let entry_code =
        src.and_then(|path| std::fs::read_to_string(path).ok())
            .unwrap_or_else(|| DEFAULT_ENTRY.to_string());

    // Register the entry module under a fixed specifier.
    // 将入口模块注册到固定 specifier。
    let specifier = "app";
    let entry_source = Source::from_bytes(entry_code.as_bytes());
    let module = boa_engine::module::Module::parse(entry_source, None, &mut context)
        .expect("parse entry module");
    loader.register_module(js_string!(specifier), module.clone());

    let _promise = module.load_link_evaluate(&mut context);
    context.run_jobs();

    let ns = module.namespace(&mut context);
    let entry = ns
        .get(js_string!("default"), &mut context)
        .or_else(|_| ns.get(js_string!("main"), &mut context))
        .expect("get entry export");
    let Some(func) = entry.as_callable().cloned() else {
        eprintln!("entry export is not callable (expected default export or named export main)");
        std::process::exit(2);
    };

    let res = func
        .call(&boa_engine::JsValue::Undefined, &[], &mut context)
        .expect("call default export");
    context.run_jobs();

    let settled = if res.is_promise() {
        let p = JsPromise::from_object(res.as_object().cloned().unwrap()).expect("promise wrapper");

        // Drain microtasks until the promise settles (bounded).
        // 执行微任务直到 Promise settle（有界循环）。
        for _ in 0..32 {
            if matches!(p.state(), PromiseState::Pending) {
                context.run_jobs();
            } else {
                break;
            }
        }

        match p.state() {
            PromiseState::Fulfilled(v) => v,
            PromiseState::Rejected(v) => {
                if let Ok(s) = v.to_string(&mut context) {
                    eprintln!("rejected: {}", s.to_std_string_escaped());
                } else {
                    eprintln!("rejected");
                }
                std::process::exit(1);
            }
            PromiseState::Pending => {
                eprintln!("promise still pending after draining job queue");
                std::process::exit(3);
            }
        }
    } else {
        res
    };

    if let Ok(s) = settled.to_string(&mut context) {
        println!("{}", s.to_std_string_escaped());
    }
}
