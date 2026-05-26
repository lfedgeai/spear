//! User stream bridge (WS <-> WASM fd) / 用户流桥接（WS <-> WASM fd）

use crate::spearlet::execution::host_api::errno::{
    SPEAR_EAGAIN, SPEAR_EBADF, SPEAR_EINVAL, SPEAR_ENOSPC, SPEAR_ENOTCONN, SPEAR_EPIPE,
};
use crate::spearlet::execution::host_api::DefaultHostApi;
use crate::spearlet::execution::hostcall::fd_table::FdTable;
use crate::spearlet::execution::hostcall::types::{
    FdEntry, FdFlags, FdInner, FdKind, PollEvents, UserStreamChannel, UserStreamConnState,
    UserStreamCtlState, UserStreamDirection, UserStreamState,
};
use dashmap::DashMap;
use std::collections::HashSet;
use std::sync::{Arc, Mutex, OnceLock};

static USER_STREAM_HUBS: OnceLock<DashMap<String, Arc<ExecutionUserStreamHub>>> = OnceLock::new();

fn user_stream_hubs() -> &'static DashMap<String, Arc<ExecutionUserStreamHub>> {
    USER_STREAM_HUBS.get_or_init(DashMap::new)
}

pub(crate) struct ExecutionUserStreamHub {
    streams: DashMap<u32, Arc<Mutex<UserStreamChannel>>>,
    fd_table: Mutex<Option<Arc<FdTable>>>,
    notify_any_outbound: Arc<tokio::sync::Notify>,
    ctl_fds: Mutex<HashSet<i32>>,
}

impl ExecutionUserStreamHub {
    pub(crate) fn get(execution_id: &str) -> Option<Arc<Self>> {
        user_stream_hubs().get(execution_id).map(|e| e.clone())
    }

    pub(crate) fn get_or_create(execution_id: &str) -> Arc<Self> {
        user_stream_hubs()
            .entry(execution_id.to_string())
            .or_insert_with(|| {
                Arc::new(Self {
                    streams: DashMap::new(),
                    fd_table: Mutex::new(None),
                    notify_any_outbound: Arc::new(tokio::sync::Notify::new()),
                    ctl_fds: Mutex::new(HashSet::new()),
                })
            })
            .clone()
    }

    pub(crate) fn attach_fd_table(&self, table: Arc<FdTable>) {
        let mut st = self.fd_table.lock().unwrap();
        if st.is_none() {
            *st = Some(table);
        }
    }

    fn get_or_create_channel(&self, stream_id: u32) -> Arc<Mutex<UserStreamChannel>> {
        self.streams
            .entry(stream_id)
            .or_insert_with(|| Arc::new(Mutex::new(UserStreamChannel::new(stream_id))))
            .clone()
    }

    pub(crate) fn mark_connected(&self, stream_id: u32) {
        let ch = self.get_or_create_channel(stream_id);
        {
            let mut st = ch.lock().unwrap();
            if st.conn_state != UserStreamConnState::Connected {
                st.conn_state = UserStreamConnState::Connected;
                st.notify_state.notify_waiters();
            }
        }
        self.recompute_and_notify_attached_fds(&ch);
        self.notify_ctl_stream_connected(stream_id);
    }

    pub(crate) fn mark_closed_all(&self) {
        let channels = self
            .streams
            .iter()
            .map(|e| e.value().clone())
            .collect::<Vec<_>>();
        for ch in channels {
            {
                let mut st = ch.lock().unwrap();
                if st.conn_state != UserStreamConnState::Closed {
                    st.conn_state = UserStreamConnState::Closed;
                    st.notify_state.notify_waiters();
                    st.notify_outbound.notify_waiters();
                }
            }
            self.recompute_and_notify_attached_fds(&ch);
        }
        self.notify_ctl_session_closed();
    }

    pub(crate) fn push_inbound_frame(&self, stream_id: u32, frame: Vec<u8>) -> i32 {
        let ch = self.get_or_create_channel(stream_id);
        let mut st = ch.lock().unwrap();
        if frame.len() > st.max_frame_bytes {
            st.last_error = Some("frame_too_large".to_string());
            st.conn_state = UserStreamConnState::Error;
            st.notify_state.notify_waiters();
            drop(st);
            self.recompute_and_notify_attached_fds(&ch);
            return -SPEAR_EINVAL;
        }
        if st.inbound_bytes.saturating_add(frame.len()) > st.max_inbound_bytes {
            st.last_error = Some("inbound_queue_full".to_string());
            st.conn_state = UserStreamConnState::Error;
            st.notify_state.notify_waiters();
            drop(st);
            self.recompute_and_notify_attached_fds(&ch);
            return -SPEAR_ENOSPC;
        }
        st.inbound_bytes = st.inbound_bytes.saturating_add(frame.len());
        st.inbound.push_back(frame);
        drop(st);
        self.recompute_and_notify_attached_fds(&ch);
        0
    }

