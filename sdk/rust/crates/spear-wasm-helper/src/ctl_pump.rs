//! Helpers for draining user-stream control events.
//! user stream 控制事件 draining 辅助模块。

use spear_wasm::{constants, user_stream_ctl_read_event, Fd, SpearError, UserStreamCtlEvent};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlEvent {
    StreamConnected(u32),
    SessionClosed,
}

pub fn drain_control_events(ctl_fd: Fd) -> Result<Vec<ControlEvent>, SpearError> {
    let mut events = Vec::new();
    loop {
        match user_stream_ctl_read_event(ctl_fd)? {
            Some(UserStreamCtlEvent { stream_id, kind })
                if kind == constants::SPEAR_USER_STREAM_CTL_EVENT_STREAM_CONNECTED as u32 =>
            {
                events.push(ControlEvent::StreamConnected(stream_id));
            }
            Some(UserStreamCtlEvent { kind, .. })
                if kind == constants::SPEAR_USER_STREAM_CTL_EVENT_SESSION_CLOSED as u32 =>
            {
                events.push(ControlEvent::SessionClosed);
            }
            Some(_) => {}
            None => return Ok(events),
        }
    }
}
