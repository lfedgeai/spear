//! Application state machine for the Rust live caption sample.
//! Rust 版实时字幕 sample 的应用状态机。

use std::collections::HashSet;

use spear_wasm_helper::{
    app_event::{classify_app_event, AppEvent},
    ctl_pump::{drain_control_events, ControlEvent},
    event_loop::{EpollDriver, ReadyEvent},
    rtasr_protocol::TranscriptEvent,
    stream::{BufferedTextOutput, StreamEndpoint, StreamModality},
    time_format::current_hms_prefix,
    user_stream_protocol::{
        is_audio_utterance_begin, is_open_handshake, parse_stream_message, IncomingStreamMessage,
        StreamMessageError,
    },
};
use spear_wasm::{
    constants, user_stream_ctl_open, user_stream_read_alloc, Fd, SpearError,
};

use crate::{
    config::{EPOLL_READY_CAPACITY, EPOLL_TIMEOUT_MS, STREAM_TEXT, STREAM_VOICE},
    rtasr_session::LiveCaptionRtasr,
};

fn debug_guest(message: impl std::fmt::Display) {
    eprintln!("[live_caption] {message}");
}

pub fn run() -> Result<(), String> {
    let mut app = LiveCaptionApp::new()?;
    let result = app.run_loop();
    app.close_all();
    result
}

struct LiveCaptionApp {
    epoll: EpollDriver,
    ctl_fd: Fd,
    text_stream: StreamEndpoint,
    voice_stream: StreamEndpoint,
    text_output: BufferedTextOutput,
    rtasr: Option<LiveCaptionRtasr>,
    caption_open: bool,
    warned: HashSet<&'static str>,
    session_closed: bool,
}

impl LiveCaptionApp {
    fn new() -> Result<Self, String> {
        let epoll = EpollDriver::new().map_err(render_error)?;
        let ctl_fd = user_stream_ctl_open().map_err(render_error)?;

        let app = Self {
            epoll,
            ctl_fd,
            text_stream: StreamEndpoint::new(STREAM_TEXT, StreamModality::Text),
            voice_stream: StreamEndpoint::new(STREAM_VOICE, StreamModality::Audio),
            text_output: BufferedTextOutput::new(crate::config::META_V1_JSON),
            rtasr: None,
            caption_open: false,
            warned: HashSet::new(),
            session_closed: false,
        };
        app.epoll
            .add(ctl_fd.raw(), constants::SPEAR_EPOLLIN | constants::SPEAR_EPOLLERR | constants::SPEAR_EPOLLHUP)
            .map_err(render_error)?;
        Ok(app)
    }

    fn run_loop(&mut self) -> Result<(), String> {
        while !self.session_closed {
            let events = self
                .epoll
                .wait(EPOLL_TIMEOUT_MS, EPOLL_READY_CAPACITY)
                .map_err(render_error)?;
            for event in events {
                self.dispatch_event(event)?;
                self.flush_text_output()?;
                if self.session_closed {
                    break;
                }
            }
        }
        Ok(())
    }

    fn dispatch_event(&mut self, event: ReadyEvent) -> Result<(), String> {
        match classify_app_event(
            event,
            self.ctl_fd.raw(),
            &[&self.text_stream, &self.voice_stream],
            self.rtasr.as_ref().map(|session| session.fd().raw()),
        ) {
            AppEvent::Control(ready) => self.handle_ctl_ready(ready),
            AppEvent::Stream {
                stream_id, ready, ..
            } if stream_id == self.text_stream.stream_id() => self.handle_text_stream_ready(ready),
            AppEvent::Stream {
                stream_id, ready, ..
            } if stream_id == self.voice_stream.stream_id() => self.handle_voice_stream_ready(ready),
            AppEvent::Rtasr(ready) => self.handle_rtasr_ready(ready),
            AppEvent::Unknown(_) | AppEvent::Stream { .. } => Ok(()),
        }
    }