    pub(crate) fn pop_outbound_frame_any(&self) -> Option<(u32, Vec<u8>)> {
        for entry in self.streams.iter() {
            let stream_id = *entry.key();
            let ch = entry.value().clone();
            if let Some(frame) = self.pop_outbound_frame(&ch) {
                return Some((stream_id, frame));
            }
        }
        None
    }

    fn pop_outbound_frame(&self, ch: &Arc<Mutex<UserStreamChannel>>) -> Option<Vec<u8>> {
        let mut st = ch.lock().unwrap();
        let frame = st.outbound.pop_front()?;
        st.outbound_bytes = st.outbound_bytes.saturating_sub(frame.len());
        drop(st);
        self.recompute_and_notify_attached_fds(ch);
        Some(frame)
    }

    pub(crate) fn notify_outbound_waiters(&self, stream_id: u32) {
        let ch = self.get_or_create_channel(stream_id);
        let notify = {
            let st = ch.lock().unwrap();
            st.notify_outbound.clone()
        };
        notify.notify_waiters();
        self.notify_any_outbound.notify_waiters();
    }

    fn recompute_and_notify_attached_fds(&self, ch: &Arc<Mutex<UserStreamChannel>>) {
        let table = { self.fd_table.lock().unwrap().clone() };
        let Some(table) = table else {
            return;
        };

        let attached = {
            let st = ch.lock().unwrap();
            st.attached_fds.iter().copied().collect::<Vec<_>>()
        };

        for fd in attached {
            let Some(entry) = table.get(fd) else {
                continue;
            };
            let notify = {
                let mut e = match entry.lock() {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                let old = e.poll_mask;
                let new_mask = compute_user_stream_poll_mask(&e, ch);
                e.poll_mask = new_mask;
                old.bits() != new_mask.bits()
            };
            if notify {
                table.notify_watchers(fd);
            }
        }
    }

    fn list_ctl_fds(&self) -> Vec<i32> {
        self.ctl_fds.lock().unwrap().iter().copied().collect()
    }

    pub(crate) fn register_ctl_fd(&self, fd: i32) {
        self.ctl_fds.lock().unwrap().insert(fd);
    }

    pub(crate) fn unregister_ctl_fd(&self, fd: i32) {
        self.ctl_fds.lock().unwrap().remove(&fd);
    }

    fn notify_ctl_stream_connected(&self, stream_id: u32) {
        let table = { self.fd_table.lock().unwrap().clone() };
        let Some(table) = table else {
            return;
        };
        let fds = self.list_ctl_fds();
        for fd in fds {
            let Some(entry) = table.get(fd) else {
                continue;
            };
            let notify = {
                let mut e = match entry.lock() {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                let old = e.poll_mask;
                if let FdInner::UserStreamCtl(st) = &mut e.inner {
                    if st.pending.len() >= st.max_pending {
                        st.pending.pop_front();
                    }
                    st.pending
                        .push_back(encode_ctl_event(stream_id, CTL_EVENT_STREAM_CONNECTED));
                }
                e.poll_mask = compute_user_stream_ctl_poll_mask(&e);
                e.poll_mask.bits() != old.bits()
            };
            if notify {
                table.notify_watchers(fd);
            }
        }
    }

    fn notify_ctl_session_closed(&self) {
        let table = { self.fd_table.lock().unwrap().clone() };
        let Some(table) = table else {
            return;
        };
        let fds = self.list_ctl_fds();
        for fd in fds {
            let Some(entry) = table.get(fd) else {
                continue;
            };
            let notify = {
                let mut e = match entry.lock() {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                let old = e.poll_mask;
                if let FdInner::UserStreamCtl(st) = &mut e.inner {
                    if st.pending.len() >= st.max_pending {
                        st.pending.pop_front();
                    }
                    st.pending
                        .push_back(encode_ctl_event(0, CTL_EVENT_SESSION_CLOSED));
                }
                e.poll_mask = compute_user_stream_ctl_poll_mask(&e);
                e.poll_mask.bits() != old.bits()
            };
            if notify {
                table.notify_watchers(fd);
            }
        }
    }
}

fn compute_user_stream_poll_mask(e: &FdEntry, ch: &Arc<Mutex<UserStreamChannel>>) -> PollEvents {
    let FdInner::UserStream(st) = &e.inner else {
        return PollEvents::EMPTY;
    };
    let dir = st.direction;
    let c = ch.lock().unwrap();

    let mut mask = PollEvents::EMPTY;
    if dir.allows_read() && !c.inbound.is_empty() {
        mask.insert(PollEvents::IN);
    }
    let writable = dir.allows_write()
        && c.conn_state == UserStreamConnState::Connected
        && c.outbound_bytes < c.max_outbound_bytes
        && !e.closed;
    if writable {
        mask.insert(PollEvents::OUT);
    }
    if c.conn_state == UserStreamConnState::Error {
        mask.insert(PollEvents::ERR);
    }
    if e.closed || c.conn_state == UserStreamConnState::Closed {
        mask.insert(PollEvents::HUP);
    }
    mask
}

const CTL_EVENT_STREAM_CONNECTED: u32 = 1;
const CTL_EVENT_SESSION_CLOSED: u32 = 2;

fn encode_ctl_event(stream_id: u32, kind: u32) -> [u8; 8] {
    let mut out = [0u8; 8];
    out[0..4].copy_from_slice(&stream_id.to_le_bytes());
    out[4..8].copy_from_slice(&kind.to_le_bytes());
    out
}

fn compute_user_stream_ctl_poll_mask(e: &FdEntry) -> PollEvents {
    let FdInner::UserStreamCtl(st) = &e.inner else {
        return PollEvents::EMPTY;
    };
    if e.closed {
        return PollEvents::HUP;
    }
    let mut mask = PollEvents::EMPTY;
    if !st.pending.is_empty() {
        mask.insert(PollEvents::IN);
    }
    mask
}

impl DefaultHostApi {
    pub fn user_stream_open(&self, stream_id: i32, direction: i32) -> i32 {
        let Some(execution_id) = super::core::current_wasm_execution_id() else {
            return -SPEAR_ENOTCONN;
        };
        let stream_id_u32 = match u32::try_from(stream_id) {
            Ok(v) => v,
            Err(_) => return -SPEAR_EINVAL,
        };
        let Some(dir) = UserStreamDirection::from_i32(direction) else {
            return -SPEAR_EINVAL;
        };

        let hub = ExecutionUserStreamHub::get_or_create(&execution_id);
        hub.attach_fd_table(self.fd_table.clone());
        let ch = hub.get_or_create_channel(stream_id_u32);

        let fd = self.fd_table.alloc(FdEntry {
            kind: FdKind::UserStream,
            flags: FdFlags::default(),
            poll_mask: PollEvents::EMPTY,
            watchers: HashSet::new(),
            closed: false,
            inner: FdInner::UserStream(Box::new(UserStreamState {
                execution_id,
                stream_id: stream_id_u32,
                direction: dir,
                channel: ch.clone(),
            })),
        });

        {
            let mut st = ch.lock().unwrap();
            st.attached_fds.insert(fd);
        }

        if let Some(entry) = self.fd_table.get(fd) {
            if let Ok(mut e) = entry.lock() {
                let new_mask = compute_user_stream_poll_mask(&e, &ch);
                e.poll_mask = new_mask;
            }
        }

        fd
    }

    pub fn user_stream_read(&self, fd: i32) -> Result<Vec<u8>, i32> {
        let Some(entry) = self.fd_table.get(fd) else {
            return Err(-SPEAR_EBADF);
        };
        let (changed, payload) = {
            let mut e = entry.lock().map_err(|_| -SPEAR_EINVAL)?;
            if e.closed {
                return Err(-SPEAR_EBADF);
            }
            let FdInner::UserStream(st) = &mut e.inner else {
                return Err(-SPEAR_EBADF);
            };
            if !st.direction.allows_read() {
                return Err(-SPEAR_EINVAL);
            }
            let ch = st.channel.clone();
            let old_mask = e.poll_mask;

            let mut payload: Option<Vec<u8>> = None;
            {
                let mut c = ch.lock().unwrap();
                if let Some(p) = c.inbound.pop_front() {
                    c.inbound_bytes = c.inbound_bytes.saturating_sub(p.len());
                    payload = Some(p);
                }
            }

            let new_mask = compute_user_stream_poll_mask(&e, &ch);
            e.poll_mask = new_mask;
            (new_mask.bits() != old_mask.bits(), payload)
        };

        if changed {
            self.fd_table.notify_watchers(fd);
        }

        match payload {
            Some(p) => Ok(p),
            None => Err(-SPEAR_EAGAIN),
        }
    }

    pub fn user_stream_write(&self, fd: i32, bytes: &[u8]) -> i32 {
        let Some(entry) = self.fd_table.get(fd) else {
            return -SPEAR_EBADF;
        };
        let (frame_stream_id, _msg_type) = match super::ssf::parse_ssf_v1_header(bytes) {
            Ok(v) => v,
            Err(e) => return e,
        };

        let (execution_id, stream_id, old_mask, rc, new_mask) = {
            let mut e = match entry.lock() {
                Ok(v) => v,
                Err(_) => return -SPEAR_EINVAL,
            };
            if e.closed {
                return -SPEAR_EBADF;
            }
            let (execution_id, stream_id, direction, ch) = {
                let FdInner::UserStream(st) = &mut e.inner else {
                    return -SPEAR_EBADF;
                };
                (
                    st.execution_id.clone(),
                    st.stream_id,
                    st.direction,
                    st.channel.clone(),
                )
            };
            if !direction.allows_write() {
                return -SPEAR_EINVAL;
            }
            if stream_id != frame_stream_id {
                return -SPEAR_EINVAL;
            }
            let old_mask = e.poll_mask;
            let mut rc = 0;
            {
                let mut c = ch.lock().unwrap();
                match c.conn_state {
                    UserStreamConnState::Connected => {}
                    UserStreamConnState::Init => {
                        rc = -SPEAR_ENOTCONN;
                    }
                    UserStreamConnState::Closed => {
                        rc = -SPEAR_EPIPE;
                    }
                    UserStreamConnState::Error => {
                        rc = -SPEAR_EPIPE;
                    }
                }
                if rc == 0 {
                    if bytes.len() > c.max_frame_bytes {
                        rc = -SPEAR_EINVAL;
                    } else if c.outbound_bytes.saturating_add(bytes.len()) > c.max_outbound_bytes {
                        rc = -SPEAR_EAGAIN;
                    } else {
                        c.outbound_bytes = c.outbound_bytes.saturating_add(bytes.len());
                        c.outbound.push_back(bytes.to_vec());
                    }
                }
            }

            let new_mask = compute_user_stream_poll_mask(&e, &ch);
            e.poll_mask = new_mask;
            (execution_id, stream_id, old_mask, rc, new_mask)
        };

        if new_mask.bits() != old_mask.bits() {
            self.fd_table.notify_watchers(fd);
        }

        if rc == 0 {
            let hub = ExecutionUserStreamHub::get_or_create(&execution_id);
            hub.notify_outbound_waiters(stream_id);
        }
        rc
    }

    pub fn user_stream_close(&self, fd: i32) -> i32 {
        if let Some(entry) = self.fd_table.get(fd) {
            if let Ok(e) = entry.lock() {
                if let FdInner::UserStream(st) = &e.inner {
                    if let Ok(mut c) = st.channel.lock() {
                        c.attached_fds.remove(&fd);
                    }
                }
                if let FdInner::UserStreamCtl(st) = &e.inner {
                    let hub = ExecutionUserStreamHub::get_or_create(&st.execution_id);
                    hub.unregister_ctl_fd(fd);
                }
            }
        }
        self.fd_table.close(fd)
    }

    pub fn user_stream_ctl_open(&self) -> i32 {
        let Some(execution_id) = super::core::current_wasm_execution_id() else {
            return -SPEAR_ENOTCONN;
        };
        let hub = ExecutionUserStreamHub::get_or_create(&execution_id);
        hub.attach_fd_table(self.fd_table.clone());

        let fd = self.fd_table.alloc(FdEntry {
            kind: FdKind::UserStreamCtl,
            flags: FdFlags::default(),
            poll_mask: PollEvents::EMPTY,
            watchers: HashSet::new(),
            closed: false,
            inner: FdInner::UserStreamCtl(Box::new(UserStreamCtlState {
                execution_id: execution_id.clone(),
                pending: std::collections::VecDeque::new(),
                max_pending: 1024,
            })),
        });

        hub.register_ctl_fd(fd);

        if let Some(entry) = self.fd_table.get(fd) {
            if let Ok(mut e) = entry.lock() {
                e.poll_mask = compute_user_stream_ctl_poll_mask(&e);
            }
        }
        fd
    }

    pub fn user_stream_ctl_read(&self, fd: i32) -> Result<[u8; 8], i32> {
        let Some(entry) = self.fd_table.get(fd) else {
            return Err(-SPEAR_EBADF);
        };
        let (changed, payload) = {
            let mut e = entry.lock().map_err(|_| -SPEAR_EINVAL)?;
            if e.closed {
                return Err(-SPEAR_EBADF);
            }
            let old = e.poll_mask;
            let (payload, new_mask) = {
                let FdInner::UserStreamCtl(st) = &mut e.inner else {
                    return Err(-SPEAR_EBADF);
                };
                let payload = st.pending.pop_front();
                let mut new_mask = PollEvents::EMPTY;
                if !st.pending.is_empty() {
                    new_mask.insert(PollEvents::IN);
                }
                (payload, new_mask)
            };
            e.poll_mask = new_mask;
            (new_mask.bits() != old.bits(), payload)
        };
        if changed {
            self.fd_table.notify_watchers(fd);
        }
        match payload {
            Some(p) => Ok(p),
            None => Err(-SPEAR_EAGAIN),
        }
    }
}

pub fn map_ws_close_to_channels(execution_id: &str) {
    let Some(hub) = user_stream_hubs().get(execution_id).map(|e| e.clone()) else {
        return;
    };
    hub.mark_closed_all();
    hub.notify_any_outbound.notify_waiters();
    user_stream_hubs().remove(execution_id);
}

pub fn ws_push_frame(execution_id: &str, frame: Vec<u8>) -> i32 {
    let (stream_id, _msg_type) = match super::ssf::parse_ssf_v1_header(&frame) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let Some(hub) = ExecutionUserStreamHub::get(execution_id) else {
        return -SPEAR_ENOTCONN;
    };
    hub.mark_connected(stream_id);
    hub.push_inbound_frame(stream_id, frame)
}

pub fn ws_pop_any_outbound(execution_id: &str) -> Option<Vec<u8>> {
    let hub = ExecutionUserStreamHub::get(execution_id)?;
    hub.pop_outbound_frame_any().map(|(_, f)| f)
}

pub(crate) async fn ws_wait_any_outbound(execution_id: &str) {
    if let Some(hub) = ExecutionUserStreamHub::get(execution_id) {
        hub.notify_any_outbound.notified().await;
    } else {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spearlet::execution::host_api::ssf;

    #[test]
    fn test_parse_ssf_v1_header_ok() {
        let frame = ssf::build_ssf_v1_frame(7, 2, b"{}", b"hello");
        let (sid, ty) = ssf::parse_ssf_v1_header(&frame).unwrap();
        assert_eq!(sid, 7);
        assert_eq!(ty, 2);
    }

    #[test]
    fn test_parse_ssf_v1_header_rejects_bad_magic() {
        let mut frame = ssf::build_ssf_v1_frame(1, 2, b"{}", b"hello");
        frame[0] = b'X';
        assert_eq!(ssf::parse_ssf_v1_header(&frame).unwrap_err(), -SPEAR_EINVAL);
    }

    #[test]
    fn test_parse_ssf_v1_header_rejects_bad_version() {
        let mut frame = ssf::build_ssf_v1_frame(1, 2, b"{}", b"hello");
        frame[4] = 2;
        frame[5] = 0;
        assert_eq!(ssf::parse_ssf_v1_header(&frame).unwrap_err(), -SPEAR_EINVAL);
    }

    #[test]
    fn test_parse_ssf_v1_header_rejects_short_frame() {
        let frame = vec![0u8; 31];
        assert_eq!(ssf::parse_ssf_v1_header(&frame).unwrap_err(), -SPEAR_EINVAL);
    }

    #[test]
    fn test_parse_ssf_v1_header_rejects_invalid_header_len() {
        let mut frame = ssf::build_ssf_v1_frame(1, 2, b"{}", b"hello");
        frame[6] = 16;
        frame[7] = 0;
        assert_eq!(ssf::parse_ssf_v1_header(&frame).unwrap_err(), -SPEAR_EINVAL);
    }

    #[test]
    fn test_parse_ssf_v1_header_rejects_len_mismatch() {
        let mut frame = ssf::build_ssf_v1_frame(1, 2, b"{}", b"hello");
        let wrong = 1234u32.to_le_bytes();
        frame[24..28].copy_from_slice(&wrong);
        assert_eq!(ssf::parse_ssf_v1_header(&frame).unwrap_err(), -SPEAR_EINVAL);
    }

    #[test]
    fn test_ws_push_frame_rejects_invalid_frame() {
        let rc = ws_push_frame("exec-1", vec![1, 2, 3]);
        assert!(rc < 0);
    }

    #[test]
    fn test_ws_push_frame_rejects_unknown_execution() {
        let frame = ssf::build_ssf_v1_frame(1, 2, b"{}", b"hello");
        let rc = ws_push_frame("exec-unknown", frame);
        assert_eq!(rc, -SPEAR_ENOTCONN);
    }
}
