# Task Events Subscriber

## Overview

SPEARlet includes a streaming Task Events subscriber that connects to the SMS `EventsService`, subscribes to the global task unified event stream, and filters task lifecycle events from `sms.TaskEvent` payloads. It persists a cursor to ensure resumable processing across restarts and handles automatic reconnection with configurable backoff.

## Components

- `TaskEventSubscriber`: Maintains configuration and the last processed event sequence cursor.
- Node UUID derivation: Uses `node_name` if it is a valid UUID; otherwise derives a stable UUIDv5 from `grpc.addr`, `grpc.port`, and `node_name`.
- Cursor persistence: Stores `after_seq` under `storage.data_dir` with filename pattern `task_events_cursor_{node_uuid}.json`.

## Key Behaviors

- Connects to SMS via `sms_grpc_addr` using gRPC and subscribes to `EventsService.SubscribeEvents` with a `ResourceType::Task` selector and `after_seq`.
- Streams events; on each event:
  - Updates `after_seq` in memory and persists to disk.
  - Only handles unified events where `resource_type == Task` and the payload decodes as `sms.TaskEvent`.
  - For `Create` events, fetches task details via `GetTask` and materializes the task locally.
  - For `Update` events, re-fetches the task snapshot from SMS and refreshes local materialization.
  - For `Cancel` events, triggers local task runtime cleanup and deletion acknowledgment.
- No longer filters by task-level `node_uuid`; every SPEARlet receives task control-plane events and only cleans up its own local task/instance state.
- Automatically reconnects on stream or connection errors, waiting `sms_connect_retry_ms` between attempts.

## Configuration

Relevant `SpearletConfig` fields:

```toml
[spearlet]
node_name = "spearlet-node"
sms_grpc_addr = "127.0.0.1:50051"
auto_register = true
heartbeat_interval = 30
cleanup_interval = 300
sms_connect_timeout_ms = 15000
sms_connect_retry_ms = 500
reconnect_total_timeout_ms = 300000

[spearlet.grpc]
addr = "0.0.0.0:50052"

[spearlet.http]
cors_enabled = true
swagger_enabled = true

[spearlet.storage]
backend = "memory"
data_dir = "./data/spearlet"
```

Environment variables supported: `SPEARLET_SMS_ADDR`, `SPEARLET_SMS_CONNECT_TIMEOUT_MS`, `SPEARLET_SMS_CONNECT_RETRY_MS`, `SPEARLET_RECONNECT_TOTAL_TIMEOUT_MS`, `SPEARLET_STORAGE_DATA_DIR`.

## Usage

Start the subscriber during SPEARlet initialization:

```rust
use std::sync::Arc;
use spear_next::spearlet::{config::SpearletConfig, task_events::TaskEventSubscriber};

let config = Arc::new(SpearletConfig::default());
let subscriber = TaskEventSubscriber::new(config.clone());
subscriber.start().await; // runs in background
```

The subscriber persists a cursor to `storage.data_dir`, enabling resubscription from the last processed event after restart.

## Error Handling & Resilience

- Retries connection on failure with `sms_connect_retry_ms` delay.
- Handles stream errors gracefully, resubscribing after delay.
- Does not depend on task-level `node_uuid` routing; local cleanup only applies to task/instance state materialized on the current node.
- Cursor file directory is created on demand if missing.

## Testing

- Cursor roundtrip test: `src/spearlet/task_events_test.rs` verifies `store_cursor` and `load_cursor` behavior.
- Integration tests should simulate SMS event streaming and reconnection scenarios.

## Code References

- `src/spearlet/task_events.rs:44` — subscriber startup and reconnect loop
- `src/spearlet/task_events.rs:76` — event handling and task fetch on Create
- `src/spearlet/config.rs:259` — default configuration values

---

This document ensures alignment of documentation with the latest Task Events subscriber implementation.
