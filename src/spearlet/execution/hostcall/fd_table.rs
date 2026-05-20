use crate::spearlet::execution::host_api::errno::{EBADF, EINVAL};
use crate::spearlet::execution::hostcall::types::{FdEntry, FdFlags, FdInner, FdKind, PollEvents};
use dashmap::DashMap;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Arc, Mutex};

pub const EP_CTL_ADD: i32 = 1;
pub const EP_CTL_MOD: i32 = 2;
pub const EP_CTL_DEL: i32 = 3;

pub const FD_CTL_SET_FLAGS: i32 = 1;
pub const FD_CTL_GET_FLAGS: i32 = 2;
pub const FD_CTL_GET_KIND: i32 = 3;
pub const FD_CTL_GET_STATUS: i32 = 4;
pub const FD_CTL_GET_METRICS: i32 = 5;

#[derive(Debug)]
pub struct FdTable {
    next_fd: AtomicI32,
    entries: DashMap<i32, Arc<Mutex<FdEntry>>>,
}

impl FdTable {
    pub fn new(start_fd: i32) -> Self {
        Self {
            next_fd: AtomicI32::new(start_fd),
            entries: DashMap::new(),
        }
    }

    pub fn alloc(&self, entry: FdEntry) -> i32 {
        let fd = self.next_fd.fetch_add(1, Ordering::Relaxed);
        self.entries.insert(fd, Arc::new(Mutex::new(entry)));
        fd
    }

    pub fn get(&self, fd: i32) -> Option<Arc<Mutex<FdEntry>>> {
        self.entries.get(&fd).map(|e| e.clone())
    }

    pub fn close(&self, fd: i32) -> i32 {
        let Some(entry) = self.get(fd) else {
            return -EBADF;
        };

        let (kind, watchers, epoll_state) = {
            let mut e = entry.lock().unwrap();
            if e.closed {
                return 0;
            }
            e.closed = true;
            e.poll_mask.insert(PollEvents::HUP);

            if e.kind == FdKind::Mic {
                if let FdInner::Mic(st) = &mut e.inner {
                    st.running = false;
                    st.generation = st.generation.wrapping_add(1);
                }
            }

            let watchers = e.watchers.iter().copied().collect::<Vec<_>>();
            let epoll_state = match &e.inner {
                FdInner::Epoll(st) => Some(st.clone()),
                _ => None,
            };
            (e.kind, watchers, epoll_state)
        };

        for epfd in watchers {
            self.notify_epoll(epfd);
        }

        if kind == FdKind::Epoll {
            if let Some(st) = epoll_state {
                st.mark_closed();
                let watched = st.list_watched_fds();
                for wfd in watched {
                    self.unregister_watcher(wfd, fd);
                }
            }
        }

        0
    }

    pub fn close_all(&self) {
        let fds = self.entries.iter().map(|e| *e.key()).collect::<Vec<_>>();
        for fd in fds {
            let _ = self.close(fd);
        }
    }

    pub fn register_watcher(&self, fd: i32, epfd: i32) -> i32 {
        let Some(entry) = self.get(fd) else {
            return -EBADF;
        };
        let mut e = entry.lock().unwrap();
        e.watchers.insert(epfd);
        0
    }

    pub fn unregister_watcher(&self, fd: i32, epfd: i32) {
        let Some(entry) = self.get(fd) else {
            return;
        };
        let mut e = entry.lock().unwrap();
        e.watchers.remove(&epfd);
    }

    pub fn notify_watchers(&self, fd: i32) {
        let Some(entry) = self.get(fd) else {
            return;
        };
        let watchers = {
            let e = entry.lock().unwrap();
            e.watchers.iter().copied().collect::<Vec<_>>()
        };
        for epfd in watchers {
            self.notify_epoll(epfd);
        }
    }

    pub fn notify_epoll(&self, epfd: i32) {
        let Some(entry) = self.get(epfd) else {
            return;
        };
        let st = {
            let e = entry.lock().unwrap();
            match &e.inner {
                FdInner::Epoll(st) => Some(st.clone()),
                _ => None,
            }
        };
        if let Some(st) = st {
            st.notify();
        }
    }

