# SMS And Spearlet Cleanup

## Scope

This cleanup targets two low-risk readability improvements in the main Rust workspace:

- `src/sms/services/resource_service.rs`
- `src/spearlet/object_service.rs`

The intent is to remove dead surface area and reduce repeated internal logic without changing external behavior.

## Changes

- Removed three unused `NodeResourceInfo` helper methods from `resource_service.rs`:
  - `update_metadata`
  - `get_memory_usage_bytes`
  - `get_available_disk_bytes`
- Centralized unix timestamp generation in `object_service.rs` so object write paths no longer duplicate the same time calculation.
- Consolidated object statistics aggregation behind a single internal scan helper, reducing repeated scan-and-deserialize logic in:
  - `object_count`
  - `total_object_size`
  - `pinned_object_count`
  - `get_stats`
- Added shared object load/save helpers in `object_service.rs` so `put/get/ref-count/pin/unpin/delete` no longer duplicate KV fetch, deserialize, serialize, and store boilerplate.
- Added a focused test that verifies `get_stats()` tracks both total object size and pinned object count.
- Added a focused overwrite test that locks in the existing behavior where overwrite updates payload fields while preserving reference count and pin state.

## Verification

- `cargo test resource_service --lib`
- `cargo test object_service --lib -- --nocapture`

`cargo clippy --lib -- -D warnings` still reports pre-existing workspace warnings outside the changed files, so this cleanup was validated with targeted tests instead of a full warning-clean workspace run.
