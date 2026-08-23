# User Stream Live Caption Cleanup

## Scope

This cleanup targets the Rust sample at `samples/wasm-rust/user_stream_live_caption`.
The goal is to improve readability without changing the runtime behavior or adding new abstraction layers.

## Changes

- Simplified pending audio aggregation in `rtasr_session.rs` from chunk collection plus byte bookkeeping to a single contiguous buffer.
- Removed merge-on-write behavior that was only serving the old chunk-based representation.
- Kept the commit flush guard, but renamed the internal state to better reflect its meaning.
- Replaced a `loop` plus nested `match` in `app.rs` with a direct `while let` drain loop.
- Extracted repeated caption-open logic into a single helper so transcript event handling is easier to read.

## Verification

- Existing unit tests continue to pass.
- Added focused tests for pending audio ordering and flush-state reset behavior.
- `cargo clippy --tests -- -D warnings` passes for the sample after the cleanup.
