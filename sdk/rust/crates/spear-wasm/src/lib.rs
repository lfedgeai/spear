//! Safe, ergonomic Rust wrappers for Spear WASM hostcalls
//! 面向 Rust 的安全/易用 Spear WASM hostcall 封装
//!
//! Design goal / 设计目标：
//! - Keep this crate independent from any JS engine.
//! - 该 crate 与任何 JS 引擎解耦。
//!
//! ABI note / ABI 说明：
//! - All hostcalls use pointers into WASM linear memory (32-bit).
//! - 所有 hostcall 都通过 32 位指针访问 WASM 线性内存。

#![deny(unsafe_op_in_unsafe_fn)]

use thiserror::Error;

pub use spear_wasm_sys::constants;

pub mod ssf;

#[derive(Debug, Clone, Error)]
#[error("{op}: {code} (errno={errno})")]
pub struct SpearError {
    /// Stable error discriminator / 稳定错误码（可判别）
    pub code: &'static str,
    /// Raw negative errno returned by hostcalls / hostcall 返回的负 errno
    pub errno: i32,
    /// Operation name / 操作名
    pub op: &'static str,
}

impl SpearError {
    pub fn is_code(&self, code: &str) -> bool {
        self.code == code
    }
}

fn errno_to_code(errno: i32) -> &'static str {
    match -errno {
        constants::SPEAR_EBADF => "invalid_fd",
        constants::SPEAR_EFAULT => "invalid_ptr",
        constants::SPEAR_ENOSPC => "buffer_too_small",
        constants::SPEAR_EINVAL => "invalid_cmd",
        constants::SPEAR_EIO => "internal",
        constants::SPEAR_EAGAIN => "eagain",
        constants::SPEAR_ENOTCONN => "not_connected",
        constants::SPEAR_ETIMEDOUT => "timeout",
        constants::SPEAR_ENOSYS => "unsupported",
        _ => "unknown",
    }
}

fn rc_to_result(rc: i32, op: &'static str) -> Result<i32, SpearError> {
    if rc >= 0 {
        Ok(rc)
    } else {
        Err(SpearError {
            code: errno_to_code(rc),
            errno: rc,
            op,
        })
    }
}

fn rc_to_unit(rc: i32, op: &'static str) -> Result<(), SpearError> {
    rc_to_result(rc, op).map(|_| ())
}

fn cast_ptr_len(bytes: &[u8]) -> (i32, i32) {
    let ptr = bytes.as_ptr() as usize as i32;
    let len = bytes.len() as i32;
    (ptr, len)
}

pub const LOG_TRACE: i32 = 0;
pub const LOG_DEBUG: i32 = 1;
pub const LOG_INFO: i32 = 2;
pub const LOG_WARN: i32 = 3;
pub const LOG_ERROR: i32 = 4;

pub fn log_write(level: i32, msg: &str) -> Result<(), SpearError> {
    #[cfg(target_arch = "wasm32")]
    {
        let b = msg.as_bytes();
        let (ptr, len) = cast_ptr_len(b);
        let rc = unsafe { spear_wasm_sys::log(level, ptr, len) };
        rc_to_unit(rc, "log")
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        match level {
            LOG_ERROR | LOG_WARN => eprintln!("{msg}"),
            _ => println!("{msg}"),
        }
        Ok(())
    }
}

pub fn log_trace(msg: &str) -> Result<(), SpearError> {
    log_write(LOG_TRACE, msg)
}

pub fn log_debug(msg: &str) -> Result<(), SpearError> {
    log_write(LOG_DEBUG, msg)
}

pub fn log_info(msg: &str) -> Result<(), SpearError> {
    log_write(LOG_INFO, msg)
}

pub fn log_warn(msg: &str) -> Result<(), SpearError> {
    log_write(LOG_WARN, msg)
}

pub fn log_error(msg: &str) -> Result<(), SpearError> {
    log_write(LOG_ERROR, msg)
}

pub fn sleep_ms(ms: u32) {
    #[cfg(target_arch = "wasm32")]
    unsafe {
        spear_wasm_sys::sleep_ms(ms.min(i32::MAX as u32) as i32);
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = ms;
    }
}

