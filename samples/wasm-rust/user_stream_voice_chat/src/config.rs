//! Shared configuration for the Rust voice chat sample.
//! Rust 版语音对话 sample 的共享配置。

pub const STREAM_TEXT: u32 = 1;
pub const STREAM_VOICE: u32 = 2;

pub const MIN_BYTES_BEFORE_COMMIT: usize = 4_800;

pub const RTASR_TRANSPORT: &str = "websocket";
pub const CHAT_TIMEOUT_MS: u32 = 30_000;

pub const EPOLL_TIMEOUT_MS: i32 = 2_000;
pub const EPOLL_READY_CAPACITY: usize = 16;

pub const META_V1_JSON: &[u8] = br#"{"v":1}"#;
