//! Boa integration for Spear
//! Spear 的 Boa 集成层
//!
//! This crate injects virtual modules (e.g. `spear`, `spear/chat`) into Boa.
//! 该 crate 会向 Boa 注入虚拟模块（例如 `spear`、`spear/chat`）。
//!
//! Current milestone / 当前里程碑：
//! - M1: `Spear.chat.completions.create`.
//! - M1：实现 `Spear.chat.completions.create`。

#![deny(unsafe_op_in_unsafe_fn)]

use boa_engine::context::Context;
use boa_engine::js_string;
use boa_engine::module::{Module, ModuleLoader, Referrer};
use boa_engine::object::ObjectInitializer;
use boa_engine::property::Attribute;
use boa_engine::{JsNativeError, JsResult, JsString, JsValue, NativeFunction, Source};
use std::collections::HashMap;
use std::rc::Rc;
use std::{cell::RefCell, sync::Arc};

/// Tool arena size in bytes.
/// Tool arena 大小（字节）。
const TOOL_ARENA_BYTES: usize = 128 * 1024;

/// Tool arena memory.
/// Tool arena 内存。
///
/// Safety / 安全性：
/// - This is only used as a linear-memory scratch arena for tool calls.
/// - 这块内存仅用作 tool call 的线性内存 scratch arena。
/// - The host runtime writes args and reads outputs via pointers.
/// - 宿主 runtime 会通过指针写入参数并读取输出。
static mut TOOL_ARENA: [u8; TOOL_ARENA_BYTES] = [0u8; TOOL_ARENA_BYTES];

fn tool_arena_ptr_len() -> (u32, u32) {
    // Avoid creating a shared reference to `static mut`.
    // 避免创建指向 `static mut` 的共享引用。
    let ptr = std::ptr::addr_of!(TOOL_ARENA) as *const u8 as usize as u32;
    (ptr, TOOL_ARENA_BYTES as u32)
}

thread_local! {
    static TOOL_CTX_PTR: std::cell::Cell<*mut Context> = const { std::cell::Cell::new(std::ptr::null_mut()) };
    static TOOL_HANDLERS: RefCell<Vec<Option<JsValue>>> = const { RefCell::new(Vec::new()) };
}

/// Initialize tool runtime state for trampolines.
/// 初始化 tool trampoline 的运行时状态。
pub fn init_tool_runtime(context: &mut Context, slots: usize) {
    TOOL_CTX_PTR.with(|p| p.set(context as *mut Context));
    TOOL_HANDLERS.with(|h| {
        let mut h = h.borrow_mut();
        h.clear();
        h.resize(slots.max(1), None);
    });
}

fn with_tool_runtime_mut<R>(f: impl FnOnce(&mut Context, &mut [Option<JsValue>]) -> R) -> Result<R, i32> {
    let ctx_ptr = TOOL_CTX_PTR.with(|p| p.get());
    if ctx_ptr.is_null() {
        return Err(-libc::EIO);
    }
    TOOL_HANDLERS.with(|h| {
        let mut h = h.borrow_mut();
        // Safety / 安全性：
        // - `ctx_ptr` is set once during init and lives for the process lifetime.
        // - `ctx_ptr` 会在 init 阶段设置，并在进程生命周期内保持有效。
        let ctx = unsafe { &mut *ctx_ptr };
        Ok(f(ctx, h.as_mut_slice()))
    })
}

const SPEAR_MODULE: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/js/spear.mjs"));
const SPEAR_CHAT_MODULE: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/js/spear_chat.mjs"));
const SPEAR_SSF_MODULE: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/js/spear_ssf.mjs"));

fn json_stringify(ctx: &mut Context, value: JsValue) -> JsResult<String> {
    let json = ctx.global_object().get(js_string!("JSON"), ctx)?;
    let Some(json_obj) = json.as_object().cloned() else {
        return Err(JsNativeError::typ().with_message("JSON is not an object").into());
    };
    let stringify = json_obj.get(js_string!("stringify"), ctx)?;
    let Some(callable) = stringify.as_callable().cloned() else {
        return Err(JsNativeError::typ()
            .with_message("JSON.stringify is not callable")
            .into());
    };
    let out = callable.call(&json_obj.into(), &[value], ctx)?;
    Ok(out.to_string(ctx)?.to_std_string_escaped())
}

