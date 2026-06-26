//! Rust live caption sample entrypoint.
//! Rust 版实时字幕 sample 入口。

mod app;
mod config;
mod rtasr_session;

fn main() {
    if let Err(err) = app::run() {
        eprintln!("{err}");
        std::process::exit(1);
    }
}
