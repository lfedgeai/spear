use crate::spearlet::execution::host_api::errno::{
    SPEAR_EBADF, SPEAR_EFAULT, SPEAR_EINVAL, SPEAR_EIO, SPEAR_ENOSPC, SPEAR_OK,
};
use crate::spearlet::execution::host_api::{DefaultHostApi, SpearHostApi};
use crate::spearlet::execution::runtime::{ResourcePoolConfig, RuntimeConfig};
use crate::spearlet::execution::ExecutionError;
use crate::spearlet::execution::RuntimeType;
use crate::spearlet::mcp::task_subset::McpTaskPolicy;
use crate::spearlet::param_keys::chat as chat_keys;
use std::time::SystemTime;
use tracing::debug;
use wasmedge_sdk::{
    error::CoreError, AsInstance, CallingFrame, ImportObject, ImportObjectBuilder, Instance,
    ValType, WasmValue,
};
use wasmedge_sys::ffi;
use wasmedge_sys::instance::function::AsFunc;
use wasmedge_sys::{Executor, Function};

// Helper function to extract DefaultHostApi from host data
fn _unused() {}

const SPEAR_ERR_INVALID_FD: i32 = -SPEAR_EBADF;
const SPEAR_ERR_INVALID_PTR: i32 = -SPEAR_EFAULT;
const SPEAR_ERR_BUFFER_TOO_SMALL: i32 = -SPEAR_ENOSPC;
const SPEAR_ERR_INVALID_CMD: i32 = -SPEAR_EINVAL;
const SPEAR_ERR_INTERNAL: i32 = -SPEAR_EIO;

const SPEAR_LOG_MAX_BYTES: i32 = 16 * 1024;

const CTL_SET_PARAM: i32 = 1;
const CTL_GET_METRICS: i32 = 2;

fn guard_termination(host_data: &DefaultHostApi) -> Result<(), CoreError> {
    if let Some(s) = host_data.check_wasm_termination() {
        let msg = match (s.scope, s.message.as_deref()) {
            (
                crate::spearlet::execution::host_api::termination::TerminationScope::Execution,
                Some(m),
            ) => {
                format!("Terminate execution: {}", m)
            }
            (
                crate::spearlet::execution::host_api::termination::TerminationScope::Instance,
                Some(m),
            ) => {
                format!("Destroy instance: {}", m)
            }
            (
                crate::spearlet::execution::host_api::termination::TerminationScope::Execution,
                None,
            ) => "Terminate execution".to_string(),
            (
                crate::spearlet::execution::host_api::termination::TerminationScope::Instance,
                None,
            ) => "Destroy instance".to_string(),
        };
        crate::spearlet::debug_reporter::report_termination(
            "wasm_hostcalls.rs:guard_termination",
            &s,
            format!("[DEBUG] termination observed: {}", msg),
        );
        host_data.wasm_log_write("warn", &msg);
        return Err(CoreError::Common(
            wasmedge_sdk::error::CoreCommonError::UserDefError,
        ));
    }
    Ok(())
}

macro_rules! guarded {
    ($f:ident) => {
        |host_data: &mut DefaultHostApi,
         instance: &mut Instance,
         frame: &mut CallingFrame,
         input: Vec<WasmValue>|
         -> Result<Vec<WasmValue>, CoreError> {
            guard_termination(host_data)?;
            $f(host_data, instance, frame, input)
        }
    };
}

fn get_i32_arg(input: &[WasmValue], idx: usize) -> Option<i32> {
    input.get(idx).map(|v| v.to_i32())
}

fn mem_read(instance: &mut Instance, ptr: i32, len: i32) -> Result<Vec<u8>, i32> {
    if ptr < 0 || len < 0 {
        return Err(SPEAR_ERR_INVALID_PTR);
    }
    let mem = instance
        .get_memory_ref("memory")
        .map_err(|_| SPEAR_ERR_INVALID_PTR)?;
    mem.get_data(ptr as u32, len as u32)
        .map_err(|_| SPEAR_ERR_INVALID_PTR)
}

fn mem_write(instance: &mut Instance, ptr: i32, data: &[u8]) -> Result<(), i32> {
    if ptr < 0 {
        return Err(SPEAR_ERR_INVALID_PTR);
    }
    let mut mem = instance
        .get_memory_mut("memory")
        .map_err(|_| SPEAR_ERR_INVALID_PTR)?;
    mem.set_data(data, ptr as u32)
        .map_err(|_| SPEAR_ERR_INVALID_PTR)
}