fn tool_error_json(code: &str, message: &str) -> String {
    // Keep it minimal and JSON-safe.
    // 保持简洁且 JSON 安全。
    let code = serde_json::Value::String(code.to_string());
    let message = serde_json::Value::String(message.to_string());
    serde_json::json!({"error": {"code": code, "message": message}}).to_string()
}

fn tool_write_output(out_ptr: i32, out_len_ptr: i32, payload: &[u8]) -> i32 {
    // Safety / 安全性：
    // - Tool trampolines are invoked by host runtime, pointers are within WASM linear memory.
    // - tool trampoline 由宿主 runtime 调用，指针位于 WASM 线性内存中。
    if out_ptr <= 0 || out_len_ptr <= 0 {
        return -libc::EFAULT;
    }
    let cap = unsafe { *(out_len_ptr as *const u32) } as usize;
    if cap < payload.len() {
        unsafe {
            *(out_len_ptr as *mut u32) = payload.len() as u32;
        }
        return -libc::ENOSPC;
    }
    unsafe {
        std::ptr::copy_nonoverlapping(payload.as_ptr(), out_ptr as *mut u8, payload.len());
        *(out_len_ptr as *mut u32) = payload.len() as u32;
    }
    0
}

fn tool_trampoline_common(slot: usize, args_ptr: i32, args_len: i32, out_ptr: i32, out_len_ptr: i32) -> i32 {
    if args_ptr <= 0 || args_len < 0 {
        let s = tool_error_json("invalid_args", "invalid args pointer/length");
        return tool_write_output(out_ptr, out_len_ptr, s.as_bytes());
    }

    let args_bytes = unsafe { std::slice::from_raw_parts(args_ptr as *const u8, args_len as usize) };
    let args_json = match std::str::from_utf8(args_bytes) {
        Ok(s) => s,
        Err(_) => {
            let s = tool_error_json("invalid_utf8", "args is not valid utf-8");
            return tool_write_output(out_ptr, out_len_ptr, s.as_bytes());
        }
    };

    let out = match with_tool_runtime_mut(|ctx, handlers| {
        if slot >= handlers.len() {
            return Ok::<_, JsNativeError>(tool_error_json("slot_oob", "tool slot out of range"));
        }
        let Some(handler) = handlers[slot].clone() else {
            return Ok::<_, JsNativeError>(tool_error_json("handler_not_found", "tool handler not found"));
        };

        let Some(callable) = handler.as_callable().cloned() else {
            return Ok::<_, JsNativeError>(tool_error_json("handler_not_callable", "tool handler not callable"));
        };

        let res = callable.call(
            &JsValue::undefined(),
            &[JsValue::from(js_string!(args_json))],
            ctx,
        );

        let res = match res {
            Ok(v) => v,
            Err(e) => {
                let msg = e.to_string();
                return Ok::<_, JsNativeError>(tool_error_json("handler_throw", &msg));
            }
        };

        json_stringify(ctx, res).map_err(|e| {
            JsNativeError::error().with_message(e.to_string())
        })
    }) {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => tool_error_json("stringify_failed", &e.to_string()),
        Err(errno) => tool_error_json("runtime_uninitialized", &format!("errno={errno}")),
    };

    tool_write_output(out_ptr, out_len_ptr, out.as_bytes())
}

seq_macro::seq!(N in 0..32 {
    #(
        #[no_mangle]
        pub extern "C" fn spear_tool_trampoline_~N(
            args_ptr: i32,
            args_len: i32,
            out_ptr: i32,
            out_len_ptr: i32,
        ) -> i32 {
            tool_trampoline_common(N, args_ptr, args_len, out_ptr, out_len_ptr)
        }
    )*

    const TOOL_TRAMPOLINES: [extern "C" fn(i32, i32, i32, i32) -> i32; 32] = [
        #(spear_tool_trampoline_~N,)*
    ];
});

