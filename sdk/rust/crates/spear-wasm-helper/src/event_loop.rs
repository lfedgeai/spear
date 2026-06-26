//! Lightweight epoll driver for Rust-first guest apps.
//! Rust-first guest app 的轻量 epoll driver。

use spear_wasm::{constants, epoll_close, epoll_create, epoll_ctl, epoll_wait, EpollFd, SpearError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadyEvent {
    pub fd: i32,
    pub events: i32,
}

impl ReadyEvent {
    pub fn has_flag(self, flag: i32) -> bool {
        self.events & flag != 0
    }

    pub fn has_any_hup_or_err(self) -> bool {
        self.has_flag(constants::SPEAR_EPOLLHUP | constants::SPEAR_EPOLLERR)
    }
}

#[derive(Debug)]
pub struct EpollDriver {
    epoll_fd: Option<EpollFd>,
}

impl EpollDriver {
    pub fn new() -> Result<Self, SpearError> {
        Ok(Self {
            epoll_fd: Some(epoll_create()?),
        })
    }

    pub fn add(&self, fd: i32, mask: i32) -> Result<(), SpearError> {
        epoll_ctl(self.raw_fd(), constants::SPEAR_EPOLL_CTL_ADD, fd, mask)
    }

    pub fn remove(&self, fd: i32) {
        let _ = epoll_ctl(self.raw_fd(), constants::SPEAR_EPOLL_CTL_DEL, fd, 0);
    }

    pub fn wait(&self, timeout_ms: i32, initial_capacity: usize) -> Result<Vec<ReadyEvent>, SpearError> {
        Ok(epoll_wait(self.raw_fd(), timeout_ms, initial_capacity)?
            .into_iter()
            .map(|event| ReadyEvent {
                fd: event.fd,
                events: event.events,
            })
            .collect())
    }

    pub fn close(&mut self) -> Result<(), SpearError> {
        let Some(epoll_fd) = self.epoll_fd.take() else {
            return Ok(());
        };
        epoll_close(epoll_fd)
    }

    fn raw_fd(&self) -> EpollFd {
        self.epoll_fd.expect("epoll driver already closed")
    }
}
