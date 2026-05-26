//! Low-level Spear WASM hostcall bindings (ABI only)
//! Spear WASM Hostcall 低层绑定（仅 ABI）
//!
//! This crate exposes `extern "C"` functions imported from the `spear` WASM import module.
//! 本 crate 暴露从 `spear` WASM import module 导入的 `extern "C"` 函数。
//!
//! Safety / 安全性：
//! - These functions are unsafe because they operate on raw pointers in the WASM linear memory.
//! - 这些函数是不安全的，因为它们会操作 WASM 线性内存中的原始指针。

#![deny(unsafe_op_in_unsafe_fn)]

pub mod constants {
    //! Hostcall constants mirrored from the C SDK
    //! 与 C SDK 对齐的 hostcall 常量

    pub const SPEAR_EPERM: i32 = 1;
    pub const SPEAR_ENOENT: i32 = 2;
    pub const SPEAR_EIO: i32 = 5;
    pub const SPEAR_EBADF: i32 = 9;
    pub const SPEAR_EAGAIN: i32 = 11;
    pub const SPEAR_ENOMEM: i32 = 12;
    pub const SPEAR_EFAULT: i32 = 14;
    pub const SPEAR_EINVAL: i32 = 22;
    pub const SPEAR_ENOSPC: i32 = 28;
    pub const SPEAR_EPIPE: i32 = 32;
    pub const SPEAR_ENOSYS: i32 = 38;
    pub const SPEAR_ECONNRESET: i32 = 104;
    pub const SPEAR_ENOTCONN: i32 = 107;
    pub const SPEAR_ETIMEDOUT: i32 = 110;

    pub const SPEAR_CCHAT_CTL_SET_PARAM: i32 = 1;
    pub const SPEAR_CCHAT_CTL_GET_METRICS: i32 = 2;

    pub const SPEAR_CCHAT_SEND_FLAG_ENABLE_METRICS: i32 = 1 << 0;
    pub const SPEAR_CCHAT_SEND_FLAG_AUTO_TOOL_CALL: i32 = 1 << 1;

    pub const SPEAR_RTA_CTL_SET_PARAM: i32 = 1;
    pub const SPEAR_RTA_CTL_CONNECT: i32 = 2;
    pub const SPEAR_RTA_CTL_GET_STATUS: i32 = 3;
    pub const SPEAR_RTA_CTL_SEND_EVENT: i32 = 4;
    pub const SPEAR_RTA_CTL_FLUSH: i32 = 5;
    pub const SPEAR_RTA_CTL_CLEAR: i32 = 6;
    pub const SPEAR_RTA_CTL_SET_AUTOFLUSH: i32 = 7;
    pub const SPEAR_RTA_CTL_GET_AUTOFLUSH: i32 = 8;

    pub const SPEAR_MIC_CTL_SET_PARAM: i32 = 1;
    pub const SPEAR_MIC_CTL_GET_STATUS: i32 = 2;

    pub const SPEAR_EPOLL_CTL_ADD: i32 = 1;
    pub const SPEAR_EPOLL_CTL_MOD: i32 = 2;
    pub const SPEAR_EPOLL_CTL_DEL: i32 = 3;

    pub const SPEAR_EPOLLIN: i32 = 0x001;
    pub const SPEAR_EPOLLOUT: i32 = 0x004;
    pub const SPEAR_EPOLLERR: i32 = 0x008;
    pub const SPEAR_EPOLLHUP: i32 = 0x010;

    pub const SPEAR_FD_CTL_SET_FLAGS: i32 = 1;
    pub const SPEAR_FD_CTL_GET_FLAGS: i32 = 2;
    pub const SPEAR_FD_CTL_GET_KIND: i32 = 3;
    pub const SPEAR_FD_CTL_GET_STATUS: i32 = 4;
    pub const SPEAR_FD_CTL_GET_METRICS: i32 = 5;

    pub const SPEAR_USER_STREAM_DIR_INBOUND: i32 = 1;
    pub const SPEAR_USER_STREAM_DIR_OUTBOUND: i32 = 2;
    pub const SPEAR_USER_STREAM_DIR_BIDIRECTIONAL: i32 = 3;

    pub const SPEAR_USER_STREAM_CTL_EVENT_STREAM_CONNECTED: i32 = 1;
    pub const SPEAR_USER_STREAM_CTL_EVENT_SESSION_CLOSED: i32 = 2;

    // SSF (Spear Stream Frame) v1 msg_type values.
    // SSF（Spear Stream Frame）v1 的 msg_type 定义。
    pub const SPEAR_SSF_MSG_TYPE_CTRL: i32 = 1;
    pub const SPEAR_SSF_MSG_TYPE_DATA: i32 = 2;
    pub const SPEAR_SSF_MSG_TYPE_COMMIT: i32 = 3;
}

#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "spear")]
extern "C" {
    pub fn time_now_ms() -> i64;
    pub fn wall_time_s() -> i64;
    pub fn sleep_ms(ms: i32);
    pub fn random_i64() -> i64;

    pub fn log(level: i32, msg_ptr: i32, msg_len: i32) -> i32;

    pub fn cchat_create() -> i32;
    pub fn cchat_write_msg(fd: i32, role_ptr: i32, role_len: i32, content_ptr: i32, content_len: i32) -> i32;
    pub fn cchat_write_fn(fd: i32, fn_offset: i32, fn_ptr: i32, fn_len: i32) -> i32;
    pub fn cchat_ctl(fd: i32, cmd: i32, arg_ptr: i32, arg_len_ptr: i32) -> i32;
    pub fn cchat_send(fd: i32, flags: i32) -> i32;
    pub fn cchat_recv(response_fd: i32, out_ptr: i32, out_len_ptr: i32) -> i32;
    pub fn cchat_close(fd: i32) -> i32;

    pub fn rtasr_create() -> i32;
    pub fn rtasr_ctl(fd: i32, cmd: i32, arg_ptr: i32, arg_len_ptr: i32) -> i32;
    pub fn rtasr_write(fd: i32, buf_ptr: i32, buf_len: i32) -> i32;
    pub fn rtasr_read(fd: i32, out_ptr: i32, out_len_ptr: i32) -> i32;
    pub fn rtasr_close(fd: i32) -> i32;

    pub fn mic_create() -> i32;
    pub fn mic_ctl(fd: i32, cmd: i32, arg_ptr: i32, arg_len_ptr: i32) -> i32;
    pub fn mic_read(fd: i32, out_ptr: i32, out_len_ptr: i32) -> i32;
    pub fn mic_close(fd: i32) -> i32;

    pub fn spear_epoll_create() -> i32;
    pub fn spear_epoll_ctl(epfd: i32, op: i32, fd: i32, events: i32) -> i32;
    pub fn spear_epoll_wait(epfd: i32, out_ptr: i32, out_len_ptr: i32, timeout_ms: i32) -> i32;
    pub fn spear_epoll_close(epfd: i32) -> i32;

    pub fn spear_fd_ctl(fd: i32, cmd: i32, arg_ptr: i32, arg_len_ptr: i32) -> i32;

    pub fn user_stream_open(stream_id: i32, direction: i32) -> i32;
    pub fn user_stream_read(fd: i32, out_ptr: i32, out_len_ptr: i32) -> i32;
    pub fn user_stream_write(fd: i32, buf_ptr: i32, buf_len: i32) -> i32;
    pub fn user_stream_close(fd: i32) -> i32;

    pub fn user_stream_ctl_open() -> i32;
    pub fn user_stream_ctl_read(fd: i32, out_ptr: i32, out_len_ptr: i32) -> i32;
}