/// Default tool trampoline slots.
/// 默认 tool trampoline 槽位数量。
pub const DEFAULT_TOOL_SLOTS: usize = TOOL_TRAMPOLINES.len();

fn tool_trampoline_offset(slot: usize) -> Option<i32> {
    TOOL_TRAMPOLINES
        .get(slot)
        .map(|f| *f as usize as i32)
}

#[derive(Debug, Clone)]
pub struct BuiltinModuleRegistry {
    sources: Rc<RefCell<HashMap<String, Arc<str>>>>,
    modules: Rc<RefCell<HashMap<String, Module>>>,
}

impl BuiltinModuleRegistry {
    pub fn new() -> Self {
        let sources: HashMap<String, Arc<str>> = HashMap::from([
            ("spear".to_string(), Arc::from(SPEAR_MODULE)),
            ("spear/chat".to_string(), Arc::from(SPEAR_CHAT_MODULE)),
            ("spear/ssf".to_string(), Arc::from(SPEAR_SSF_MODULE)),
        ]);
        Self {
            sources: Rc::new(RefCell::new(sources)),
            modules: Rc::new(RefCell::new(HashMap::new())),
        }
    }

    /// Insert a non-builtin module source (e.g. app entry).
    /// 插入非内置模块源码（例如应用入口）。
    pub fn insert_source(&self, specifier: &str, source: impl Into<Arc<str>>) {
        // Replace any previously cached module so new source takes effect.
        // 替换任何已缓存的模块，确保新源码生效。
        self.modules.borrow_mut().remove(specifier);
        self.sources
            .borrow_mut()
            .insert(specifier.to_string(), source.into());
    }

    fn load_or_parse(&self, specifier: &str, context: &mut Context) -> JsResult<Module> {
        if let Some(m) = self.modules.borrow().get(specifier).cloned() {
            return Ok(m);
        }
        let Some(src) = self.sources.borrow().get(specifier).cloned() else {
            return Err(JsNativeError::typ()
                .with_message("unknown module specifier")
                .into());
        };

        let source = Source::from_bytes(src.as_bytes());
        let module = Module::parse(source, None, context)?;
        self.modules.borrow_mut().insert(specifier.to_string(), module.clone());
        Ok(module)
    }
}

impl ModuleLoader for BuiltinModuleRegistry {
    fn load_imported_module(
        &self,
        _referrer: Referrer,
        specifier: JsString,
        finish_load: Box<dyn FnOnce(JsResult<Module>, &mut Context)>,
        context: &mut Context,
    ) {
        let key = specifier.to_std_string_escaped();
        let out = self.load_or_parse(&key, context);
        finish_load(out, context);
    }

    fn register_module(&self, specifier: JsString, module: Module) {
        self.modules
            .borrow_mut()
            .insert(specifier.to_std_string_escaped(), module);
    }

    fn get_module(&self, specifier: JsString) -> Option<Module> {
        self.modules
            .borrow()
            .get(&specifier.to_std_string_escaped())
            .cloned()
    }
}

/// Build a Boa context with Spear builtin modules.
/// 构建带 Spear 内置模块的 Boa Context。
pub fn build_context(loader: Rc<BuiltinModuleRegistry>) -> JsResult<Context> {
    Context::builder().module_loader(loader).build()
}