    fn handle_ctl_ready(&mut self, event: ReadyEvent) -> Result<(), String> {
        if event.has_any_hup_or_err() {
            self.session_closed = true;
            return Ok(());
        }

        for control_event in drain_control_events(self.ctl_fd).map_err(render_error)? {
            match control_event {
                ControlEvent::StreamConnected(stream_id) => self.handle_stream_connected(stream_id)?,
                ControlEvent::SessionClosed => {
                    self.session_closed = true;
                    return Ok(());
                }
            }
        }
        Ok(())
    }

    fn handle_stream_connected(&mut self, stream_id: u32) -> Result<(), String> {
        if stream_id == self.text_stream.stream_id() && !self.text_stream.is_connected() {
            debug_guest(format!("stream_connected text stream_id={stream_id}"));
            let _ = self
                .text_stream
                .attach_runtime_fd(&self.epoll, true)
                .map_err(render_error)?;
        } else if stream_id == self.voice_stream.stream_id() && !self.voice_stream.is_connected() {
            debug_guest(format!("stream_connected voice stream_id={stream_id}"));
            let _ = self
                .voice_stream
                .attach_runtime_fd(&self.epoll, false)
                .map_err(render_error)?;
        }
        Ok(())
    }

    fn handle_text_stream_ready(&mut self, event: ReadyEvent) -> Result<(), String> {
        if event.has_flag(constants::SPEAR_EPOLLHUP) {
            self.close_text_stream();
            return Ok(());
        }

        if event.has_flag(constants::SPEAR_EPOLLIN) {
            // Drain every queued control frame before returning to epoll.
            // 在返回 epoll 前，尽量清空当前已排队的控制帧。
            while let Some(fd) = self.text_stream.fd() {
                match user_stream_read_alloc(fd) {
                    Ok(Some(frame)) => {
                        if let Ok(IncomingStreamMessage::Ctrl(ctrl)) = parse_stream_message(&frame) {
                            if is_open_handshake(&ctrl, self.text_stream.modality()) {
                                self.text_stream.mark_protocol_opened();
                            }
                        }
                    }
                    Ok(None) => break,
                    Err(err) => return Err(render_error(err)),
                }
            }
        }

        if event.has_flag(constants::SPEAR_EPOLLOUT) {
            self.flush_text_output()?;
        }
        Ok(())
    }

    fn handle_voice_stream_ready(&mut self, event: ReadyEvent) -> Result<(), String> {
        if event.has_flag(constants::SPEAR_EPOLLHUP) {
            self.close_voice_stream();
            return Ok(());
        }
        if !event.has_flag(constants::SPEAR_EPOLLIN) {
            return Ok(());
        }

        loop {
            let Some(fd) = self.voice_stream.fd() else {
                return Ok(());
            };
            match user_stream_read_alloc(fd) {
                Ok(Some(frame)) => self.handle_voice_frame(&frame)?,
                Ok(None) => return Ok(()),
                Err(err) => return Err(render_error(err)),
            }
        }
    }

    fn handle_voice_frame(&mut self, frame: &[u8]) -> Result<(), String> {
        let message = match parse_stream_message(frame) {
            Ok(message) => message,
            Err(StreamMessageError::InvalidDataMeta) => {
                self.warn_once("voice_bad_meta", "protocol: voice DATA meta invalid; dropping frame\n")?;
                return Ok(());
            }
            Err(StreamMessageError::InvalidCommitMeta) => {
                self.warn_once("voice_bad_commit_meta", "protocol: voice COMMIT meta invalid; dropping frame\n")?;
                return Ok(());
            }
            Err(StreamMessageError::InvalidFrame | StreamMessageError::InvalidCtrlMeta | StreamMessageError::UnsupportedType) => {
                return Ok(());
            }
        };

        if !self.voice_stream.can_write_data() {
            match &message {
                IncomingStreamMessage::Ctrl(ctrl) if is_open_handshake(ctrl, self.voice_stream.modality()) => {
                    self.voice_stream.mark_protocol_opened();
                }
                _ => self.warn_once("voice_missing_open", "protocol: voice stream missing CTRL(open); dropping frame\n")?,
            }
            return Ok(());
        }

        match message {
            IncomingStreamMessage::Ctrl(ctrl) => {
                if is_open_handshake(&ctrl, self.voice_stream.modality()) {
                    debug_guest("voice ctrl open handshake");
                    return Ok(());
                }
                if is_audio_utterance_begin(&ctrl) {
                    debug_guest("voice ctrl utterance begin");
                    self.begin_utterance()?;
                }
            }
            IncomingStreamMessage::Data { meta, data } => {
                let _meta = meta;
                if let Some(session) = self.rtasr.as_mut() {
                    debug_guest(format!("voice data bytes={}", data.len()));
                    session.append_audio(&data).map_err(render_error)?;
                }
            }
            IncomingStreamMessage::Commit { meta } => {
                let _meta = meta;
                if let Some(session) = self.rtasr.as_mut() {
                    debug_guest("voice commit");
                    session.flush_commit().map_err(render_error)?;
                }
            }
        }
        Ok(())
    }

