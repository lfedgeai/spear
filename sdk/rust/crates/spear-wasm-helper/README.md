# spear-wasm-helper

Higher-level Rust helpers built on top of `spear-wasm`.

## Purpose

`spear-wasm-helper` sits between the low-level `spear-wasm` hostcall wrappers and concrete WASM samples or apps.

Use this crate when you want:

- reusable `user stream` state handling
- reusable `SSF v1` / stream-protocol parsing
- reusable RTASR transcript event parsing
- a base RTASR session skeleton for app-level orchestration

Do **not** use this crate as a place for sample-specific state machines or product behavior.

## Layering

- `spear-wasm-sys`: raw hostcall bindings
- `spear-wasm`: safe low-level Rust wrappers
- `spear-wasm-helper`: reusable higher-level building blocks for Rust-on-WASM apps
- `samples/wasm-rust/*`: concrete app state machines and behavior

## Modules

- `time_format`: second-precision `[HH:MM:SS]` prefix formatting
- `ctl_pump`: draining helpers for `user_stream_ctl_*` events
- `event_loop`: a lightweight `EpollDriver` and ready-event wrapper
- `app_event`: classify low-level ready events into logical `AppEvent`s
- `stream`: `ManagedStream`, `StreamEndpoint`, `BufferedTextOutput`
- `user_stream_protocol`: `SSF v1` / `CTRL(open)` / `utterance_begin` parsing helpers
- `rtasr_protocol`: transcript event parsing helpers
- `rtasr_session`: `BaseRtasrSession`, `RtasrConnectOptions`

## Good Fits

- Rust-first WASM samples
- Small Rust guest apps running inside Spear
- Reusable guest-side orchestration helpers that are more opinionated than `spear-wasm`, but still generic

## Non-Goals

- Product-specific flows such as `live_caption` or `voice_chat`
- Downstream chat orchestration
- UI formatting policy outside generic text-stream buffering