/// Install native bindings into an existing context.
/// 向已创建的 Context 安装原生绑定。
pub fn install_native_bindings(context: &mut Context) {
    // Native binding: `__spear_cchat_completion(options_json: string) -> string`
    // 原生绑定：`__spear_cchat_completion(options_json: string) -> string`
    let f = NativeFunction::from_fn_ptr(|_this, args, ctx| {
        let options_json = args
            .get(0)
            .cloned()
            .unwrap_or(JsValue::Undefined)
            .to_string(ctx)?
            .to_std_string_escaped();

        let out = match cchat_completion_impl(&options_json) {
            Ok(v) => v,
            Err(e) => {
                return Err(JsNativeError::error().with_message(e).into());
            }
        };

        Ok(JsValue::from(js_string!(out)))
    });

    let _ = context.register_global_builtin_callable(js_string!("__spear_cchat_completion"), 1, f);

    let print = NativeFunction::from_fn_ptr(|_this, args, ctx| {
        let s = args
            .get(0)
            .cloned()
            .unwrap_or(JsValue::Undefined)
            .to_string(ctx)?
            .to_std_string_escaped();
        let _ = spear_wasm::log_info(&s);
        Ok(JsValue::Undefined)
    });
    let _ = context.register_global_builtin_callable(js_string!("__spear_print"), 1, print);

    let sleep_ms = NativeFunction::from_fn_ptr(|_this, args, ctx| {
        let ms = args
            .get(0)
            .cloned()
            .unwrap_or(JsValue::Undefined)
            .to_number(ctx)? as u32;
        spear_wasm::sleep_ms(ms);
        Ok(JsValue::Undefined)
    });
    let _ = context.register_global_builtin_callable(js_string!("__spear_sleep_ms"), 1, sleep_ms);

    let tool_reg = NativeFunction::from_fn_ptr(|_this, args, ctx| {
        let fn_json = args
            .get(0)
            .cloned()
            .unwrap_or(JsValue::Undefined)
            .to_string(ctx)?
            .to_std_string_escaped();
        let handler = args.get(1).cloned().unwrap_or(JsValue::Undefined);
        let Some(_callable) = handler.as_callable() else {
            return Err(JsNativeError::typ()
                .with_message("handler must be callable")
                .into());
        };

        let slot = TOOL_HANDLERS.with(|h| {
            let mut h = h.borrow_mut();
            if h.is_empty() {
                h.resize(DEFAULT_TOOL_SLOTS, None);
            }
            h.iter()
                .position(|x| x.is_none())
                .ok_or_else(|| JsNativeError::range().with_message("tool_slot_exhausted"))
                .map(|i| {
                    h[i] = Some(handler.clone());
                    i
                })
        })?;

        let fn_offset = tool_trampoline_offset(slot)
            .ok_or_else(|| JsNativeError::error().with_message("missing trampoline"))?;

        let obj = ObjectInitializer::new(ctx)
            .property(js_string!("slot"), slot as i32, Attribute::all())
            .property(js_string!("fnOffset"), fn_offset, Attribute::all())
            .property(js_string!("fnJson"), js_string!(fn_json), Attribute::all())
            .build();

        Ok(obj.into())
    });
    let _ = context.register_global_builtin_callable(js_string!("__spear_tool_register"), 2, tool_reg);

    let user_stream_open = NativeFunction::from_fn_ptr(|_this, args, _ctx| {
        let stream_id = args.get(0).cloned().unwrap_or(JsValue::Undefined);
        let direction = args.get(1).cloned().unwrap_or(JsValue::Undefined);
        let stream_id = stream_id.to_number(_ctx)? as i32;
        let direction = direction.to_number(_ctx)? as i32;
        let fd = spear_wasm::user_stream_open(
            u32::try_from(stream_id).unwrap_or(0),
            match direction {
                1 => spear_wasm::UserStreamDirection::Inbound,
                2 => spear_wasm::UserStreamDirection::Outbound,
                _ => spear_wasm::UserStreamDirection::Bidirectional,
            },
        )
        .map_err(|e| JsNativeError::error().with_message(e.to_string()))?;
        Ok(JsValue::from(fd.raw()))
    });
    let _ = context.register_global_builtin_callable(js_string!("__spear_user_stream_open"), 2, user_stream_open);

    let user_stream_close = NativeFunction::from_fn_ptr(|_this, args, _ctx| {
        let fd = args.get(0).cloned().unwrap_or(JsValue::Undefined);
        let fd = fd.to_number(_ctx)? as i32;
        spear_wasm::user_stream_close(spear_wasm::Fd::from_raw(fd))
            .map_err(|e| JsNativeError::error().with_message(e.to_string()))?;
        Ok(JsValue::Undefined)
    });
    let _ = context.register_global_builtin_callable(js_string!("__spear_user_stream_close"), 1, user_stream_close);

    let user_stream_write = NativeFunction::from_fn_ptr(|_this, args, ctx| {
        let fd = args.get(0).cloned().unwrap_or(JsValue::Undefined);
        let data = args.get(1).cloned().unwrap_or(JsValue::Undefined);
        let fd = fd.to_number(ctx)? as i32;
        let s = data.to_string(ctx)?.to_std_string().map_err(|e| {
            JsNativeError::error().with_message(format!("invalid string: {e}"))
        })?;
        let bytes = s.encode_utf16().map(|u| (u & 0xFF) as u8).collect::<Vec<u8>>();
        spear_wasm::user_stream_write(spear_wasm::Fd::from_raw(fd), &bytes)
            .map_err(|e| JsNativeError::error().with_message(e.to_string()))?;
        Ok(JsValue::Undefined)
    });
    let _ = context.register_global_builtin_callable(js_string!("__spear_user_stream_write"), 2, user_stream_write);

    let user_stream_read = NativeFunction::from_fn_ptr(|_this, args, ctx| {
        let fd = args.get(0).cloned().unwrap_or(JsValue::Undefined);
        let fd = fd.to_number(ctx)? as i32;
        let bytes = spear_wasm::user_stream_read_alloc(spear_wasm::Fd::from_raw(fd))
            .map_err(|e| JsNativeError::error().with_message(e.to_string()))?;
        let Some(bytes) = bytes else {
            return Ok(JsValue::Null);
        };
        let u16s = bytes.into_iter().map(|b| b as u16).collect::<Vec<u16>>();
        let s = String::from_utf16_lossy(&u16s);
        Ok(JsValue::from(js_string!(s)))
    });
    let _ = context.register_global_builtin_callable(js_string!("__spear_user_stream_read"), 1, user_stream_read);

    let ctl_open = NativeFunction::from_fn_ptr(|_this, _args, _ctx| {
        let fd = spear_wasm::user_stream_ctl_open()
            .map_err(|e| JsNativeError::error().with_message(e.to_string()))?;
        Ok(JsValue::from(fd.raw()))
    });
    let _ = context.register_global_builtin_callable(js_string!("__spear_user_stream_ctl_open"), 0, ctl_open);

    let ctl_read = NativeFunction::from_fn_ptr(|_this, args, ctx| {
        let fd = args.get(0).cloned().unwrap_or(JsValue::Undefined);
        let fd = fd.to_number(ctx)? as i32;
        let evt = spear_wasm::user_stream_ctl_read_event(spear_wasm::Fd::from_raw(fd))
            .map_err(|e| JsNativeError::error().with_message(e.to_string()))?;
        let Some(evt) = evt else {
            return Ok(JsValue::Null);
        };
        let obj = ObjectInitializer::new(ctx)
            .property(js_string!("streamId"), evt.stream_id as i32, Attribute::all())
            .property(js_string!("kind"), evt.kind as i32, Attribute::all())
            .build();
        Ok(obj.into())
    });
    let _ = context.register_global_builtin_callable(js_string!("__spear_user_stream_ctl_read_event"), 1, ctl_read);

    let ssf_parse_v1 = NativeFunction::from_fn_ptr(|_this, args, ctx| {
        let bin = args
            .get(0)
            .cloned()
            .unwrap_or(JsValue::Undefined)
            .to_string(ctx)?
            .to_std_string()
            .map_err(|e| JsNativeError::error().with_message(format!("invalid string: {e}")))?;

        let bytes = bin
            .encode_utf16()
            .map(|u| (u & 0xFF) as u8)
            .collect::<Vec<u8>>();

        let (hdr, meta, data) = match spear_ssf::split_v1(&bytes) {
            Ok(v) => v,
            Err(_) => return Ok(JsValue::Null),
        };

        let meta_u16s = meta.iter().map(|b| *b as u16).collect::<Vec<u16>>();
        let data_u16s = data.iter().map(|b| *b as u16).collect::<Vec<u16>>();
        let meta_bin = String::from_utf16_lossy(&meta_u16s);
        let data_bin = String::from_utf16_lossy(&data_u16s);

        let seq_lo = (hdr.seq & 0xFFFF_FFFF) as u32;
        let seq_hi = (hdr.seq >> 32) as u32;

        let obj = ObjectInitializer::new(ctx)
            .property(js_string!("streamId"), hdr.stream_id as i32, Attribute::all())
            .property(js_string!("msgType"), hdr.msg_type as i32, Attribute::all())
            .property(js_string!("flags"), hdr.flags as i32, Attribute::all())
            .property(js_string!("seqLo"), seq_lo as i32, Attribute::all())
            .property(js_string!("seqHi"), seq_hi as i32, Attribute::all())
            .property(js_string!("metaBin"), js_string!(meta_bin), Attribute::all())
            .property(js_string!("dataBin"), js_string!(data_bin), Attribute::all())
            .build();
        Ok(obj.into())
    });
    let _ = context.register_global_builtin_callable(js_string!("__spear_ssf_parse_v1"), 1, ssf_parse_v1);

    let ssf_build_v1 = NativeFunction::from_fn_ptr(|_this, args, ctx| {
        let stream_id = args.get(0).cloned().unwrap_or(JsValue::Undefined).to_number(ctx)? as u32;
        let msg_type = args.get(1).cloned().unwrap_or(JsValue::Undefined).to_number(ctx)? as u16;
        let flags = args.get(2).cloned().unwrap_or(JsValue::Undefined).to_number(ctx)? as u16;
        let seq_lo = args.get(3).cloned().unwrap_or(JsValue::Undefined).to_number(ctx)? as u32;
        let seq_hi = args.get(4).cloned().unwrap_or(JsValue::Undefined).to_number(ctx)? as u32;
        let seq = ((seq_hi as u64) << 32) | (seq_lo as u64);

        let meta_bin = args
            .get(5)
            .cloned()
            .unwrap_or(JsValue::Undefined)
            .to_string(ctx)?
            .to_std_string()
            .map_err(|e| JsNativeError::error().with_message(format!("invalid string: {e}")))?;
        let data_bin = args
            .get(6)
            .cloned()
            .unwrap_or(JsValue::Undefined)
            .to_string(ctx)?
            .to_std_string()
            .map_err(|e| JsNativeError::error().with_message(format!("invalid string: {e}")))?;

        let meta = meta_bin
            .encode_utf16()
            .map(|u| (u & 0xFF) as u8)
            .collect::<Vec<u8>>();
        let data = data_bin
            .encode_utf16()
            .map(|u| (u & 0xFF) as u8)
            .collect::<Vec<u8>>();

        let frame = spear_ssf::build_v1_frame(stream_id, msg_type, flags, seq, &meta, &data);
        let u16s = frame.into_iter().map(|b| b as u16).collect::<Vec<u16>>();
        let s = String::from_utf16_lossy(&u16s);
        Ok(JsValue::from(js_string!(s)))
    });
    let _ = context.register_global_builtin_callable(js_string!("__spear_ssf_build_v1"), 7, ssf_build_v1);

    let utf8_encode = NativeFunction::from_fn_ptr(|_this, args, ctx| {
        let s = args
            .get(0)
            .cloned()
            .unwrap_or(JsValue::Undefined)
            .to_string(ctx)?
            .to_std_string()
            .map_err(|e| JsNativeError::error().with_message(format!("invalid string: {e}")))?;

        let bytes = s.as_bytes();
        let u16s = bytes.iter().map(|b| *b as u16).collect::<Vec<u16>>();
        let bin = String::from_utf16_lossy(&u16s);
        Ok(JsValue::from(js_string!(bin)))
    });
    let _ = context.register_global_builtin_callable(js_string!("__spear_utf8_encode"), 1, utf8_encode);

    let utf8_decode = NativeFunction::from_fn_ptr(|_this, args, ctx| {
        let bin = args
            .get(0)
            .cloned()
            .unwrap_or(JsValue::Undefined)
            .to_string(ctx)?
            .to_std_string()
            .map_err(|e| JsNativeError::error().with_message(format!("invalid string: {e}")))?;

        let bytes = bin
            .encode_utf16()
            .map(|u| (u & 0xFF) as u8)
            .collect::<Vec<u8>>();
        let s = String::from_utf8_lossy(&bytes).to_string();
        Ok(JsValue::from(js_string!(s)))
    });
    let _ = context.register_global_builtin_callable(js_string!("__spear_utf8_decode"), 1, utf8_decode);
}