    fn handle_rtasr_ready(&mut self, event: ReadyEvent) -> Result<(), String> {
        if event.has_any_hup_or_err() {
            self.close_rtasr();
            return Ok(());
        }
        if !event.has_flag(constants::SPEAR_EPOLLIN) {
            return Ok(());
        }

        let Some(session) = self.rtasr.as_mut() else {
            return Ok(());
        };
        debug_guest("rtasr drain events");
        let transcript_events = session.drain_events().map_err(render_error)?;
        for event in transcript_events {
            match event {
                TranscriptEvent::SpeechStarted => {
                    self.open_caption_if_needed()?;
                }
                TranscriptEvent::Delta(delta) => {
                    self.open_caption_if_needed()?;
                    if !delta.is_empty() {
                        self.text_output.write(delta);
                    }
                }
                TranscriptEvent::Completed(_) => {
                    if self.caption_open {
                        self.text_output.write("\n");
                        self.text_output.commit();
                        self.caption_open = false;
                    }
                }
            }
            self.flush_text_output()?;
        }
        Ok(())
    }

    fn open_caption_if_needed(&mut self) -> Result<(), String> {
        if self.caption_open {
            return Ok(());
        }
        self.caption_open = true;
        self.text_output
            .write(&current_hms_prefix().map_err(render_error)?);
        Ok(())
    }

    fn begin_utterance(&mut self) -> Result<(), String> {
        if let Some(session) = self.rtasr.as_mut() {
            debug_guest("begin_utterance existing session");
            return session.prepare_for_new_utterance().map_err(render_error);
        }

        debug_guest("begin_utterance creating session");
        let session = LiveCaptionRtasr::connect().map_err(render_error)?;
        let fd = session.fd().raw();
        debug_guest(format!("begin_utterance session fd={fd}"));
        self.epoll
            .add(fd, constants::SPEAR_EPOLLIN | constants::SPEAR_EPOLLERR | constants::SPEAR_EPOLLHUP)
            .map_err(render_error)?;
        self.rtasr = Some(session);
        Ok(())
    }

    fn warn_once(&mut self, key: &'static str, message: &'static str) -> Result<(), String> {
        if self.warned.insert(key) {
            self.text_output.write(message);
            self.flush_text_output()?;
        }
        Ok(())
    }

    fn flush_text_output(&mut self) -> Result<(), String> {
        self.text_output
            .flush(self.text_stream.as_stream())
            .map_err(render_error)
    }

    fn close_text_stream(&mut self) {
        let _ = self.text_stream.close_with_epoll(&self.epoll);
    }

    fn close_voice_stream(&mut self) {
        let _ = self.voice_stream.close_with_epoll(&self.epoll);
    }

    fn close_rtasr(&mut self) {
        if let Some(session) = self.rtasr.take() {
            self.epoll.remove(session.fd().raw());
            let _ = session.close();
        }
    }

    fn close_all(&mut self) {
        self.close_rtasr();
        self.close_text_stream();
        self.close_voice_stream();
        let _ = spear_wasm::user_stream_close(self.ctl_fd);
        let _ = self.epoll.close();
    }
}

fn render_error(err: SpearError) -> String {
    err.to_string()
}
