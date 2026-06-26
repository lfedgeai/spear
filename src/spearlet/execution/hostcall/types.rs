use serde_json::Value;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Condvar, Mutex};

use crate::spearlet::execution::ai::ir::ChatMessage;
use crate::spearlet::mcp::policy::McpSessionParams;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FdKind {
    ChatSession,
    ChatResponse,
    Epoll,
    RtAsr,
    Mic,
    UserStream,
    UserStreamCtl,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FdFlags(u32);

impl FdFlags {
    pub const EMPTY: Self = Self(0);
    pub const O_NONBLOCK: Self = Self(0x1);

    pub fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }

    pub fn insert(&mut self, other: Self) {
        self.0 |= other.0;
    }

    pub fn remove(&mut self, other: Self) {
        self.0 &= !other.0;
    }

    pub fn bits(self) -> u32 {
        self.0
    }
}

impl Default for FdFlags {
    fn default() -> Self {
        Self::EMPTY
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PollEvents(u32);

impl PollEvents {
    pub const EMPTY: Self = Self(0);
    pub const IN: Self = Self(0x001);
    pub const OUT: Self = Self(0x004);
    pub const ERR: Self = Self(0x008);
    pub const HUP: Self = Self(0x010);

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub fn bits(self) -> u32 {
        self.0
    }

    pub fn from_bits_truncate(v: u32) -> Self {
        Self(v & (Self::IN.0 | Self::OUT.0 | Self::ERR.0 | Self::HUP.0))
    }

    pub fn intersects(self, other: Self) -> bool {
        (self.0 & other.0) != 0
    }

    pub fn and(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }

    pub fn or(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    pub fn insert(&mut self, other: Self) {
        self.0 |= other.0;
    }
}

impl Default for PollEvents {
    fn default() -> Self {
        Self::EMPTY
    }
}

#[derive(Clone, Debug, Default)]
pub struct ChatSessionState {
    pub messages: Vec<ChatMessage>,
    pub tools: Vec<(i32, String)>,
    pub params: HashMap<String, Value>,
    pub mcp: McpSessionParams,
}

#[derive(Clone, Debug, Default)]
pub struct ChatResponseState {
    pub bytes: Vec<u8>,
    pub metrics_bytes: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RtAsrConnState {
    Init,
    Configured,
    Connecting,
    Connected,
    Draining,
    Closed,
    Error,
}

#[derive(Clone, Debug)]
pub enum RtAsrSendItem {
    Audio(Vec<u8>),
    WsText(String),
}

impl RtAsrSendItem {
    pub fn byte_len(&self) -> usize {
        match self {
            RtAsrSendItem::Audio(b) => b.len(),
            RtAsrSendItem::WsText(s) => s.len(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RtAsrSegmentationStrategy {
    Manual,
    ServerVad,
    ClientCommit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RtAsrClientCommitMode {
    Hybrid,
}

#[derive(Clone, Debug)]
pub struct RtAsrVadConfig {
    pub silence_ms: u64,
    pub threshold: Option<f64>,
    pub prefix_padding_ms: Option<u64>,
}

impl Default for RtAsrVadConfig {
    fn default() -> Self {
        Self {
            silence_ms: 500,
            threshold: Some(0.5),
            prefix_padding_ms: Some(300),
        }
    }
}

#[derive(Clone, Debug)]
pub struct RtAsrClientCommitConfig {
    pub mode: RtAsrClientCommitMode,
    pub flush_interval_ms: Option<u64>,
    pub max_buffer_bytes: Option<usize>,
    pub silence_ms: Option<u64>,
    pub min_flush_gap_ms: u64,
}

impl Default for RtAsrClientCommitConfig {
    fn default() -> Self {
        Self {
            mode: RtAsrClientCommitMode::Hybrid,
            flush_interval_ms: None,
            max_buffer_bytes: None,
            silence_ms: None,
            min_flush_gap_ms: 500,
        }
    }
}

#[derive(Clone, Debug)]
pub struct RtAsrSegmentationConfig {
    pub strategy: RtAsrSegmentationStrategy,
    pub vad: Option<RtAsrVadConfig>,
    pub client_commit: Option<RtAsrClientCommitConfig>,
    pub flush_on_close: bool,
}

impl Default for RtAsrSegmentationConfig {
    fn default() -> Self {
        Self {
            strategy: RtAsrSegmentationStrategy::ServerVad,
            vad: Some(RtAsrVadConfig::default()),
            client_commit: None,
            flush_on_close: false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct RtAsrState {
    pub state: RtAsrConnState,
    pub params: HashMap<String, Value>,
    pub send_queue: VecDeque<RtAsrSendItem>,
    pub send_queue_bytes: usize,
    pub max_send_queue_bytes: usize,
    pub recv_queue: VecDeque<Vec<u8>>,
    pub recv_queue_bytes: usize,
    pub max_recv_queue_bytes: usize,
    pub dropped_events: u64,
    pub last_error: Option<String>,
    pub tasks_spawned: bool,
    pub stub_event_seq: u64,

    pub segmentation: RtAsrSegmentationConfig,
    pub pending_flush: bool,
    pub buffered_audio_bytes_since_flush: usize,
    pub last_flush_at: std::time::Instant,
    pub last_audio_at: std::time::Instant,
}

impl Default for RtAsrState {
    fn default() -> Self {
        let now = std::time::Instant::now();
        Self {
            state: RtAsrConnState::Init,
            params: HashMap::new(),
            send_queue: VecDeque::new(),
            send_queue_bytes: 0,
            max_send_queue_bytes: 1024 * 1024,
            recv_queue: VecDeque::new(),
            recv_queue_bytes: 0,
            max_recv_queue_bytes: 1024 * 1024,
            dropped_events: 0,
            last_error: None,
            tasks_spawned: false,
            stub_event_seq: 0,

            segmentation: RtAsrSegmentationConfig::default(),
            pending_flush: false,
            buffered_audio_bytes_since_flush: 0,
            last_flush_at: now,
            last_audio_at: now,
        }
    }
}

#[derive(Clone, Debug)]
pub struct MicConfig {
    pub sample_rate_hz: u32,
    pub channels: u8,
    pub frame_ms: u32,
    pub format: String,
}

/// User stream direction / 用户流方向
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UserStreamDirection {
    In,
    Out,
    Bidi,
}

impl UserStreamDirection {
    pub fn from_i32(v: i32) -> Option<Self> {
        match v {
            1 => Some(Self::In),
            2 => Some(Self::Out),
            3 => Some(Self::Bidi),
            _ => None,
        }
    }

    pub fn allows_read(self) -> bool {
        matches!(self, Self::In | Self::Bidi)
    }

    pub fn allows_write(self) -> bool {
        matches!(self, Self::Out | Self::Bidi)
    }
}

/// User stream connection state / 用户流连接状态
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UserStreamConnState {
    Init,
    Connected,
    Closed,
    Error,
}

/// Shared channel state for one logical stream_id / 单个 stream_id 的共享通道状态
pub struct UserStreamChannel {
    pub stream_id: u32,
    pub conn_state: UserStreamConnState,

    pub inbound: VecDeque<Vec<u8>>,
    pub inbound_bytes: usize,
    pub max_inbound_bytes: usize,

    pub outbound: VecDeque<Vec<u8>>,
    pub outbound_bytes: usize,
    pub max_outbound_bytes: usize,

    pub max_frame_bytes: usize,
    pub last_error: Option<String>,

    pub notify_outbound: std::sync::Arc<tokio::sync::Notify>,
    pub notify_state: std::sync::Arc<tokio::sync::Notify>,
    pub attached_fds: HashSet<i32>,
}

impl std::fmt::Debug for UserStreamChannel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UserStreamChannel")
            .field("stream_id", &self.stream_id)
            .field("conn_state", &self.conn_state)
            .field("inbound_len", &self.inbound.len())
            .field("inbound_bytes", &self.inbound_bytes)
            .field("max_inbound_bytes", &self.max_inbound_bytes)
            .field("outbound_len", &self.outbound.len())
            .field("outbound_bytes", &self.outbound_bytes)
            .field("max_outbound_bytes", &self.max_outbound_bytes)
            .field("max_frame_bytes", &self.max_frame_bytes)
            .field("last_error", &self.last_error)
            .field("attached_fds_len", &self.attached_fds.len())
            .finish()
    }
}

impl UserStreamChannel {
    pub fn new(stream_id: u32) -> Self {
        Self {
            stream_id,
            conn_state: UserStreamConnState::Init,
            inbound: VecDeque::new(),
            inbound_bytes: 0,
            max_inbound_bytes: 8 * 1024 * 1024,
            outbound: VecDeque::new(),
            outbound_bytes: 0,
            max_outbound_bytes: 8 * 1024 * 1024,
            max_frame_bytes: 256 * 1024,
            last_error: None,
            notify_outbound: std::sync::Arc::new(tokio::sync::Notify::new()),
            notify_state: std::sync::Arc::new(tokio::sync::Notify::new()),
            attached_fds: HashSet::new(),
        }
    }
}

/// Per-fd user stream view / 每个 fd 的 user stream 视图
pub struct UserStreamState {
    pub execution_id: String,
    pub stream_id: u32,
    pub direction: UserStreamDirection,
    pub channel: Arc<Mutex<UserStreamChannel>>,
}

impl std::fmt::Debug for UserStreamState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UserStreamState")
            .field("execution_id", &self.execution_id)
            .field("stream_id", &self.stream_id)
            .field("direction", &self.direction)
            .finish()
    }
}

pub struct UserStreamCtlState {
    pub execution_id: String,
    pub pending: VecDeque<[u8; 8]>,
    pub max_pending: usize,
}

impl std::fmt::Debug for UserStreamCtlState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UserStreamCtlState")
            .field("execution_id", &self.execution_id)
            .field("pending_len", &self.pending.len())
            .field("max_pending", &self.max_pending)
            .finish()
    }
}

pub struct MicState {
    pub config: Option<MicConfig>,
    pub queue: VecDeque<Vec<u8>>,
    pub queue_bytes: usize,
    pub max_queue_bytes: usize,
    pub dropped_frames: u64,
    pub last_error: Option<String>,
    pub running: bool,
    pub generation: u64,
    pub stub_pcm16: Option<Vec<u8>>,
    pub stub_pcm16_offset: usize,
}

impl std::fmt::Debug for MicState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MicState")
            .field("config", &self.config)
            .field("queue_len", &self.queue.len())
            .field("queue_bytes", &self.queue_bytes)
            .field("max_queue_bytes", &self.max_queue_bytes)
            .field("dropped_frames", &self.dropped_frames)
            .field("last_error", &self.last_error)
            .field("running", &self.running)
            .field("generation", &self.generation)
            .field("stub_pcm16_len", &self.stub_pcm16.as_ref().map(|v| v.len()))
            .field("stub_pcm16_offset", &self.stub_pcm16_offset)
            .finish()
    }
}

impl Default for MicState {
    fn default() -> Self {
        Self {
            config: None,
            queue: VecDeque::new(),
            queue_bytes: 0,
            max_queue_bytes: 512 * 1024,
            dropped_frames: 0,
            last_error: None,
            running: false,
            generation: 0,
            stub_pcm16: None,
            stub_pcm16_offset: 0,
        }
    }
}

#[derive(Debug)]
pub struct EpollState {
    inner: Mutex<EpollInner>,
    cv: Condvar,
}

#[derive(Debug)]
struct EpollInner {
    watch: HashMap<i32, PollEvents>,
    seq: u64,
    closed: bool,
}

impl EpollState {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(EpollInner {
                watch: HashMap::new(),
                seq: 0,
                closed: false,
            }),
            cv: Condvar::new(),
        }
    }

    pub fn notify(&self) {
        let mut st = self.inner.lock().unwrap();
        st.seq = st.seq.wrapping_add(1);
        self.cv.notify_all();
    }

    pub fn snapshot_watch(&self) -> HashMap<i32, PollEvents> {
        let st = self.inner.lock().unwrap();
        st.watch.clone()
    }

    pub fn ctl_add(&self, fd: i32, events: PollEvents) {
        let mut st = self.inner.lock().unwrap();
        st.watch.insert(fd, events);
        st.seq = st.seq.wrapping_add(1);
        self.cv.notify_all();
    }

    pub fn ctl_mod(&self, fd: i32, events: PollEvents) {
        let mut st = self.inner.lock().unwrap();
        st.watch.insert(fd, events);
        st.seq = st.seq.wrapping_add(1);
        self.cv.notify_all();
    }

    pub fn ctl_del(&self, fd: i32) {
        let mut st = self.inner.lock().unwrap();
        st.watch.remove(&fd);
        st.seq = st.seq.wrapping_add(1);
        self.cv.notify_all();
    }

    pub fn list_watched_fds(&self) -> Vec<i32> {
        let st = self.inner.lock().unwrap();
        st.watch.keys().copied().collect()
    }

    pub fn mark_closed(&self) {
        let mut st = self.inner.lock().unwrap();
        st.closed = true;
        st.seq = st.seq.wrapping_add(1);
        self.cv.notify_all();
    }

    pub fn wait_for_change(&self, last_seq: u64, timeout: Option<std::time::Duration>) -> bool {
        let mut st = self.inner.lock().unwrap();
        if st.closed {
            return true;
        }
        if st.seq != last_seq {
            return true;
        }
        match timeout {
            None => {
                while !st.closed && st.seq == last_seq {
                    st = self.cv.wait(st).unwrap();
                }
                true
            }
            Some(dur) => {
                let (guard, wait_res) = self
                    .cv
                    .wait_timeout_while(st, dur, |g| !g.closed && g.seq == last_seq)
                    .unwrap();
                st = guard;
                !wait_res.timed_out() || st.closed || st.seq != last_seq
            }
        }
    }

    pub fn current_seq(&self) -> u64 {
        let st = self.inner.lock().unwrap();
        st.seq
    }
}

impl Default for EpollState {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
pub enum FdInner {
    ChatSession(ChatSessionState),
    ChatResponse(ChatResponseState),
    Epoll(Arc<EpollState>),
    RtAsr(Box<RtAsrState>),
    Mic(MicState),
    UserStream(Box<UserStreamState>),
    UserStreamCtl(Box<UserStreamCtlState>),
}

#[derive(Debug)]
pub struct FdEntry {
    pub kind: FdKind,
    pub flags: FdFlags,
    pub poll_mask: PollEvents,
    pub watchers: HashSet<i32>,
    pub closed: bool,
    pub inner: FdInner,
}

impl FdEntry {
    pub fn effective_poll_mask(&self) -> PollEvents {
        if self.closed {
            self.poll_mask.or(PollEvents::HUP)
        } else {
            self.poll_mask
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Barrier};
    use std::time::Duration;

    #[test]
    fn poll_events_from_bits_truncate_masks_unknown_bits() {
        let all = 0xFFFF_FFFF;
        let ev = PollEvents::from_bits_truncate(all);
        assert_eq!(
            ev.bits(),
            (PollEvents::IN
                .or(PollEvents::OUT)
                .or(PollEvents::ERR)
                .or(PollEvents::HUP))
            .bits()
        );
    }

    #[test]
    fn fd_flags_insert_contains_remove_work() {
        let mut f = FdFlags::default();
        assert!(!f.contains(FdFlags::O_NONBLOCK));
        f.insert(FdFlags::O_NONBLOCK);
        assert!(f.contains(FdFlags::O_NONBLOCK));
        f.remove(FdFlags::O_NONBLOCK);
        assert!(!f.contains(FdFlags::O_NONBLOCK));
    }

    #[test]
    fn fd_entry_effective_poll_mask_adds_hup_when_closed() {
        let e = FdEntry {
            kind: FdKind::ChatResponse,
            flags: FdFlags::default(),
            poll_mask: PollEvents::IN,
            watchers: HashSet::new(),
            closed: true,
            inner: FdInner::ChatResponse(ChatResponseState::default()),
        };
        assert!(e.effective_poll_mask().intersects(PollEvents::IN));
        assert!(e.effective_poll_mask().intersects(PollEvents::HUP));
    }

    #[test]
    fn epoll_state_wait_for_change_times_out_without_change() {
        let ep = EpollState::new();
        let last = ep.current_seq();
        let changed = ep.wait_for_change(last, Some(Duration::from_millis(10)));
        assert!(!changed);
    }

    #[test]
    fn epoll_state_wait_for_change_observes_notify() {
        let ep = Arc::new(EpollState::new());
        let barrier = Arc::new(Barrier::new(2));
        let ep2 = Arc::clone(&ep);
        let b2 = Arc::clone(&barrier);
        std::thread::spawn(move || {
            b2.wait();
            ep2.notify();
        });

        let last = ep.current_seq();
        barrier.wait();
        let changed = ep.wait_for_change(last, Some(Duration::from_millis(200)));
        assert!(changed);
        assert_ne!(ep.current_seq(), last);
    }
}