fn cchat_completion_impl(options_json: &str) -> Result<String, String> {
    let options: serde_json::Value = serde_json::from_str(options_json)
        .map_err(|e| format!("invalid options json: {e}"))?;

    let model = options
        .get("model")
        .and_then(|v| v.as_str())
        .unwrap_or("gpt-4o-mini");

    let backend = options
        .get("backend")
        .and_then(|v| v.as_str())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty());

    let _timeout_ms = options
        .get("timeoutMs")
        .and_then(|v| v.as_u64())
        .unwrap_or(30_000)
        .min(u64::from(u32::MAX)) as u32;

    let messages = options
        .get("messages")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let mut sess = spear_wasm::ChatSession::create().map_err(|e| e.to_string())?;

    for m in messages {
        let role = m
            .get("role")
            .and_then(|v| v.as_str())
            .unwrap_or("user");
        let content = m
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        sess.write_message(role, content).map_err(|e| e.to_string())?;
    }

    let model_param = serde_json::json!({"key":"model","value": model}).to_string();
    sess.set_param_json(&model_param)
        .map_err(|e| e.to_string())?;

    if let Some(b) = backend {
        let backend_param = serde_json::json!({"key":"backend","value": b}).to_string();
        sess.set_param_json(&backend_param)
            .map_err(|e| e.to_string())?;
    }

    let tools = options.get("tools").and_then(|v| v.as_array()).cloned();
    let mut flags = 0;
    if let Some(tools) = tools {
        if !tools.is_empty() {
            let (arena_ptr, arena_len) = tool_arena_ptr_len();
            let arena_ptr_param = serde_json::json!({"key":"tool_arena_ptr","value": arena_ptr}).to_string();
            let arena_len_param = serde_json::json!({"key":"tool_arena_len","value": arena_len}).to_string();
            sess.set_param_json(&arena_ptr_param)
                .map_err(|e| e.to_string())?;
            sess.set_param_json(&arena_len_param)
                .map_err(|e| e.to_string())?;

            let max_total_tool_calls = options
                .get("maxTotalToolCalls")
                .and_then(|v| v.as_u64())
                .unwrap_or(4)
                .min(u64::from(u32::MAX)) as u32;
            let max_iterations = options
                .get("maxIterations")
                .and_then(|v| v.as_u64())
                .unwrap_or(4)
                .min(u64::from(u32::MAX)) as u32;
            sess.set_param_json(&serde_json::json!({"key":"max_total_tool_calls","value": max_total_tool_calls}).to_string())
                .map_err(|e| e.to_string())?;
            sess.set_param_json(&serde_json::json!({"key":"max_iterations","value": max_iterations}).to_string())
                .map_err(|e| e.to_string())?;

            for t in tools {
                let fn_offset = t
                    .get("fnOffset")
                    .and_then(|v| v.as_i64())
                    .ok_or_else(|| "tool missing fnOffset".to_string())? as i32;
                let fn_json = t
                    .get("fnJson")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "tool missing fnJson".to_string())?;
                sess.write_fn(fn_offset, fn_json)
                    .map_err(|e| e.to_string())?;
            }

            flags |= spear_wasm::constants::SPEAR_CCHAT_SEND_FLAG_AUTO_TOOL_CALL;
        }
    }

    let resp_fd = sess.send(flags).map_err(|e| e.to_string())?;

    let resp_bytes = spear_wasm::cchat_recv_alloc(resp_fd).map_err(|e| e.to_string())?;
    // Ensure UTF-8; fallback to lossy.
    // 确保 UTF-8；否则使用 lossy。
    let resp = String::from_utf8(resp_bytes.clone()).unwrap_or_else(|_| {
        String::from_utf8_lossy(&resp_bytes).into_owned()
    });

    let _ = spear_wasm::cchat_close(resp_fd);
    let _ = sess.close();
    Ok(resp)
}