    pub fn ep_create(&self) -> i32 {
        self.alloc(FdEntry {
            kind: FdKind::Epoll,
            flags: FdFlags::default(),
            poll_mask: PollEvents::default(),
            watchers: HashSet::new(),
            closed: false,
            inner: FdInner::Epoll(Arc::new(
                crate::spearlet::execution::hostcall::types::EpollState::new(),
            )),
        })
    }

    pub fn ep_ctl(&self, epfd: i32, op: i32, fd: i32, events: PollEvents) -> i32 {
        if epfd == fd {
            return -EINVAL;
        }
        let Some(ep_entry) = self.get(epfd) else {
            return -EBADF;
        };
        let Some(fd_entry) = self.get(fd) else {
            return -EBADF;
        };

        let st = {
            let ep = ep_entry.lock().unwrap();
            if ep.kind != FdKind::Epoll || ep.closed {
                return -EBADF;
            }
            match &ep.inner {
                FdInner::Epoll(st) => st.clone(),
                _ => return -EBADF,
            }
        };

        {
            let fd_e = fd_entry.lock().unwrap();
            if fd_e.kind == FdKind::Epoll {
                return -EINVAL;
            }
        }

        match op {
            EP_CTL_ADD => {
                st.ctl_add(fd, events);
                self.register_watcher(fd, epfd)
            }
            EP_CTL_MOD => {
                st.ctl_mod(fd, events);
                0
            }
            EP_CTL_DEL => {
                st.ctl_del(fd);
                self.unregister_watcher(fd, epfd);
                0
            }
            _ => -EINVAL,
        }
    }

    pub fn ep_wait_ready(&self, epfd: i32, timeout_ms: i32) -> Result<Vec<(i32, i32)>, i32> {
        let Some(ep_entry) = self.get(epfd) else {
            return Err(-EBADF);
        };
        let st = {
            let ep = ep_entry.lock().unwrap();
            if ep.kind != FdKind::Epoll || ep.closed {
                return Err(-EBADF);
            }
            match &ep.inner {
                FdInner::Epoll(st) => st.clone(),
                _ => return Err(-EBADF),
            }
        };

        let start = std::time::Instant::now();
        let timeout = if timeout_ms < 0 {
            None
        } else {
            Some(std::time::Duration::from_millis(timeout_ms as u64))
        };

        loop {
            let watch = st.snapshot_watch();
            let mut ready: Vec<(i32, i32)> = Vec::new();
            for (fd, interests) in watch {
                let Some(entry) = self.get(fd) else {
                    continue;
                };
                let mask = {
                    let e = entry.lock().unwrap();
                    e.effective_poll_mask().and(interests)
                };
                if !mask.is_empty() {
                    ready.push((fd, mask.bits() as i32));
                }
            }

            ready.sort_by_key(|(fd, _)| *fd);
            ready.dedup_by_key(|(fd, _)| *fd);

            if !ready.is_empty() {
                return Ok(ready);
            }

            if timeout_ms == 0 {
                return Ok(Vec::new());
            }

            let last_seq = st.current_seq();
            let remaining = timeout.map(|t| {
                let elapsed = start.elapsed();
                t.saturating_sub(elapsed)
            });
            if let Some(r) = remaining {
                if r.is_zero() {
                    return Ok(Vec::new());
                }
            }
            let notified = st.wait_for_change(last_seq, remaining);
            if !notified {
                return Ok(Vec::new());
            }
        }
    }

