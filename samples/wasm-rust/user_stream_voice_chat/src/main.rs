//! Rust voice chat sample entrypoint.
//! Rust 版语音对话 sample 入口。

mod app;
mod chat_session;
mod config;
mod rtasr_session;

fn main() {
    if let Err(err) = app::run() {
        eprintln!("{err}");
        std::process::exit(1);
    }
}