fn mem_read_u32(instance: &mut Instance, ptr: i32) -> Result<u32, i32> {
    let bytes = mem_read(instance, ptr, 4)?;
    Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn mem_write_u32(instance: &mut Instance, ptr: i32, v: u32) -> Result<(), i32> {
    mem_write(instance, ptr, &v.to_le_bytes())
}

fn mem_write_with_len(
    instance: &mut Instance,
    out_ptr: i32,
    out_len_ptr: i32,
    payload: &[u8],
) -> i32 {
    let max_len = match mem_read_u32(instance, out_len_ptr) {
        Ok(v) => v as usize,
        Err(e) => return e,
    };
    let need = payload.len();
    if max_len < need {
        let _ = mem_write_u32(instance, out_len_ptr, need as u32);
        return SPEAR_ERR_BUFFER_TOO_SMALL;
    }
    if mem_write(instance, out_ptr, payload).is_err() {
        return SPEAR_ERR_INVALID_PTR;
    }
    if mem_write_u32(instance, out_len_ptr, need as u32).is_err() {
        return SPEAR_ERR_INVALID_PTR;
    }
    need as i32
}

fn choose_func_table_name(instance: &Instance) -> Option<String> {
    let names = instance.table_names()?;
    if names.is_empty() {
        return None;
    }
    if names.iter().any(|x| x == "__indirect_function_table") {
        return Some("__indirect_function_table".to_string());
    }
    if names.iter().any(|x| x == "table") {
        return Some("table".to_string());
    }
    Some(names[0].clone())
}

fn debug_call_wasm_tool_by_offset(instance: &mut Instance, fn_offset: i32) {
    if !tracing::enabled!(tracing::Level::DEBUG) {
        return;
    }

    if fn_offset < 0 {
        debug!(
            fn_offset,
            "cchat_write_fn: skip calling tool (negative offset)"
        );
        return;
    }

    let Some(table_name) = choose_func_table_name(instance) else {
        debug!(fn_offset, "cchat_write_fn: no exported table found");
        return;
    };

    let table = match instance.get_table(&table_name) {
        Ok(t) => t,
        Err(e) => {
            debug!(fn_offset, table_name, err = %e, "cchat_write_fn: get_table failed");
            return;
        }
    };

    let idx = fn_offset as u32;
    let v = match table.get_data(idx) {
        Ok(v) => v,
        Err(e) => {
            debug!(fn_offset, table_name, err = %e, "cchat_write_fn: table.get_data failed");
            return;
        }
    };

    if v.is_null_ref() {
        debug!(fn_offset, table_name, "cchat_write_fn: funcref is null");
        return;
    }

    if v.ty() != ValType::FuncRef {
        debug!(fn_offset, table_name, ty = ?v.ty(), "cchat_write_fn: table entry is not funcref");
        return;
    }

    let raw_func_ctx = unsafe { ffi::WasmEdge_ValueGetFuncRef(v.as_raw()) };
    if raw_func_ctx.is_null() {
        debug!(
            fn_offset,
            table_name, "cchat_write_fn: WasmEdge_ValueGetFuncRef returned null"
        );
        return;
    }

    let func = std::mem::ManuallyDrop::new(unsafe { Function::from_raw(raw_func_ctx as _) });

    let ty = func.ty();
    debug!(fn_offset, table_name, ty = ?ty, "cchat_write_fn: resolved tool function type");

    let Some(ty) = ty else {
        debug!(
            fn_offset,
            table_name, "cchat_write_fn: tool function type unavailable"
        );
        return;
    };

    let param_len = ty.args_len();
    let ret_len = ty.returns_len();
    if !(param_len == 4 && ret_len == 1 && ty.args() == [ValType::I32; 4]) {
        debug!(
            fn_offset,
            table_name,
            param_len,
            ret_len,
            args = ?ty.args(),
            returns = ?ty.returns(),
            "cchat_write_fn: skip calling tool due to signature mismatch"
        );
        return;
    }

    let _ = func;
}

pub fn cchat_create(
    host_data: &mut DefaultHostApi,
    _instance: &mut Instance,
    _frame: &mut CallingFrame,
    input: Vec<WasmValue>,
) -> Result<Vec<WasmValue>, CoreError> {
    if !input.is_empty() {
        return Ok(vec![WasmValue::from_i32(SPEAR_ERR_INTERNAL)]);
    }
    Ok(vec![WasmValue::from_i32(host_data.cchat_create())])
}

pub fn cchat_write_msg(
    host_data: &mut DefaultHostApi,
    instance: &mut Instance,
    _frame: &mut CallingFrame,
    input: Vec<WasmValue>,
) -> Result<Vec<WasmValue>, CoreError> {
    if input.len() != 5 {
        return Ok(vec![WasmValue::from_i32(SPEAR_ERR_INTERNAL)]);
    }

    let fd = get_i32_arg(&input, 0).unwrap_or(SPEAR_ERR_INVALID_FD);
    let role_ptr = get_i32_arg(&input, 1).unwrap_or(-1);
    let role_len = get_i32_arg(&input, 2).unwrap_or(-1);
    let content_ptr = get_i32_arg(&input, 3).unwrap_or(-1);
    let content_len = get_i32_arg(&input, 4).unwrap_or(-1);

    let role = match mem_read(instance, role_ptr, role_len) {
        Ok(b) => String::from_utf8_lossy(&b).to_string(),
        Err(e) => return Ok(vec![WasmValue::from_i32(e)]),
    };
    let content = match mem_read(instance, content_ptr, content_len) {
        Ok(b) => String::from_utf8_lossy(&b).to_string(),
        Err(e) => return Ok(vec![WasmValue::from_i32(e)]),
    };

    let rc = host_data.cchat_write_msg(fd, role, content);
    Ok(vec![WasmValue::from_i32(rc)])
}

pub fn cchat_write_fn(
    host_data: &mut DefaultHostApi,
    instance: &mut Instance,
    _frame: &mut CallingFrame,
    input: Vec<WasmValue>,
) -> Result<Vec<WasmValue>, CoreError> {
    if input.len() != 4 {
        return Ok(vec![WasmValue::from_i32(SPEAR_ERR_INTERNAL)]);
    }

    let fd = get_i32_arg(&input, 0).unwrap_or(SPEAR_ERR_INVALID_FD);
    let fn_offset = get_i32_arg(&input, 1).unwrap_or(-1);
    let fn_ptr = get_i32_arg(&input, 2).unwrap_or(-1);
    let fn_len = get_i32_arg(&input, 3).unwrap_or(-1);

    let fn_json = match mem_read(instance, fn_ptr, fn_len) {
        Ok(b) => String::from_utf8_lossy(&b).to_string(),
        Err(e) => return Ok(vec![WasmValue::from_i32(e)]),
    };

    debug!(fd, fn_offset, fn_len, "cchat_write_fn: register tool");
    debug_call_wasm_tool_by_offset(instance, fn_offset);

    let rc = host_data.cchat_write_fn(fd, fn_offset, fn_json);
    Ok(vec![WasmValue::from_i32(rc)])
}

pub fn cchat_ctl(
    host_data: &mut DefaultHostApi,
    instance: &mut Instance,
    _frame: &mut CallingFrame,
    input: Vec<WasmValue>,
) -> Result<Vec<WasmValue>, CoreError> {
    if input.len() != 4 {
        return Ok(vec![WasmValue::from_i32(SPEAR_ERR_INTERNAL)]);
    }

    let fd = get_i32_arg(&input, 0).unwrap_or(SPEAR_ERR_INVALID_FD);
    let cmd = get_i32_arg(&input, 1).unwrap_or(SPEAR_ERR_INVALID_CMD);
    let arg_ptr = get_i32_arg(&input, 2).unwrap_or(-1);
    let arg_len_ptr = get_i32_arg(&input, 3).unwrap_or(-1);

    match cmd {
        CTL_SET_PARAM => {
            let arg_len = match mem_read_u32(instance, arg_len_ptr) {
                Ok(v) => v as i32,
                Err(e) => return Ok(vec![WasmValue::from_i32(e)]),
            };
            let bytes = match mem_read(instance, arg_ptr, arg_len) {
                Ok(b) => b,
                Err(e) => return Ok(vec![WasmValue::from_i32(e)]),
            };
            let v: serde_json::Value = match serde_json::from_slice(&bytes) {
                Ok(v) => v,
                Err(_) => return Ok(vec![WasmValue::from_i32(SPEAR_ERR_INTERNAL)]),
            };
            let key = v.get("key").and_then(|x| x.as_str()).unwrap_or("");
            let value = v.get("value").cloned().unwrap_or(serde_json::Value::Null);
            if key.is_empty() {
                return Ok(vec![WasmValue::from_i32(SPEAR_ERR_INTERNAL)]);
            }
            let rc = host_data.cchat_ctl_set_param(fd, key.to_string(), value);
            Ok(vec![WasmValue::from_i32(rc)])
        }
        CTL_GET_METRICS => {
            let payload = match host_data.cchat_ctl_get_metrics(fd) {
                Ok(b) => b,
                Err(e) => return Ok(vec![WasmValue::from_i32(e)]),
            };
            let wrote = mem_write_with_len(instance, arg_ptr, arg_len_ptr, &payload);
            Ok(vec![WasmValue::from_i32(wrote)])
        }
        _ => Ok(vec![WasmValue::from_i32(SPEAR_ERR_INVALID_CMD)]),
    }
}

pub fn cchat_send(
    host_data: &mut DefaultHostApi,
    instance: &mut Instance,
    _frame: &mut CallingFrame,
    input: Vec<WasmValue>,
) -> Result<Vec<WasmValue>, CoreError> {
    if input.len() != 2 {
        return Ok(vec![WasmValue::from_i32(SPEAR_ERR_INTERNAL)]);
    }

    let fd = get_i32_arg(&input, 0).unwrap_or(SPEAR_ERR_INVALID_FD);
    let flags = get_i32_arg(&input, 1).unwrap_or(0);

    const AUTO_TOOL_CALL: i32 = 2;
    if (flags & AUTO_TOOL_CALL) == 0 {
        match host_data.cchat_send(fd, flags) {
            Ok(resp_fd) => Ok(vec![WasmValue::from_i32(resp_fd)]),
            Err(e) => Ok(vec![WasmValue::from_i32(e)]),
        }
    } else {
        let snapshot = match host_data.cchat_snapshot(fd) {
            Ok(s) => s,
            Err(e) => return Ok(vec![WasmValue::from_i32(e)]),
        };
        let arena_ptr = snapshot
            .params
            .get(chat_keys::TOOL_ARENA_PTR)
            .and_then(|v| v.as_i64())
            .unwrap_or(0) as i32;
        let arena_len = snapshot
            .params
            .get(chat_keys::TOOL_ARENA_LEN)
            .and_then(|v| v.as_i64())
            .unwrap_or(0) as i32;

        let max_tool_output_bytes = snapshot
            .params
            .get(chat_keys::MAX_TOOL_OUTPUT_BYTES)
            .and_then(|v| v.as_u64())
            .unwrap_or(64 * 1024)
            .min(1024 * 1024) as usize;

        let mut tool_exec = |fn_offset: i32, args_json: &str| -> Result<String, i32> {
            call_wasm_tool_with_arena(
                instance,
                fn_offset,
                args_json,
                arena_ptr,
                arena_len,
                max_tool_output_bytes,
            )
        };

        match host_data.cchat_send_with_tools(fd, flags, &mut tool_exec) {
            Ok(resp_fd) => Ok(vec![WasmValue::from_i32(resp_fd)]),
            Err(e) => Ok(vec![WasmValue::from_i32(e)]),
        }
    }
}

fn call_wasm_tool_with_arena(
    instance: &mut Instance,
    fn_offset: i32,
    args_json: &str,
    arena_ptr: i32,
    arena_len: i32,
    max_tool_output_bytes: usize,
) -> Result<String, i32> {
    if arena_ptr <= 0 || arena_len <= 0 {
        return Err(SPEAR_ERR_INVALID_PTR);
    }
    if fn_offset < 0 {
        return Err(SPEAR_ERR_INVALID_PTR);
    }

    let args_bytes = args_json.as_bytes();
    let args_len = args_bytes.len();

    let base = arena_ptr as i64;
    let arena_len_i64 = arena_len as i64;
    let args_ptr = base;
    let out_ptr = args_ptr + align_up(args_len as i64 + 1, 8);
    let mut out_cap = (arena_len_i64 - (out_ptr - base) - 8).max(0) as usize;
    out_cap = out_cap.min(max_tool_output_bytes);
    let out_len_ptr = out_ptr + align_up(out_cap as i64, 8);

    if out_cap == 0 {
        return Err(SPEAR_ERR_BUFFER_TOO_SMALL);
    }

    if out_len_ptr + 4 > base + arena_len_i64 {
        return Err(SPEAR_ERR_BUFFER_TOO_SMALL);
    }

    mem_write(instance, args_ptr as i32, args_bytes)?;
    mem_write(instance, (args_ptr as i32) + args_len as i32, &[0])?;

    let mut attempt: u32 = 0;
    loop {
        attempt += 1;
        if attempt > 2 {
            return Err(SPEAR_ERR_INTERNAL);
        }

        mem_write_u32(instance, out_len_ptr as i32, out_cap as u32)?;

        let rc = call_wasm_tool_by_offset(
            instance,
            fn_offset,
            args_ptr as i32,
            args_len as i32,
            out_ptr as i32,
            out_len_ptr as i32,
        )?;

        if rc == 0 {
            let wrote = mem_read_u32(instance, out_len_ptr as i32)? as usize;
            if wrote > out_cap {
                return Err(SPEAR_ERR_INTERNAL);
            }
            let out = mem_read(instance, out_ptr as i32, wrote as i32)?;
            let s = String::from_utf8(out).map_err(|_| SPEAR_ERR_INTERNAL)?;
            return Ok(s);
        }

        if rc == SPEAR_ERR_BUFFER_TOO_SMALL {
            let need = mem_read_u32(instance, out_len_ptr as i32)? as usize;
            if need > max_tool_output_bytes {
                return Err(SPEAR_ERR_BUFFER_TOO_SMALL);
            }
            let max_fit = (base + arena_len_i64 - out_ptr - 8).max(0) as usize;
            if need > max_fit {
                return Err(SPEAR_ERR_BUFFER_TOO_SMALL);
            }
            out_cap = need;
            continue;
        }

        return Err(rc);
    }
}

fn call_wasm_tool_by_offset(
    instance: &Instance,
    fn_offset: i32,
    args_ptr: i32,
    args_len: i32,
    out_ptr: i32,
    out_len_ptr: i32,
) -> Result<i32, i32> {
    let Some(table_name) = choose_func_table_name(instance) else {
        return Err(SPEAR_ERR_INTERNAL);
    };

    let table = instance
        .get_table(&table_name)
        .map_err(|_| SPEAR_ERR_INTERNAL)?;
    let func_ref = table
        .get_data(fn_offset as u32)
        .map_err(|_| SPEAR_ERR_INTERNAL)?;
    if func_ref.is_null_ref() {
        return Err(SPEAR_ERR_INTERNAL);
    }
    if func_ref.ty() != ValType::FuncRef {
        return Err(SPEAR_ERR_INTERNAL);
    }

    let func_ref_ctx = unsafe { ffi::WasmEdge_ValueGetFuncRef(func_ref.as_raw()) };
    if func_ref_ctx.is_null() {
        return Err(SPEAR_ERR_INTERNAL);
    }

    let mut func =
        std::mem::ManuallyDrop::new(unsafe { Function::from_raw(func_ref_ctx as *mut _) });

    let ty = func.ty();
    let Some(ty) = ty else {
        return Err(SPEAR_ERR_INTERNAL);
    };
    let params = ty.args();
    let rets = ty.returns();
    if params.len() != 4
        || rets.len() != 1
        || params[0] != ValType::I32
        || params[1] != ValType::I32
        || params[2] != ValType::I32
        || params[3] != ValType::I32
        || rets[0] != ValType::I32
    {
        return Err(SPEAR_ERR_INTERNAL);
    }

    let mut executor = Executor::create(None, None).map_err(|_| SPEAR_ERR_INTERNAL)?;
    let args = [
        WasmValue::from_i32(args_ptr),
        WasmValue::from_i32(args_len),
        WasmValue::from_i32(out_ptr),
        WasmValue::from_i32(out_len_ptr),
    ];
    let returns = executor
        .call_func(&mut func, args)
        .map_err(|_| SPEAR_ERR_INTERNAL)?;
    Ok(returns
        .get(0)
        .map(|v| v.to_i32())
        .unwrap_or(SPEAR_ERR_INTERNAL))
}

fn align_up(v: i64, align: i64) -> i64 {
    if align <= 1 {
        return v;
    }
    (v + align - 1) & !(align - 1)
}

pub fn cchat_recv(
    host_data: &mut DefaultHostApi,
    instance: &mut Instance,
    _frame: &mut CallingFrame,
    input: Vec<WasmValue>,
) -> Result<Vec<WasmValue>, CoreError> {
    if input.len() != 3 {
        return Ok(vec![WasmValue::from_i32(SPEAR_ERR_INTERNAL)]);
    }

    let response_fd = get_i32_arg(&input, 0).unwrap_or(SPEAR_ERR_INVALID_FD);
    let out_ptr = get_i32_arg(&input, 1).unwrap_or(-1);
    let out_len_ptr = get_i32_arg(&input, 2).unwrap_or(-1);

    let payload = match host_data.cchat_recv(response_fd) {
        Ok(b) => b,
        Err(e) => return Ok(vec![WasmValue::from_i32(e)]),
    };
    let wrote = mem_write_with_len(instance, out_ptr, out_len_ptr, &payload);
    Ok(vec![WasmValue::from_i32(wrote)])
}

pub fn cchat_close(
    host_data: &mut DefaultHostApi,
    _instance: &mut Instance,
    _frame: &mut CallingFrame,
    input: Vec<WasmValue>,
) -> Result<Vec<WasmValue>, CoreError> {
    if input.len() != 1 {
        return Ok(vec![WasmValue::from_i32(SPEAR_ERR_INTERNAL)]);
    }
    let fd = get_i32_arg(&input, 0).unwrap_or(SPEAR_ERR_INVALID_FD);
    Ok(vec![WasmValue::from_i32(host_data.cchat_close(fd))])
}

pub fn rtasr_create(
    host_data: &mut DefaultHostApi,
    _instance: &mut Instance,
    _frame: &mut CallingFrame,
    input: Vec<WasmValue>,
) -> Result<Vec<WasmValue>, CoreError> {
    if !input.is_empty() {
        return Ok(vec![WasmValue::from_i32(SPEAR_ERR_INTERNAL)]);
    }
    Ok(vec![WasmValue::from_i32(host_data.rtasr_create())])
}

pub fn rtasr_ctl(
    host_data: &mut DefaultHostApi,
    instance: &mut Instance,
    _frame: &mut CallingFrame,
    input: Vec<WasmValue>,
) -> Result<Vec<WasmValue>, CoreError> {
    if input.len() != 4 {
        return Ok(vec![WasmValue::from_i32(SPEAR_ERR_INTERNAL)]);
    }

    let fd = get_i32_arg(&input, 0).unwrap_or(SPEAR_ERR_INVALID_FD);
    let cmd = get_i32_arg(&input, 1).unwrap_or(SPEAR_ERR_INVALID_CMD);
    let arg_ptr = get_i32_arg(&input, 2).unwrap_or(-1);
    let arg_len_ptr = get_i32_arg(&input, 3).unwrap_or(-1);

    let arg_len = match mem_read_u32(instance, arg_len_ptr) {
        Ok(v) => v as i32,
        Err(e) => return Ok(vec![WasmValue::from_i32(e)]),
    };

    let payload_bytes = if arg_len > 0 {
        match mem_read(instance, arg_ptr, arg_len) {
            Ok(b) => Some(b),
            Err(e) => return Ok(vec![WasmValue::from_i32(e)]),
        }
    } else {
        None
    };

    let out = match payload_bytes {
        Some(b) => host_data.rtasr_ctl(fd, cmd, Some(&b)),
        None => host_data.rtasr_ctl(fd, cmd, None),
    };

    match out {
        Ok(Some(resp)) => {
            let wrote = mem_write_with_len(instance, arg_ptr, arg_len_ptr, &resp);
            Ok(vec![WasmValue::from_i32(wrote)])
        }
        Ok(None) => Ok(vec![WasmValue::from_i32(SPEAR_OK)]),
        Err(e) => Ok(vec![WasmValue::from_i32(e)]),
    }
}

pub fn rtasr_write(
    host_data: &mut DefaultHostApi,
    instance: &mut Instance,
    _frame: &mut CallingFrame,
    input: Vec<WasmValue>,
) -> Result<Vec<WasmValue>, CoreError> {
    if input.len() != 3 {
        return Ok(vec![WasmValue::from_i32(SPEAR_ERR_INTERNAL)]);
    }

    let fd = get_i32_arg(&input, 0).unwrap_or(SPEAR_ERR_INVALID_FD);
    let buf_ptr = get_i32_arg(&input, 1).unwrap_or(-1);
    let buf_len = get_i32_arg(&input, 2).unwrap_or(-1);

    let bytes = match mem_read(instance, buf_ptr, buf_len) {
        Ok(b) => b,
        Err(e) => return Ok(vec![WasmValue::from_i32(e)]),
    };
    let rc = host_data.rtasr_write(fd, &bytes);
    Ok(vec![WasmValue::from_i32(rc)])
}

pub fn rtasr_read(
    host_data: &mut DefaultHostApi,
    instance: &mut Instance,
    _frame: &mut CallingFrame,
    input: Vec<WasmValue>,
) -> Result<Vec<WasmValue>, CoreError> {
    if input.len() != 3 {
        return Ok(vec![WasmValue::from_i32(SPEAR_ERR_INTERNAL)]);
    }

    let fd = get_i32_arg(&input, 0).unwrap_or(SPEAR_ERR_INVALID_FD);
    let out_ptr = get_i32_arg(&input, 1).unwrap_or(-1);
    let out_len_ptr = get_i32_arg(&input, 2).unwrap_or(-1);

    let payload = match host_data.rtasr_read(fd) {
        Ok(b) => b,
        Err(e) => return Ok(vec![WasmValue::from_i32(e)]),
    };
    let wrote = mem_write_with_len(instance, out_ptr, out_len_ptr, &payload);
    Ok(vec![WasmValue::from_i32(wrote)])
}

pub fn rtasr_close(
    host_data: &mut DefaultHostApi,
    _instance: &mut Instance,
    _frame: &mut CallingFrame,
    input: Vec<WasmValue>,
) -> Result<Vec<WasmValue>, CoreError> {
    if input.len() != 1 {
        return Ok(vec![WasmValue::from_i32(SPEAR_ERR_INTERNAL)]);
    }
    let fd = get_i32_arg(&input, 0).unwrap_or(SPEAR_ERR_INVALID_FD);
    Ok(vec![WasmValue::from_i32(host_data.rtasr_close(fd))])
}

pub fn mic_create(
    host_data: &mut DefaultHostApi,
    _instance: &mut Instance,
    _frame: &mut CallingFrame,
    input: Vec<WasmValue>,
) -> Result<Vec<WasmValue>, CoreError> {
    if !input.is_empty() {
        return Ok(vec![WasmValue::from_i32(SPEAR_ERR_INTERNAL)]);
    }
    Ok(vec![WasmValue::from_i32(host_data.mic_create())])
}

pub fn mic_ctl(
    host_data: &mut DefaultHostApi,
    instance: &mut Instance,
    _frame: &mut CallingFrame,
    input: Vec<WasmValue>,
) -> Result<Vec<WasmValue>, CoreError> {
    if input.len() != 4 {
        return Ok(vec![WasmValue::from_i32(SPEAR_ERR_INTERNAL)]);
    }

    let fd = get_i32_arg(&input, 0).unwrap_or(SPEAR_ERR_INVALID_FD);
    let cmd = get_i32_arg(&input, 1).unwrap_or(SPEAR_ERR_INVALID_CMD);
    let arg_ptr = get_i32_arg(&input, 2).unwrap_or(-1);
    let arg_len_ptr = get_i32_arg(&input, 3).unwrap_or(-1);

    let arg_len = match mem_read_u32(instance, arg_len_ptr) {
        Ok(v) => v as i32,
        Err(e) => return Ok(vec![WasmValue::from_i32(e)]),
    };
    let payload_bytes = if arg_len > 0 {
        match mem_read(instance, arg_ptr, arg_len) {
            Ok(b) => Some(b),
            Err(e) => return Ok(vec![WasmValue::from_i32(e)]),
        }
    } else {
        None
    };

    let out = match payload_bytes {
        Some(b) => host_data.mic_ctl(fd, cmd, Some(&b)),
        None => host_data.mic_ctl(fd, cmd, None),
    };

    match out {
        Ok(Some(resp)) => {
            let wrote = mem_write_with_len(instance, arg_ptr, arg_len_ptr, &resp);
            Ok(vec![WasmValue::from_i32(wrote)])
        }
        Ok(None) => Ok(vec![WasmValue::from_i32(SPEAR_OK)]),
        Err(e) => Ok(vec![WasmValue::from_i32(e)]),
    }
}

pub fn mic_read(
    host_data: &mut DefaultHostApi,
    instance: &mut Instance,
    _frame: &mut CallingFrame,
    input: Vec<WasmValue>,
) -> Result<Vec<WasmValue>, CoreError> {
    if input.len() != 3 {
        return Ok(vec![WasmValue::from_i32(SPEAR_ERR_INTERNAL)]);
    }

    let fd = get_i32_arg(&input, 0).unwrap_or(SPEAR_ERR_INVALID_FD);
    let out_ptr = get_i32_arg(&input, 1).unwrap_or(-1);
    let out_len_ptr = get_i32_arg(&input, 2).unwrap_or(-1);

    let payload = match host_data.mic_read(fd) {
        Ok(b) => b,
        Err(e) => return Ok(vec![WasmValue::from_i32(e)]),
    };
    let wrote = mem_write_with_len(instance, out_ptr, out_len_ptr, &payload);
    Ok(vec![WasmValue::from_i32(wrote)])
}

pub fn mic_close(
    host_data: &mut DefaultHostApi,
    _instance: &mut Instance,
    _frame: &mut CallingFrame,
    input: Vec<WasmValue>,
) -> Result<Vec<WasmValue>, CoreError> {
    if input.len() != 1 {
        return Ok(vec![WasmValue::from_i32(SPEAR_ERR_INTERNAL)]);
    }
    let fd = get_i32_arg(&input, 0).unwrap_or(SPEAR_ERR_INVALID_FD);
    Ok(vec![WasmValue::from_i32(host_data.mic_close(fd))])
}

pub fn user_stream_open(
    host_data: &mut DefaultHostApi,
    _instance: &mut Instance,
    _frame: &mut CallingFrame,
    input: Vec<WasmValue>,
) -> Result<Vec<WasmValue>, CoreError> {
    if input.len() != 2 {
        return Ok(vec![WasmValue::from_i32(SPEAR_ERR_INTERNAL)]);
    }
    let stream_id = get_i32_arg(&input, 0).unwrap_or(0);
    let direction = get_i32_arg(&input, 1).unwrap_or(0);
    Ok(vec![WasmValue::from_i32(
        host_data.user_stream_open(stream_id, direction),
    )])
}

pub fn user_stream_read(
    host_data: &mut DefaultHostApi,
    instance: &mut Instance,
    _frame: &mut CallingFrame,
    input: Vec<WasmValue>,
) -> Result<Vec<WasmValue>, CoreError> {
    if input.len() != 3 {
        return Ok(vec![WasmValue::from_i32(SPEAR_ERR_INTERNAL)]);
    }

    let fd = get_i32_arg(&input, 0).unwrap_or(SPEAR_ERR_INVALID_FD);
    let out_ptr = get_i32_arg(&input, 1).unwrap_or(-1);
    let out_len_ptr = get_i32_arg(&input, 2).unwrap_or(-1);

    let payload = match host_data.user_stream_read(fd) {
        Ok(b) => b,
        Err(e) => return Ok(vec![WasmValue::from_i32(e)]),
    };
    let wrote = mem_write_with_len(instance, out_ptr, out_len_ptr, &payload);
    Ok(vec![WasmValue::from_i32(wrote)])
}

pub fn user_stream_write(
    host_data: &mut DefaultHostApi,
    instance: &mut Instance,
    _frame: &mut CallingFrame,
    input: Vec<WasmValue>,
) -> Result<Vec<WasmValue>, CoreError> {
    if input.len() != 3 {
        return Ok(vec![WasmValue::from_i32(SPEAR_ERR_INTERNAL)]);
    }
    let fd = get_i32_arg(&input, 0).unwrap_or(SPEAR_ERR_INVALID_FD);
    let buf_ptr = get_i32_arg(&input, 1).unwrap_or(-1);
    let buf_len = get_i32_arg(&input, 2).unwrap_or(-1);

    let bytes = match mem_read(instance, buf_ptr, buf_len) {
        Ok(b) => b,
        Err(e) => return Ok(vec![WasmValue::from_i32(e)]),
    };
    let rc = host_data.user_stream_write(fd, &bytes);
    Ok(vec![WasmValue::from_i32(rc)])
}

pub fn user_stream_close(
    host_data: &mut DefaultHostApi,
    _instance: &mut Instance,
    _frame: &mut CallingFrame,
    input: Vec<WasmValue>,
) -> Result<Vec<WasmValue>, CoreError> {
    if input.len() != 1 {
        return Ok(vec![WasmValue::from_i32(SPEAR_ERR_INTERNAL)]);
    }
    let fd = get_i32_arg(&input, 0).unwrap_or(SPEAR_ERR_INVALID_FD);
    Ok(vec![WasmValue::from_i32(host_data.user_stream_close(fd))])
}

pub fn user_stream_ctl_open(
    host_data: &mut DefaultHostApi,
    _instance: &mut Instance,
    _frame: &mut CallingFrame,
    input: Vec<WasmValue>,
) -> Result<Vec<WasmValue>, CoreError> {
    if !input.is_empty() {
        return Ok(vec![WasmValue::from_i32(SPEAR_ERR_INTERNAL)]);
    }
    Ok(vec![WasmValue::from_i32(host_data.user_stream_ctl_open())])
}

pub fn user_stream_ctl_read(
    host_data: &mut DefaultHostApi,
    instance: &mut Instance,
    _frame: &mut CallingFrame,
    input: Vec<WasmValue>,
) -> Result<Vec<WasmValue>, CoreError> {
    if input.len() != 3 {
        return Ok(vec![WasmValue::from_i32(SPEAR_ERR_INTERNAL)]);
    }

    let fd = get_i32_arg(&input, 0).unwrap_or(SPEAR_ERR_INVALID_FD);
    let out_ptr = get_i32_arg(&input, 1).unwrap_or(-1);
    let out_len_ptr = get_i32_arg(&input, 2).unwrap_or(-1);

    let payload = match host_data.user_stream_ctl_read(fd) {
        Ok(b) => b,
        Err(e) => return Ok(vec![WasmValue::from_i32(e)]),
    };
    let wrote = mem_write_with_len(instance, out_ptr, out_len_ptr, &payload);
    Ok(vec![WasmValue::from_i32(wrote)])
}

pub fn spear_ep_create(
    host_data: &mut DefaultHostApi,
    _instance: &mut Instance,
    _frame: &mut CallingFrame,
    input: Vec<WasmValue>,
) -> Result<Vec<WasmValue>, CoreError> {
    if !input.is_empty() {
        return Ok(vec![WasmValue::from_i32(SPEAR_ERR_INTERNAL)]);
    }
    Ok(vec![WasmValue::from_i32(host_data.spear_ep_create())])
}

pub fn spear_ep_ctl(
    host_data: &mut DefaultHostApi,
    _instance: &mut Instance,
    _frame: &mut CallingFrame,
    input: Vec<WasmValue>,
) -> Result<Vec<WasmValue>, CoreError> {
    if input.len() != 4 {
        return Ok(vec![WasmValue::from_i32(SPEAR_ERR_INTERNAL)]);
    }

    let epfd = get_i32_arg(&input, 0).unwrap_or(SPEAR_ERR_INVALID_FD);
    let op = get_i32_arg(&input, 1).unwrap_or(SPEAR_ERR_INVALID_CMD);
    let fd = get_i32_arg(&input, 2).unwrap_or(SPEAR_ERR_INVALID_FD);
    let events = get_i32_arg(&input, 3).unwrap_or(0);
    Ok(vec![WasmValue::from_i32(
        host_data.spear_ep_ctl(epfd, op, fd, events),
    )])
}

pub fn spear_ep_wait(
    host_data: &mut DefaultHostApi,
    instance: &mut Instance,
    _frame: &mut CallingFrame,
    input: Vec<WasmValue>,
) -> Result<Vec<WasmValue>, CoreError> {
    if input.len() != 4 {
        return Ok(vec![WasmValue::from_i32(SPEAR_ERR_INTERNAL)]);
    }

    let epfd = get_i32_arg(&input, 0).unwrap_or(SPEAR_ERR_INVALID_FD);
    let out_ptr = get_i32_arg(&input, 1).unwrap_or(-1);
    let out_len_ptr = get_i32_arg(&input, 2).unwrap_or(-1);
    let timeout_ms = get_i32_arg(&input, 3).unwrap_or(0);

    let cap = match mem_read_u32(instance, out_len_ptr) {
        Ok(v) => v as usize,
        Err(e) => return Ok(vec![WasmValue::from_i32(e)]),
    };
    let max_records = cap / 8;
    if max_records == 0 {
        let _ = mem_write_u32(instance, out_len_ptr, 8);
        return Ok(vec![WasmValue::from_i32(SPEAR_ERR_BUFFER_TOO_SMALL)]);
    }

    let ready = match host_data.spear_ep_wait_ready(epfd, timeout_ms) {
        Ok(v) => v,
        Err(e) => return Ok(vec![WasmValue::from_i32(e)]),
    };
    if ready.is_empty() {
        let _ = mem_write_u32(instance, out_len_ptr, 0);
        return Ok(vec![WasmValue::from_i32(0)]);
    }

    let n = std::cmp::min(ready.len(), max_records);
    let mut payload = Vec::with_capacity(n * 8);
    for (fd, events) in ready.into_iter().take(n) {
        payload.extend_from_slice(&fd.to_le_bytes());
        payload.extend_from_slice(&events.to_le_bytes());
    }

    if mem_write(instance, out_ptr, &payload).is_err() {
        return Ok(vec![WasmValue::from_i32(SPEAR_ERR_INVALID_PTR)]);
    }
    let _ = mem_write_u32(instance, out_len_ptr, payload.len() as u32);
    Ok(vec![WasmValue::from_i32(n as i32)])
}

pub fn spear_ep_close(
    host_data: &mut DefaultHostApi,
    _instance: &mut Instance,
    _frame: &mut CallingFrame,
    input: Vec<WasmValue>,
) -> Result<Vec<WasmValue>, CoreError> {
    if input.len() != 1 {
        return Ok(vec![WasmValue::from_i32(SPEAR_ERR_INTERNAL)]);
    }
    let epfd = get_i32_arg(&input, 0).unwrap_or(SPEAR_ERR_INVALID_FD);
    Ok(vec![WasmValue::from_i32(host_data.spear_ep_close(epfd))])
}

pub fn spear_fd_ctl(
    host_data: &mut DefaultHostApi,
    instance: &mut Instance,
    _frame: &mut CallingFrame,
    input: Vec<WasmValue>,
) -> Result<Vec<WasmValue>, CoreError> {
    if input.len() != 4 {
        return Ok(vec![WasmValue::from_i32(SPEAR_ERR_INTERNAL)]);
    }
    let fd = get_i32_arg(&input, 0).unwrap_or(SPEAR_ERR_INVALID_FD);
    let cmd = get_i32_arg(&input, 1).unwrap_or(SPEAR_ERR_INVALID_CMD);
    let arg_ptr = get_i32_arg(&input, 2).unwrap_or(-1);
    let arg_len_ptr = get_i32_arg(&input, 3).unwrap_or(-1);

    match cmd {
        crate::spearlet::execution::hostcall::fd_table::FD_CTL_SET_FLAGS => {
            let arg_len = match mem_read_u32(instance, arg_len_ptr) {
                Ok(v) => v as i32,
                Err(e) => return Ok(vec![WasmValue::from_i32(e)]),
            };
            let bytes = match mem_read(instance, arg_ptr, arg_len) {
                Ok(b) => b,
                Err(e) => return Ok(vec![WasmValue::from_i32(e)]),
            };
            match host_data.spear_fd_ctl(fd, cmd, Some(&bytes)) {
                Ok(_) => Ok(vec![WasmValue::from_i32(SPEAR_OK)]),
                Err(e) => Ok(vec![WasmValue::from_i32(e)]),
            }
        }
        crate::spearlet::execution::hostcall::fd_table::FD_CTL_GET_FLAGS
        | crate::spearlet::execution::hostcall::fd_table::FD_CTL_GET_KIND
        | crate::spearlet::execution::hostcall::fd_table::FD_CTL_GET_STATUS
        | crate::spearlet::execution::hostcall::fd_table::FD_CTL_GET_METRICS => {
            let payload = match host_data.spear_fd_ctl(fd, cmd, None) {
                Ok(Some(b)) => b,
                Ok(None) => b"{}".to_vec(),
                Err(e) => return Ok(vec![WasmValue::from_i32(e)]),
            };
            let wrote = mem_write_with_len(instance, arg_ptr, arg_len_ptr, &payload);
            Ok(vec![WasmValue::from_i32(wrote)])
        }
        _ => Ok(vec![WasmValue::from_i32(SPEAR_ERR_INVALID_CMD)]),
    }
}

pub fn spear_time_now_ms(
    host_data: &mut DefaultHostApi,
    _instance: &mut Instance,
    _frame: &mut CallingFrame,
    _input: Vec<WasmValue>,
) -> Result<Vec<WasmValue>, CoreError> {
    let v = host_data.time_now_ms() as i64;
    Ok(vec![WasmValue::from_i64(v)])
}

pub fn spear_wall_time_s(
    _host_data: &mut DefaultHostApi,
    _instance: &mut Instance,
    _frame: &mut CallingFrame,
    _input: Vec<WasmValue>,
) -> Result<Vec<WasmValue>, CoreError> {
    let v = SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    Ok(vec![WasmValue::from_i64(v)])
}

pub fn spear_sleep_ms(
    _host_data: &mut DefaultHostApi,
    _instance: &mut Instance,
    _frame: &mut CallingFrame,
    input: Vec<WasmValue>,
) -> Result<Vec<WasmValue>, CoreError> {
    if input.len() != 1 {
        return Err(CoreError::Common(
            wasmedge_sdk::error::CoreCommonError::UserDefError,
        ));
    }
    let ms = input[0].to_i32() as u64;
    std::thread::sleep(std::time::Duration::from_millis(ms));
    Ok(vec![])
}

pub fn spear_random_i64(
    _host_data: &mut DefaultHostApi,
    _instance: &mut Instance,
    _frame: &mut CallingFrame,
    _input: Vec<WasmValue>,
) -> Result<Vec<WasmValue>, CoreError> {
    let acc = rand::random::<i64>();
    Ok(vec![WasmValue::from_i64(acc)])
}

pub fn spear_log(
    host_data: &mut DefaultHostApi,
    instance: &mut Instance,
    _frame: &mut CallingFrame,
    input: Vec<WasmValue>,
) -> Result<Vec<WasmValue>, CoreError> {
    if input.len() != 3 {
        return Ok(vec![WasmValue::from_i32(SPEAR_ERR_INTERNAL)]);
    }
    let level = get_i32_arg(&input, 0).unwrap_or(2);
    let ptr = get_i32_arg(&input, 1).unwrap_or(-1);
    let len = get_i32_arg(&input, 2).unwrap_or(-1);

    if len < 0 || len > SPEAR_LOG_MAX_BYTES {
        return Ok(vec![WasmValue::from_i32(SPEAR_ERR_INVALID_CMD)]);
    }

    let bytes = match mem_read(instance, ptr, len) {
        Ok(b) => b,
        Err(e) => return Ok(vec![WasmValue::from_i32(e)]),
    };
    let msg = String::from_utf8_lossy(&bytes).to_string();

    let level_str = match level {
        0 => "trace",
        1 => "debug",
        2 => "info",
        3 => "warn",
        4 => "error",
        _ => "info",
    };
    host_data.wasm_log_write(level_str, &msg);
    Ok(vec![WasmValue::from_i32(SPEAR_OK)])
}

pub fn build_spear_import() -> Result<ImportObject<DefaultHostApi>, ExecutionError> {
    let api = DefaultHostApi::new(RuntimeConfig {
        runtime_type: RuntimeType::Wasm,
        settings: std::collections::HashMap::new(),
        global_environment: std::collections::HashMap::new(),
        spearlet_config: None,
        resource_pool: ResourcePoolConfig::default(),
    });
    let mut builder =
        ImportObjectBuilder::new("spear", api).map_err(|e| ExecutionError::RuntimeError {
            message: format!("create import builder error: {}", e),
        })?;

    builder
        .with_func::<(), i64>("time_now_ms", guarded!(spear_time_now_ms))
        .map_err(|e| ExecutionError::RuntimeError {
            message: format!("add time_now_ms function error: {}", e),
        })?;
    builder
        .with_func::<(), i64>("wall_time_s", guarded!(spear_wall_time_s))
        .map_err(|e| ExecutionError::RuntimeError {
            message: format!("add wall_time_s function error: {}", e),
        })?;
    builder
        .with_func::<(), i64>("random_i64", guarded!(spear_random_i64))
        .map_err(|e| ExecutionError::RuntimeError {
            message: format!("add random_i64 function error: {}", e),
        })?;
    builder
        .with_func::<(i32, i32, i32), i32>("log", guarded!(spear_log))
        .map_err(|e| ExecutionError::RuntimeError {
            message: format!("add log function error: {}", e),
        })?;
    builder
        .with_func::<i32, ()>("sleep_ms", guarded!(spear_sleep_ms))
        .map_err(|e| ExecutionError::RuntimeError {
            message: format!("add sleep_ms function error: {}", e),
        })?;

    builder
        .with_func::<(), i32>("cchat_create", guarded!(cchat_create))
        .map_err(|e| ExecutionError::RuntimeError {
            message: format!("add cchat_create function error: {}", e),
        })?;
    builder
        .with_func::<(i32, i32, i32, i32, i32), i32>("cchat_write_msg", guarded!(cchat_write_msg))
        .map_err(|e| ExecutionError::RuntimeError {
            message: format!("add cchat_write_msg function error: {}", e),
        })?;
    builder
        .with_func::<(i32, i32, i32, i32), i32>("cchat_write_fn", guarded!(cchat_write_fn))
        .map_err(|e| ExecutionError::RuntimeError {
            message: format!("add cchat_write_fn function error: {}", e),
        })?;
    builder
        .with_func::<(i32, i32, i32, i32), i32>("cchat_ctl", guarded!(cchat_ctl))
        .map_err(|e| ExecutionError::RuntimeError {
            message: format!("add cchat_ctl function error: {}", e),
        })?;
    builder
        .with_func::<(i32, i32), i32>("cchat_send", guarded!(cchat_send))
        .map_err(|e| ExecutionError::RuntimeError {
            message: format!("add cchat_send function error: {}", e),
        })?;
    builder
        .with_func::<(i32, i32, i32), i32>("cchat_recv", guarded!(cchat_recv))
        .map_err(|e| ExecutionError::RuntimeError {
            message: format!("add cchat_recv function error: {}", e),
        })?;
    builder
        .with_func::<i32, i32>("cchat_close", guarded!(cchat_close))
        .map_err(|e| ExecutionError::RuntimeError {
            message: format!("add cchat_close function error: {}", e),
        })?;

    builder
        .with_func::<(), i32>("rtasr_create", guarded!(rtasr_create))
        .map_err(|e| ExecutionError::RuntimeError {
            message: format!("add rtasr_create function error: {}", e),
        })?;
    builder
        .with_func::<(i32, i32, i32, i32), i32>("rtasr_ctl", guarded!(rtasr_ctl))
        .map_err(|e| ExecutionError::RuntimeError {
            message: format!("add rtasr_ctl function error: {}", e),
        })?;
    builder
        .with_func::<(i32, i32, i32), i32>("rtasr_write", guarded!(rtasr_write))
        .map_err(|e| ExecutionError::RuntimeError {
            message: format!("add rtasr_write function error: {}", e),
        })?;
    builder
        .with_func::<(i32, i32, i32), i32>("rtasr_read", guarded!(rtasr_read))
        .map_err(|e| ExecutionError::RuntimeError {
            message: format!("add rtasr_read function error: {}", e),
        })?;
    builder
        .with_func::<i32, i32>("rtasr_close", guarded!(rtasr_close))
        .map_err(|e| ExecutionError::RuntimeError {
            message: format!("add rtasr_close function error: {}", e),
        })?;

    builder
        .with_func::<(), i32>("mic_create", guarded!(mic_create))
        .map_err(|e| ExecutionError::RuntimeError {
            message: format!("add mic_create function error: {}", e),
        })?;
    builder
        .with_func::<(i32, i32, i32, i32), i32>("mic_ctl", guarded!(mic_ctl))
        .map_err(|e| ExecutionError::RuntimeError {
            message: format!("add mic_ctl function error: {}", e),
        })?;
    builder
        .with_func::<(i32, i32, i32), i32>("mic_read", guarded!(mic_read))
        .map_err(|e| ExecutionError::RuntimeError {
            message: format!("add mic_read function error: {}", e),
        })?;
    builder
        .with_func::<i32, i32>("mic_close", guarded!(mic_close))
        .map_err(|e| ExecutionError::RuntimeError {
            message: format!("add mic_close function error: {}", e),
        })?;

    builder
        .with_func::<(i32, i32), i32>("user_stream_open", guarded!(user_stream_open))
        .map_err(|e| ExecutionError::RuntimeError {
            message: format!("add user_stream_open function error: {}", e),
        })?;
    builder
        .with_func::<(i32, i32, i32), i32>("user_stream_read", guarded!(user_stream_read))
        .map_err(|e| ExecutionError::RuntimeError {
            message: format!("add user_stream_read function error: {}", e),
        })?;
    builder
        .with_func::<(i32, i32, i32), i32>("user_stream_write", guarded!(user_stream_write))
        .map_err(|e| ExecutionError::RuntimeError {
            message: format!("add user_stream_write function error: {}", e),
        })?;
    builder
        .with_func::<i32, i32>("user_stream_close", guarded!(user_stream_close))
        .map_err(|e| ExecutionError::RuntimeError {
            message: format!("add user_stream_close function error: {}", e),
        })?;

    builder
        .with_func::<(), i32>("user_stream_ctl_open", guarded!(user_stream_ctl_open))
        .map_err(|e| ExecutionError::RuntimeError {
            message: format!("add user_stream_ctl_open function error: {}", e),
        })?;
    builder
        .with_func::<(i32, i32, i32), i32>("user_stream_ctl_read", guarded!(user_stream_ctl_read))
        .map_err(|e| ExecutionError::RuntimeError {
            message: format!("add user_stream_ctl_read function error: {}", e),
        })?;

    builder
        .with_func::<(), i32>("spear_epoll_create", guarded!(spear_ep_create))
        .map_err(|e| ExecutionError::RuntimeError {
            message: format!("add spear_epoll_create function error: {}", e),
        })?;
    builder
        .with_func::<(i32, i32, i32, i32), i32>("spear_epoll_ctl", guarded!(spear_ep_ctl))
        .map_err(|e| ExecutionError::RuntimeError {
            message: format!("add spear_epoll_ctl function error: {}", e),
        })?;
    builder
        .with_func::<(i32, i32, i32, i32), i32>("spear_epoll_wait", guarded!(spear_ep_wait))
        .map_err(|e| ExecutionError::RuntimeError {
            message: format!("add spear_epoll_wait function error: {}", e),
        })?;
    builder
        .with_func::<i32, i32>("spear_epoll_close", guarded!(spear_ep_close))
        .map_err(|e| ExecutionError::RuntimeError {
            message: format!("add spear_epoll_close function error: {}", e),
        })?;
    builder
        .with_func::<(i32, i32, i32, i32), i32>("spear_fd_ctl", guarded!(spear_fd_ctl))
        .map_err(|e| ExecutionError::RuntimeError {
            message: format!("add spear_fd_ctl function error: {}", e),
        })?;

    let import = builder.build();
    Ok(import)
}

pub fn build_spear_import_with_api(
    runtime_config: RuntimeConfig,
    task_id: String,
    mcp_task_policy: std::sync::Arc<McpTaskPolicy>,
    instance_id: String,
) -> Result<ImportObject<DefaultHostApi>, ExecutionError> {
    let api = DefaultHostApi::new(runtime_config)
        .with_task_policy(task_id, mcp_task_policy)
        .with_instance_id(instance_id);
    let mut builder =
        ImportObjectBuilder::new("spear", api).map_err(|e| ExecutionError::RuntimeError {
            message: format!("create import builder error: {}", e),
        })?;

    builder
        .with_func::<(), i64>("time_now_ms", spear_time_now_ms)
        .map_err(|e| ExecutionError::RuntimeError {
            message: format!("add time_now_ms function error: {}", e),
        })?;
    builder
        .with_func::<(), i64>("wall_time_s", spear_wall_time_s)
        .map_err(|e| ExecutionError::RuntimeError {
            message: format!("add wall_time_s function error: {}", e),
        })?;
    builder
        .with_func::<(), i64>("random_i64", spear_random_i64)
        .map_err(|e| ExecutionError::RuntimeError {
            message: format!("add random_i64 function error: {}", e),
        })?;
    builder
        .with_func::<(i32, i32, i32), i32>("log", spear_log)
        .map_err(|e| ExecutionError::RuntimeError {
            message: format!("add log function error: {}", e),
        })?;
    builder
        .with_func::<i32, ()>("sleep_ms", spear_sleep_ms)
        .map_err(|e| ExecutionError::RuntimeError {
            message: format!("add sleep_ms function error: {}", e),
        })?;

    builder
        .with_func::<(), i32>("cchat_create", cchat_create)
        .map_err(|e| ExecutionError::RuntimeError {
            message: format!("add cchat_create function error: {}", e),
        })?;
    builder
        .with_func::<(i32, i32, i32, i32, i32), i32>("cchat_write_msg", cchat_write_msg)
        .map_err(|e| ExecutionError::RuntimeError {
            message: format!("add cchat_write_msg function error: {}", e),
        })?;
    builder
        .with_func::<(i32, i32, i32, i32), i32>("cchat_write_fn", cchat_write_fn)
        .map_err(|e| ExecutionError::RuntimeError {
            message: format!("add cchat_write_fn function error: {}", e),
        })?;
    builder
        .with_func::<(i32, i32, i32, i32), i32>("cchat_ctl", cchat_ctl)
        .map_err(|e| ExecutionError::RuntimeError {
            message: format!("add cchat_ctl function error: {}", e),
        })?;
    builder
        .with_func::<(i32, i32), i32>("cchat_send", cchat_send)
        .map_err(|e| ExecutionError::RuntimeError {
            message: format!("add cchat_send function error: {}", e),
        })?;
    builder
        .with_func::<(i32, i32, i32), i32>("cchat_recv", cchat_recv)
        .map_err(|e| ExecutionError::RuntimeError {
            message: format!("add cchat_recv function error: {}", e),
        })?;
    builder
        .with_func::<i32, i32>("cchat_close", guarded!(cchat_close))
        .map_err(|e| ExecutionError::RuntimeError {
            message: format!("add cchat_close function error: {}", e),
        })?;

    builder
        .with_func::<(), i32>("rtasr_create", guarded!(rtasr_create))
        .map_err(|e| ExecutionError::RuntimeError {
            message: format!("add rtasr_create function error: {}", e),
        })?;
    builder
        .with_func::<(i32, i32, i32, i32), i32>("rtasr_ctl", guarded!(rtasr_ctl))
        .map_err(|e| ExecutionError::RuntimeError {
            message: format!("add rtasr_ctl function error: {}", e),
        })?;
    builder
        .with_func::<(i32, i32, i32), i32>("rtasr_write", guarded!(rtasr_write))
        .map_err(|e| ExecutionError::RuntimeError {
            message: format!("add rtasr_write function error: {}", e),
        })?;
    builder
        .with_func::<(i32, i32, i32), i32>("rtasr_read", guarded!(rtasr_read))
        .map_err(|e| ExecutionError::RuntimeError {
            message: format!("add rtasr_read function error: {}", e),
        })?;
    builder
        .with_func::<i32, i32>("rtasr_close", guarded!(rtasr_close))
        .map_err(|e| ExecutionError::RuntimeError {
            message: format!("add rtasr_close function error: {}", e),
        })?;

    builder
        .with_func::<(), i32>("mic_create", guarded!(mic_create))
        .map_err(|e| ExecutionError::RuntimeError {
            message: format!("add mic_create function error: {}", e),
        })?;
    builder
        .with_func::<(i32, i32, i32, i32), i32>("mic_ctl", guarded!(mic_ctl))
        .map_err(|e| ExecutionError::RuntimeError {
            message: format!("add mic_ctl function error: {}", e),
        })?;
    builder
        .with_func::<(i32, i32, i32), i32>("mic_read", guarded!(mic_read))
        .map_err(|e| ExecutionError::RuntimeError {
            message: format!("add mic_read function error: {}", e),
        })?;
    builder
        .with_func::<i32, i32>("mic_close", guarded!(mic_close))
        .map_err(|e| ExecutionError::RuntimeError {
            message: format!("add mic_close function error: {}", e),
        })?;

    builder
        .with_func::<(i32, i32), i32>("user_stream_open", guarded!(user_stream_open))
        .map_err(|e| ExecutionError::RuntimeError {
            message: format!("add user_stream_open function error: {}", e),
        })?;
    builder
        .with_func::<(i32, i32, i32), i32>("user_stream_read", guarded!(user_stream_read))
        .map_err(|e| ExecutionError::RuntimeError {
            message: format!("add user_stream_read function error: {}", e),
        })?;
    builder
        .with_func::<(i32, i32, i32), i32>("user_stream_write", guarded!(user_stream_write))
        .map_err(|e| ExecutionError::RuntimeError {
            message: format!("add user_stream_write function error: {}", e),
        })?;
    builder
        .with_func::<i32, i32>("user_stream_close", guarded!(user_stream_close))
        .map_err(|e| ExecutionError::RuntimeError {
            message: format!("add user_stream_close function error: {}", e),
        })?;

    builder
        .with_func::<(), i32>("user_stream_ctl_open", guarded!(user_stream_ctl_open))
        .map_err(|e| ExecutionError::RuntimeError {
            message: format!("add user_stream_ctl_open function error: {}", e),
        })?;
    builder
        .with_func::<(i32, i32, i32), i32>("user_stream_ctl_read", guarded!(user_stream_ctl_read))
        .map_err(|e| ExecutionError::RuntimeError {
            message: format!("add user_stream_ctl_read function error: {}", e),
        })?;

    builder
        .with_func::<(), i32>("spear_epoll_create", guarded!(spear_ep_create))
        .map_err(|e| ExecutionError::RuntimeError {
            message: format!("add spear_epoll_create function error: {}", e),
        })?;
    builder
        .with_func::<(i32, i32, i32, i32), i32>("spear_epoll_ctl", guarded!(spear_ep_ctl))
        .map_err(|e| ExecutionError::RuntimeError {
            message: format!("add spear_epoll_ctl function error: {}", e),
        })?;
    builder
        .with_func::<(i32, i32, i32, i32), i32>("spear_epoll_wait", guarded!(spear_ep_wait))
        .map_err(|e| ExecutionError::RuntimeError {
            message: format!("add spear_epoll_wait function error: {}", e),
        })?;
    builder
        .with_func::<i32, i32>("spear_epoll_close", guarded!(spear_ep_close))
        .map_err(|e| ExecutionError::RuntimeError {
            message: format!("add spear_epoll_close function error: {}", e),
        })?;
    builder
        .with_func::<(i32, i32, i32, i32), i32>("spear_fd_ctl", guarded!(spear_fd_ctl))
        .map_err(|e| ExecutionError::RuntimeError {
            message: format!("add spear_fd_ctl function error: {}", e),
        })?;

    let import = builder.build();
    Ok(import)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "wasmedge")]
    #[test]
    fn test_call_cchat_write_fn() {
        static INIT: std::sync::Once = std::sync::Once::new();
        INIT.call_once(|| {
            let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("spear_next=debug"));
            let _ = tracing_subscriber::fmt()
                .with_env_filter(env_filter)
                .with_test_writer()
                .try_init();
        });

        use std::collections::HashMap;
        use wasmedge_sdk::config::{CommonConfigOptions, ConfigBuilder};
        use wasmedge_sdk::wasi::WasiModule;
        use wasmedge_sdk::{params, vm::SyncInst, wat2wasm, Module, Store, Vm};

        let wat = r#"(module
            (type $create_t (func (result i32)))
            (type $write_fn_t (func (param i32 i32 i32 i32) (result i32)))
            (import "spear" "cchat_create" (func $cchat_create (type $create_t)))
            (import "spear" "cchat_write_fn" (func $cchat_write_fn (type $write_fn_t)))

            (import "wasi_snapshot_preview1" "fd_write" (func $fd_write (param i32 i32 i32 i32) (result i32)))

            (memory (export "memory") 1)
            (data (i32.const 0) "{}")
            (data (i32.const 32) "tool_called\n")

            (table (export "table") 1 funcref)

            (func $tool (param i32 i32 i32 i32) (result i32)
                (i32.store (i32.const 64) (i32.const 32))
                (i32.store (i32.const 68) (i32.const 12))
                (drop (call $fd_write (i32.const 1) (i32.const 64) (i32.const 1) (i32.const 72)))
                i32.const 0
            )
            (elem (i32.const 0) $tool)

            (func (export "run") (result i32)
                (local $fd i32)
                (local.set $fd (call $cchat_create))
                (call $cchat_write_fn
                    (local.get $fd)
                    (i32.const 0)
                    (i32.const 0)
                    (i32.const 2)
                )
            )
        )"#;

        let bytes = wat2wasm(wat.as_bytes()).unwrap();
        let c = ConfigBuilder::new(CommonConfigOptions::default())
            .build()
            .unwrap();
        let mut imports: HashMap<String, &mut dyn SyncInst> = HashMap::new();
        let mut wasi_module = WasiModule::create(None, None, None).unwrap();
        imports.insert(wasi_module.name().to_string(), wasi_module.as_mut());

        let spear_import = build_spear_import().unwrap();
        let spear_static = Box::leak(Box::new(spear_import));
        imports.insert("spear".to_string(), spear_static);

        let store = Store::new(Some(&c), imports).unwrap();
        let mut vm = Vm::new(store);
        let module = Module::from_bytes(None, bytes).unwrap();
        vm.register_module(None, module).unwrap();

        let out = vm.run_func(None, "run", params!()).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].to_i32(), 0);
    }

    #[cfg(feature = "wasmedge")]
    #[test]
    fn test_call_log_and_buffer() {
        static INIT: std::sync::Once = std::sync::Once::new();
        INIT.call_once(|| {
            let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("spear_next=debug"));
            let _ = tracing_subscriber::fmt()
                .with_env_filter(env_filter)
                .with_test_writer()
                .try_init();
        });

        use crate::spearlet::execution::host_api::{
            clear_wasm_logs_by_execution, get_wasm_logs_by_execution, set_current_wasm_execution_id,
        };
        use std::collections::HashMap;
        use std::sync::Arc;
        use wasmedge_sdk::config::{CommonConfigOptions, ConfigBuilder};
        use wasmedge_sdk::wasi::WasiModule;
        use wasmedge_sdk::{params, vm::SyncInst, wat2wasm, Module, Store, Vm};

        let instance_id = "inst-test-log".to_string();
        let task_id = "task-test-log".to_string();
        let execution_id = "exec-test-log".to_string();
        clear_wasm_logs_by_execution(&execution_id);

        let wat = r#"(module
            (type $log_t (func (param i32 i32 i32) (result i32)))
            (import "spear" "log" (func $log (type $log_t)))
            (memory (export "memory") 1)
            (data (i32.const 0) "hello")

            (func (export "run") (result i32)
                (call $log (i32.const 2) (i32.const 0) (i32.const 5))
            )
        )"#;

        let bytes = wat2wasm(wat.as_bytes()).unwrap();
        let c = ConfigBuilder::new(CommonConfigOptions::default())
            .build()
            .unwrap();
        let mut imports: HashMap<String, &mut dyn SyncInst> = HashMap::new();
        let mut wasi_module = WasiModule::create(None, None, None).unwrap();
        imports.insert(wasi_module.name().to_string(), wasi_module.as_mut());

        let runtime_config = RuntimeConfig {
            runtime_type: RuntimeType::Wasm,
            settings: std::collections::HashMap::new(),
            global_environment: std::collections::HashMap::new(),
            spearlet_config: None,
            resource_pool: ResourcePoolConfig::default(),
        };
        let spear_import = build_spear_import_with_api(
            runtime_config,
            task_id,
            Arc::new(McpTaskPolicy::default()),
            instance_id.clone(),
        )
        .unwrap();
        let spear_static = Box::leak(Box::new(spear_import));
        imports.insert("spear".to_string(), spear_static);

        let store = Store::new(Some(&c), imports).unwrap();
        let mut vm = Vm::new(store);
        let module = Module::from_bytes(None, bytes).unwrap();
        vm.register_module(None, module).unwrap();

        set_current_wasm_execution_id(Some(execution_id.clone()));
        let out = vm.run_func(None, "run", params!()).unwrap();
        set_current_wasm_execution_id(None);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].to_i32(), 0);

        let logs = get_wasm_logs_by_execution(&execution_id, None, 10);
        assert!(logs.iter().any(|e| e.message.contains("hello")));
    }
}
