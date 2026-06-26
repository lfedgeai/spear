//! Shared configuration for the Rust live caption sample.
//! Rust 版实时字幕 sample 的共享配置。

pub const STREAM_TEXT: u32 = 1;
pub const STREAM_VOICE: u32 = 2;

pub const AUDIO_BYTES_PER_MS: usize = 32;
pub const MIN_COMMIT_AUDIO_BYTES: usize = 100 * AUDIO_BYTES_PER_MS;
pub const AGGREGATE_WRITE_BYTES: usize = 240 * AUDIO_BYTES_PER_MS;

pub const RTASR_AUTOFLUSH_JSON: &str =
    r#"{"strategy":"server_vad","vad":{"silence_ms":600}}"#;
pub const RTASR_TRANSPORT: &str = "websocket";

pub const EPOLL_TIMEOUT_MS: i32 = 2_000;
pub const EPOLL_READY_CAPACITY: usize = 16;

pub const META_V1_JSON: &[u8] = br#"{"v":1}"#;