    pub fn fd_ctl(
        &self,
        fd: i32,
        cmd: i32,
        payload: Option<&[u8]>,
    ) -> Result<Option<Vec<u8>>, i32> {
        let Some(entry) = self.get(fd) else {
            return Err(-EBADF);
        };

        match cmd {
            FD_CTL_SET_FLAGS => {
                let bytes = payload.ok_or(-EINVAL)?;
                let v: Value = serde_json::from_slice(bytes).map_err(|_| -EINVAL)?;
                let set = v
                    .get("set")
                    .and_then(|x| x.as_array())
                    .cloned()
                    .unwrap_or_default();
                let clear = v
                    .get("clear")
                    .and_then(|x| x.as_array())
                    .cloned()
                    .unwrap_or_default();

                let mut e = entry.lock().unwrap();
                for s in set {
                    if s.as_str() == Some("O_NONBLOCK") {
                        e.flags.insert(FdFlags::O_NONBLOCK);
                    }
                }
                for s in clear {
                    if s.as_str() == Some("O_NONBLOCK") {
                        e.flags.remove(FdFlags::O_NONBLOCK);
                    }
                }
                Ok(None)
            }
            FD_CTL_GET_FLAGS => {
                let e = entry.lock().unwrap();
                let mut flags: Vec<&str> = Vec::new();
                if e.flags.contains(FdFlags::O_NONBLOCK) {
                    flags.push("O_NONBLOCK");
                }
                Ok(Some(
                    serde_json::to_vec(&json!({"flags": flags})).unwrap_or_else(|_| b"{}".to_vec()),
                ))
            }
            FD_CTL_GET_KIND => {
                let e = entry.lock().unwrap();
                let kind = match e.kind {
                    FdKind::ChatSession => "ChatSession",
                    FdKind::ChatResponse => "ChatResponse",
                    FdKind::Epoll => "Epoll",
                    FdKind::RtAsr => "RtAsr",
                    FdKind::Mic => "Mic",
                    FdKind::UserStream => "UserStream",
                    FdKind::UserStreamCtl => "UserStreamCtl",
                };
                Ok(Some(
                    serde_json::to_vec(&json!({"kind": kind})).unwrap_or_else(|_| b"{}".to_vec()),
                ))
            }
            FD_CTL_GET_STATUS => {
                let e = entry.lock().unwrap();
                let kind = match e.kind {
                    FdKind::ChatSession => "ChatSession",
                    FdKind::ChatResponse => "ChatResponse",
                    FdKind::Epoll => "Epoll",
                    FdKind::RtAsr => "RtAsr",
                    FdKind::Mic => "Mic",
                    FdKind::UserStream => "UserStream",
                    FdKind::UserStreamCtl => "UserStreamCtl",
                };
                let mut flags: Vec<&str> = Vec::new();
                if e.flags.contains(FdFlags::O_NONBLOCK) {
                    flags.push("O_NONBLOCK");
                }
                let mask = e.effective_poll_mask();
                let mut poll: Vec<&str> = Vec::new();
                if mask.intersects(PollEvents::IN) {
                    poll.push("EPOLLIN");
                }
                if mask.intersects(PollEvents::OUT) {
                    poll.push("EPOLLOUT");
                }
                if mask.intersects(PollEvents::ERR) {
                    poll.push("EPOLLERR");
                }
                if mask.intersects(PollEvents::HUP) {
                    poll.push("EPOLLHUP");
                }
                Ok(Some(
                    serde_json::to_vec(&json!({
                        "kind": kind,
                        "flags": flags,
                        "poll_mask": poll,
                        "closed": e.closed
                    }))
                    .unwrap_or_else(|_| b"{}".to_vec()),
                ))
            }
            FD_CTL_GET_METRICS => {
                let e = entry.lock().unwrap();
                match &e.inner {
                    FdInner::ChatResponse(r) => {
                        if r.metrics_bytes.is_empty() {
                            Ok(Some(b"{}".to_vec()))
                        } else {
                            Ok(Some(r.metrics_bytes.clone()))
                        }
                    }
                    FdInner::UserStream(st) => {
                        let ch = st.channel.lock().unwrap();
                        let v = json!({
                            "execution_id": st.execution_id.clone(),
                            "stream_id": st.stream_id,
                            "direction": format!("{:?}", st.direction),
                            "conn_state": format!("{:?}", ch.conn_state),
                            "inbound_len": ch.inbound.len(),
                            "inbound_bytes": ch.inbound_bytes,
                            "max_inbound_bytes": ch.max_inbound_bytes,
                            "outbound_len": ch.outbound.len(),
                            "outbound_bytes": ch.outbound_bytes,
                            "max_outbound_bytes": ch.max_outbound_bytes,
                            "max_frame_bytes": ch.max_frame_bytes,
                        });
                        Ok(Some(
                            serde_json::to_vec(&v).unwrap_or_else(|_| b"{}".to_vec()),
                        ))
                    }
                    FdInner::UserStreamCtl(st) => {
                        let v = json!({
                            "execution_id": st.execution_id.clone(),
                            "pending_len": st.pending.len(),
                            "max_pending": st.max_pending,
                        });
                        Ok(Some(
                            serde_json::to_vec(&v).unwrap_or_else(|_| b"{}".to_vec()),
                        ))
                    }
                    _ => Ok(Some(b"{}".to_vec())),
                }
            }
            _ => Err(-EINVAL),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spearlet::execution::hostcall::types::{
        ChatResponseState, EpollState, FdEntry, FdFlags, FdInner, FdKind, PollEvents,
    };
    use std::collections::HashSet;
    use std::sync::Arc;

    #[test]
    fn test_ep_wait_sorted_dedup() {
        let table = FdTable::new(1000);
        let epfd = table.ep_create();
        let fd1 = table.alloc(FdEntry {
            kind: FdKind::ChatResponse,
            flags: FdFlags::default(),
            poll_mask: PollEvents::IN,
            watchers: HashSet::new(),
            closed: false,
            inner: FdInner::ChatResponse(ChatResponseState::default()),
        });
        let fd2 = table.alloc(FdEntry {
            kind: FdKind::ChatResponse,
            flags: FdFlags::default(),
            poll_mask: PollEvents::IN,
            watchers: HashSet::new(),
            closed: false,
            inner: FdInner::ChatResponse(ChatResponseState::default()),
        });

        assert_eq!(table.ep_ctl(epfd, EP_CTL_ADD, fd2, PollEvents::IN), 0);
        assert_eq!(table.ep_ctl(epfd, EP_CTL_ADD, fd1, PollEvents::IN), 0);

        let ready = table.ep_wait_ready(epfd, 0).unwrap();
        assert_eq!(ready.len(), 2);
        assert!(ready[0].0 < ready[1].0);
        assert!(ready.iter().any(|(fd, _)| *fd == fd1));
        assert!(ready.iter().any(|(fd, _)| *fd == fd2));
    }

    #[test]
    fn test_ep_wait_timeout_empty() {
        let table = FdTable::new(1000);
        let epfd = table.ep_create();
        let fd = table.alloc(FdEntry {
            kind: FdKind::ChatResponse,
            flags: FdFlags::default(),
            poll_mask: PollEvents::EMPTY,
            watchers: HashSet::new(),
            closed: false,
            inner: FdInner::ChatResponse(ChatResponseState::default()),
        });
        assert_eq!(table.ep_ctl(epfd, EP_CTL_ADD, fd, PollEvents::IN), 0);
        let ready = table.ep_wait_ready(epfd, 5).unwrap();
        assert!(ready.is_empty());
    }

    #[test]
    fn test_ep_wait_wakeup_on_notify() {
        let table = Arc::new(FdTable::new(1000));
        let epfd = table.ep_create();
        let fd = table.alloc(FdEntry {
            kind: FdKind::ChatResponse,
            flags: FdFlags::default(),
            poll_mask: PollEvents::EMPTY,
            watchers: HashSet::new(),
            closed: false,
            inner: FdInner::ChatResponse(ChatResponseState::default()),
        });
        assert_eq!(table.ep_ctl(epfd, EP_CTL_ADD, fd, PollEvents::IN), 0);

        let t2 = table.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(10));
            if let Some(entry) = t2.get(fd) {
                let mut e = entry.lock().unwrap();
                e.poll_mask.insert(PollEvents::IN);
            }
            t2.notify_watchers(fd);
        });