// M1 keeps module evaluation in the runtime binary.
// M1 的模块执行流程放在 runtime 二进制中实现。

#[cfg(test)]
mod tests {
    use super::*;
    use boa_engine::builtins::promise::PromiseState;
    use boa_engine::object::builtins::JsPromise;
    use boa_engine::{js_string, JsValue};

    fn run_entry(code: &str) -> (JsValue, Context) {
        let loader = Rc::new(BuiltinModuleRegistry::new());
        let mut ctx = build_context(loader.clone()).expect("build ctx");
        init_tool_runtime(&mut ctx, DEFAULT_TOOL_SLOTS);
        install_native_bindings(&mut ctx);

        let module = Module::parse(Source::from_bytes(code.as_bytes()), None, &mut ctx)
            .expect("parse entry");
        loader.register_module(js_string!("app"), module.clone());

        let lifecycle = module.load_link_evaluate(&mut ctx);
        for _ in 0..32 {
            if matches!(lifecycle.state(), PromiseState::Pending) {
                ctx.run_jobs();
            } else {
                break;
            }
        }

        match lifecycle.state() {
            PromiseState::Fulfilled(_) => {}
            PromiseState::Rejected(e) => return (e, ctx),
            PromiseState::Pending => return (JsValue::undefined(), ctx),
        }

        let ns = module.namespace(&mut ctx);
        let default_export = ns
            .get(js_string!("default"), &mut ctx)
            .expect("get default export");
        let func = default_export
            .as_callable()
            .cloned()
            .expect("default export callable");

        let out = func
            .call(&JsValue::undefined(), &[], &mut ctx)
            .expect("call default");
        ctx.run_jobs();

        let settled = if out.is_promise() {
            let p = JsPromise::from_object(out.as_object().cloned().unwrap()).unwrap();
            for _ in 0..32 {
                if matches!(p.state(), PromiseState::Pending) {
                    ctx.run_jobs();
                } else {
                    break;
                }
            }
            match p.state() {
                PromiseState::Fulfilled(v) | PromiseState::Rejected(v) => v,
                PromiseState::Pending => JsValue::undefined(),
            }
        } else {
            out
        };

        (settled, ctx)
    }

