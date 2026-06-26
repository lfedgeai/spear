//! Logical app-event classification for Rust-first guest apps.
//! Rust-first guest app 的逻辑事件分类辅助模块。

use crate::{
    event_loop::ReadyEvent,
    stream::{StreamEndpoint, StreamModality},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppEvent {
    Control(ReadyEvent),
    Stream {
        stream_id: u32,
        modality: StreamModality,
        ready: ReadyEvent,
    },
    Rtasr(ReadyEvent),
    Unknown(ReadyEvent),
}

pub fn classify_app_event(
    event: ReadyEvent,
    ctl_fd_raw: i32,
    streams: &[&StreamEndpoint],
    rtasr_fd_raw: Option<i32>,
) -> AppEvent {
    if event.fd == ctl_fd_raw {
        return AppEvent::Control(event);
    }

    for stream in streams {
        if stream.matches_fd(event.fd) {
            return AppEvent::Stream {
                stream_id: stream.stream_id(),
                modality: stream.modality(),
                ready: event,
            };
        }
    }

    if rtasr_fd_raw == Some(event.fd) {
        return AppEvent::Rtasr(event);
    }

    AppEvent::Unknown(event)
}