        let ready = table.ep_wait_ready(epfd, 200).unwrap();
        assert!(ready
            .iter()
            .any(|(rfd, ev)| *rfd == fd && ((*ev as u32) & PollEvents::IN.bits()) != 0));
    }

    #[test]
    fn test_ep_close_unregisters_watchers() {
        let table = FdTable::new(1000);
        let ep_state = Arc::new(EpollState::new());
        let epfd = table.alloc(FdEntry {
            kind: FdKind::Epoll,
            flags: FdFlags::default(),
            poll_mask: PollEvents::EMPTY,
            watchers: HashSet::new(),
            closed: false,
            inner: FdInner::Epoll(ep_state),
        });
        let fd = table.alloc(FdEntry {
            kind: FdKind::ChatResponse,
            flags: FdFlags::default(),
            poll_mask: PollEvents::EMPTY,
            watchers: HashSet::new(),
            closed: false,
            inner: FdInner::ChatResponse(ChatResponseState::default()),
        });
        assert_eq!(table.ep_ctl(epfd, EP_CTL_ADD, fd, PollEvents::IN), 0);
        assert_eq!(table.close(epfd), 0);

        let entry = table.get(fd).unwrap();
        let e = entry.lock().unwrap();
        assert!(!e.watchers.contains(&epfd));
    }
}