    fn run_entry_to_string(code: &str) -> String {
        let (v, mut ctx) = run_entry(code);
        v.to_string(&mut ctx)
            .unwrap()
            .to_std_string_escaped()
    }

    #[test]
    fn test_import_spear_shape() {
        let s = run_entry_to_string(
            r#"
import { Spear } from "spear";
export default async function main() {
  return typeof Spear?.chat?.completions?.create === "function"
    && typeof Spear?.userStream?.open === "function"
    && typeof Spear?.userStream?.ctlOpen === "function";
}
"#,
        );
        assert_eq!(s, "true", "unexpected result: {s}");
    }

    #[test]
    fn test_create_rejects_on_non_wasm_target() {
        let s = run_entry_to_string(
            r#"
import { Spear } from "spear";
export default async function main() {
  try {
    await Spear.chat.completions.create({ model: "x", messages: [] });
    return "unexpected";
  } catch (e) {
    return String(e);
  }
}
"#,
        );
        assert!(
            s.contains("unsupported_target") || s.contains("ENOSYS") || s.contains("cchat_create"),
            "unexpected error: {s}"
        );
    }

    #[test]
    fn test_tool_registration_is_json_serializable() {
        let s = run_entry_to_string(
            r#"
import { Spear } from "spear";
export default async function main() {
  const t = Spear.tool({
    name: "sum",
    description: "Add two integers",
    parameters: {
      type: "object",
      properties: { a: { type: "integer" }, b: { type: "integer" } },
      required: ["a", "b"],
    },
    handler: ({ a, b }) => ({ sum: a + b }),
  });
  return JSON.stringify(t);
}
"#,
        );
        assert!(s.contains("fnOffset"), "unexpected json: {s}");
        assert!(s.contains("fnJson"), "unexpected json: {s}");
        assert!(s.contains("sum"), "unexpected json: {s}");
    }
}