fn recv_alloc_with<F>(
    op: &'static str,
    mut call: F,
    initial_cap: usize,
    max_attempts: usize,
) -> Result<Vec<u8>, SpearError>
where
    F: FnMut(*mut u8, &mut u32) -> i32,
{
    let mut cap = initial_cap.max(1);
    for _ in 0..max_attempts {
        let mut buf = vec![0u8; cap];
        let mut len = cap as u32;
        let rc = call(buf.as_mut_ptr(), &mut len);
        if rc >= 0 {
            let out_len = len as usize;
            buf.truncate(out_len);
            return Ok(buf);
        }
        if rc != -constants::SPEAR_ENOSPC {
            return Err(SpearError {
                code: errno_to_code(rc),
                errno: rc,
                op,
            });
        }

        cap = (len as usize).max(cap.saturating_mul(2));
    }

    Err(SpearError {
        code: "buffer_too_small",
        errno: -constants::SPEAR_ENOSPC,
        op,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Fd(i32);

impl Fd {
    pub fn from_raw(raw: i32) -> Self {
        Self(raw)
    }

    pub fn raw(self) -> i32 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserStreamDirection {
    Inbound,
    Outbound,
    Bidirectional,
}

impl UserStreamDirection {
    pub fn as_i32(self) -> i32 {
        match self {
            UserStreamDirection::Inbound => constants::SPEAR_USER_STREAM_DIR_INBOUND,
            UserStreamDirection::Outbound => constants::SPEAR_USER_STREAM_DIR_OUTBOUND,
            UserStreamDirection::Bidirectional => constants::SPEAR_USER_STREAM_DIR_BIDIRECTIONAL,
        }
    }
}

pub use ssf::SsfMsgType;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UserStreamCtlEvent {
    pub stream_id: u32,
    pub kind: u32,
}

pub fn user_stream_open(stream_id: u32, direction: UserStreamDirection) -> Result<Fd, SpearError> {
    #[cfg(target_arch = "wasm32")]
    {
        let sid = stream_id as i32;
        let rc = unsafe { spear_wasm_sys::user_stream_open(sid, direction.as_i32()) };
        let fd = rc_to_result(rc, "user_stream_open")?;
        Ok(Fd(fd))
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (stream_id, direction);
        Err(SpearError {
            code: "unsupported_target",
            errno: -libc::ENOSYS,
            op: "user_stream_open",
        })
    }
}

pub fn user_stream_write(fd: Fd, bytes: &[u8]) -> Result<(), SpearError> {
    #[cfg(target_arch = "wasm32")]
    {
        let (ptr, len) = cast_ptr_len(bytes);
        let rc = unsafe { spear_wasm_sys::user_stream_write(fd.0, ptr, len) };
        rc_to_unit(rc, "user_stream_write")
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (fd, bytes);
        Err(SpearError {
            code: "unsupported_target",
            errno: -libc::ENOSYS,
            op: "user_stream_write",
        })
    }
}

pub fn user_stream_read_alloc(fd: Fd) -> Result<Option<Vec<u8>>, SpearError> {
    #[cfg(target_arch = "wasm32")]
    {
        let mut cap = 64 * 1024;
        for _ in 0..3 {
            let mut buf = vec![0u8; cap.max(1)];
            let mut len_u32: u32 = buf.len() as u32;
            let rc = unsafe {
                let out_ptr_i32 = buf.as_mut_ptr() as usize as i32;
                let out_len_ptr_i32 = (&mut len_u32 as *mut u32) as usize as i32;
                spear_wasm_sys::user_stream_read(fd.0, out_ptr_i32, out_len_ptr_i32)
            };
            if rc >= 0 {
                buf.truncate(len_u32 as usize);
                return Ok(Some(buf));
            }
            if rc == -constants::SPEAR_EAGAIN {
                return Ok(None);
            }
            if rc != -constants::SPEAR_ENOSPC {
                return Err(SpearError {
                    code: errno_to_code(rc),
                    errno: rc,
                    op: "user_stream_read",
                });
            }
            cap = (len_u32 as usize).max(cap.saturating_mul(2));
        }
        Err(SpearError {
            code: "buffer_too_small",
            errno: -constants::SPEAR_ENOSPC,
            op: "user_stream_read",
        })
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = fd;
        Err(SpearError {
            code: "unsupported_target",
            errno: -libc::ENOSYS,
            op: "user_stream_read",
        })
    }
}

pub fn user_stream_ctl_open() -> Result<Fd, SpearError> {
    #[cfg(target_arch = "wasm32")]
    {
        let fd = unsafe { spear_wasm_sys::user_stream_ctl_open() };
        let fd = rc_to_result(fd, "user_stream_ctl_open")?;
        Ok(Fd(fd))
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        Err(SpearError {
            code: "unsupported_target",
            errno: -libc::ENOSYS,
            op: "user_stream_ctl_open",
        })
    }
}

pub fn user_stream_ctl_read_event(fd: Fd) -> Result<Option<UserStreamCtlEvent>, SpearError> {
    #[cfg(target_arch = "wasm32")]
    {
        let mut cap = 8u32;
        let mut buf = vec![0u8; cap as usize];
        let rc = unsafe {
            let out_ptr_i32 = buf.as_mut_ptr() as usize as i32;
            let out_len_ptr_i32 = (&mut cap as *mut u32) as usize as i32;
            spear_wasm_sys::user_stream_ctl_read(fd.0, out_ptr_i32, out_len_ptr_i32)
        };
        if rc >= 0 {
            if cap as usize != 8 {
                buf.truncate(cap as usize);
            }
            if buf.len() < 8 {
                return Err(SpearError {
                    code: "invalid_ptr",
                    errno: -constants::SPEAR_EFAULT,
                    op: "user_stream_ctl_read",
                });
            }
            let stream_id = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
            let kind = u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]);
            return Ok(Some(UserStreamCtlEvent { stream_id, kind }));
        }
        if rc == -constants::SPEAR_EAGAIN {
            return Ok(None);
        }
        Err(SpearError {
            code: errno_to_code(rc),
            errno: rc,
            op: "user_stream_ctl_read",
        })
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = fd;
        Err(SpearError {
            code: "unsupported_target",
            errno: -libc::ENOSYS,
            op: "user_stream_ctl_read",
        })
    }
}

pub fn user_stream_close(fd: Fd) -> Result<(), SpearError> {
    #[cfg(target_arch = "wasm32")]
    {
        let rc = unsafe { spear_wasm_sys::user_stream_close(fd.0) };
        rc_to_unit(rc, "user_stream_close")
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = fd;
        Err(SpearError {
            code: "unsupported_target",
            errno: -libc::ENOSYS,
            op: "user_stream_close",
        })
    }
}

/// Close a chat-related FD (session or response)
/// 关闭一个 chat 相关 FD（会话或响应）
pub fn cchat_close(fd: Fd) -> Result<(), SpearError> {
    #[cfg(target_arch = "wasm32")]
    {
        let rc = unsafe { spear_wasm_sys::cchat_close(fd.0) };
        rc_to_unit(rc, "cchat_close")
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = fd;
        Err(SpearError {
            code: "unsupported_target",
            errno: -libc::ENOSYS,
            op: "cchat_close",
        })
    }
}

/// Chat session wrapper / Chat 会话封装
pub struct ChatSession {
    fd: Fd,
}

impl ChatSession {
    /// Create a new chat session / 创建新的 chat 会话
    pub fn create() -> Result<Self, SpearError> {
        #[cfg(target_arch = "wasm32")]
        {
            let fd = unsafe { spear_wasm_sys::cchat_create() };
            let fd = rc_to_result(fd, "cchat_create")?;
            return Ok(Self { fd: Fd(fd) });
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            Err(SpearError {
                code: "unsupported_target",
                errno: -libc::ENOSYS,
                op: "cchat_create",
            })
        }
    }

    pub fn fd(&self) -> Fd {
        self.fd
    }

    /// Write one message into the session / 写入一条消息
    pub fn write_message(&mut self, role: &str, content: &str) -> Result<(), SpearError> {
        #[cfg(target_arch = "wasm32")]
        {
            let role_b = role.as_bytes();
            let content_b = content.as_bytes();
            let (role_ptr, role_len) = cast_ptr_len(role_b);
            let (content_ptr, content_len) = cast_ptr_len(content_b);
            let rc = unsafe {
                spear_wasm_sys::cchat_write_msg(self.fd.0, role_ptr, role_len, content_ptr, content_len)
            };
            return rc_to_unit(rc, "cchat_write_msg");
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = (role, content);
            Err(SpearError {
                code: "unsupported_target",
                errno: -libc::ENOSYS,
                op: "cchat_write_msg",
            })
        }
    }

    /// Set one parameter by JSON (`{"key":...,"value":...}`)
    /// 通过 JSON 设置一个参数（`{"key":...,"value":...}`）
    pub fn set_param_json(&mut self, json: &str) -> Result<(), SpearError> {
        #[cfg(target_arch = "wasm32")]
        {
            let json_b = json.as_bytes();
            let (json_ptr, _json_len) = cast_ptr_len(json_b);
            let mut len_u32: u32 = json_b.len() as u32;
            let len_ptr = (&mut len_u32 as *mut u32) as usize as i32;
            let rc = unsafe {
                spear_wasm_sys::cchat_ctl(
                    self.fd.0,
                    constants::SPEAR_CCHAT_CTL_SET_PARAM,
                    json_ptr,
                    len_ptr,
                )
            };
            return rc_to_unit(rc, "cchat_ctl");
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = json;
            Err(SpearError {
                code: "unsupported_target",
                errno: -libc::ENOSYS,
                op: "cchat_ctl",
            })
        }
    }

    /// Register one tool function by offset + JSON schema
    /// 通过 offset + JSON schema 注册一个工具函数
    pub fn write_fn(&mut self, fn_offset: i32, fn_json: &str) -> Result<(), SpearError> {
        #[cfg(target_arch = "wasm32")]
        {
            let fn_b = fn_json.as_bytes();
            let (fn_ptr, fn_len) = cast_ptr_len(fn_b);
            let rc = unsafe { spear_wasm_sys::cchat_write_fn(self.fd.0, fn_offset, fn_ptr, fn_len) };
            return rc_to_unit(rc, "cchat_write_fn");
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = (fn_offset, fn_json);
            Err(SpearError {
                code: "unsupported_target",
                errno: -libc::ENOSYS,
                op: "cchat_write_fn",
            })
        }
    }

    /// Send the chat request and return a response FD
    /// 发送 chat 请求并返回响应 FD
    pub fn send(&mut self, flags: i32) -> Result<Fd, SpearError> {
        #[cfg(target_arch = "wasm32")]
        {
            let resp_fd = unsafe { spear_wasm_sys::cchat_send(self.fd.0, flags) };
            let resp_fd = rc_to_result(resp_fd, "cchat_send")?;
            return Ok(Fd(resp_fd));
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = flags;
            Err(SpearError {
                code: "unsupported_target",
                errno: -libc::ENOSYS,
                op: "cchat_send",
            })
        }
    }

    /// Close the session FD
    /// 关闭会话 FD
    pub fn close(self) -> Result<(), SpearError> {
        cchat_close(self.fd)
    }
}

/// Read response bytes with ENOSPC-capacity growth
/// 通过 ENOSPC 扩容循环读取响应字节
pub fn cchat_recv_alloc(response_fd: Fd) -> Result<Vec<u8>, SpearError> {
    #[cfg(target_arch = "wasm32")]
    {
        recv_alloc_with(
            "cchat_recv",
            |out_ptr, out_len| unsafe {
                let out_ptr_i32 = out_ptr as usize as i32;
                let out_len_ptr_i32 = (out_len as *mut u32) as usize as i32;
                spear_wasm_sys::cchat_recv(response_fd.0, out_ptr_i32, out_len_ptr_i32)
            },
            64 * 1024,
            3,
        )
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = response_fd;
        Err(SpearError {
            code: "unsupported_target",
            errno: -libc::ENOSYS,
            op: "cchat_recv",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_errno_mapping_known() {
        assert_eq!(errno_to_code(-constants::SPEAR_EBADF), "invalid_fd");
        assert_eq!(errno_to_code(-constants::SPEAR_ENOSPC), "buffer_too_small");
        assert_eq!(errno_to_code(-constants::SPEAR_EAGAIN), "eagain");
    }

    #[test]
    fn test_recv_alloc_grows_on_enospc() {
        let payload = b"hello world".to_vec();
        let mut called = 0usize;
        let out = recv_alloc_with(
            "fake_recv",
            |out_ptr, out_len| {
                called += 1;
                if called == 1 {
                    *out_len = 128;
                    return -constants::SPEAR_ENOSPC;
                }
                unsafe {
                    std::ptr::copy_nonoverlapping(payload.as_ptr(), out_ptr, payload.len());
                }
                *out_len = payload.len() as u32;
                payload.len() as i32
            },
            8,
            3,
        )
        .unwrap();

        assert_eq!(called, 2);
        assert_eq!(out, payload);
    }
}
