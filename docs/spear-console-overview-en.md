# SPEAR Console Overview

SPEAR Console is a lightweight, user-facing web UI served by SMS.

## Access

- URL: `http://<sms-host>:<sms-port>/console`

## Enable/Disable

SPEAR Console is enabled by default (served by SMS HTTP gateway under `/console`). You can disable it via:

- Config file: `enable_console = false`
- Environment: `SMS_ENABLE_CONSOLE=false`
- CLI: `--disable-console` (or `--enable-console`)

## UI Principles (Recommended)

- Keep chat as the primary surface: connection and debugging controls must not compete with the chat window.
- Progressive disclosure: default to a single “Connect” entry; show advanced details only when needed.
- Same-origin by default: avoid asking users to paste base URLs or opaque IDs.
- Clear state: always show Connected/Connecting/Disconnected and the last failure reason.
- Safe by default: limit concurrent requests, show rate-limit/backpressure errors explicitly.

## Layout (Recommended)

- Top bar: product title + a single connection chip (state + target) + `Connect` / `Disconnect`.
- Main area: the existing chat window (front-end user and back-end Agent continue to interact here).
- Side panel (drawer): connection picker and optional “Connection details”.

Wireframe: [spear-console-wireframe.svg](diagrams/spear-console-wireframe.svg)

## Connection Model

SPEAR Console uses the existing execution stream protocol:

1) User selects a Task -> Instance -> Execution in the “Start a chat” dialog.
2) Console calls `POST /api/v1/executions/{execution_id}/streams/session` (same-origin) to obtain `ws_url`.
3) Console connects to `ws_url` via WebSocket and exchanges SSF frames.
   - On `open`, Console sends an initial SSF v1 CTRL frame (`msgType=1`, empty data) to establish the stream (`stream_id=1`) without waiting for the first user input.
   - User input is sent as one or more DATA frames (`msgType=2`), followed by a COMMIT frame (`msgType=3`) to mark the end of the input segment.

### Multi-Client Concurrency (Same Execution)

- Supported: multiple WebSocket clients can connect to the same `execution_id` `ws_url` (through SMS, not directly to spearlet).
- Implementation: SMS keeps a single upstream WS to the spearlet execution and rewrites/routes `stream_id` per client to avoid collisions (most clients use `stream_id=1`).
- Note: connecting multiple clients directly to the same spearlet execution `/streams/ws` is not recommended (spearlet organizes channels by `stream_id` and outbound consumption is not multi-reader safe).

### Optional: Connect by Endpoint (Recommended)

If `endpoint` is configured for a task, Console can offer a second connect mode:

1) User selects an endpoint (searchable list of `endpoint`).
2) Console connects to `wss://<sms-host>:<sms-port>/e/{endpoint}/ws` and exchanges SSF frames.

The chat window remains unchanged; only the connection target changes.

## Information Display

SPEAR Console keeps the main chat surface focused and moves task/instance/execution “connection info” into an Info dialog (top-right in the selection dialog / header).

## Why Selection Dialog

SPEAR Console intentionally hides “SMS base URL” and assumes same-origin to simplify operation and reduce user error.

The selection dialog ensures users pick the correct instance/execution without manually copying opaque IDs.

## APIs Used (SMS HTTP)

- `GET /api/v1/tasks`
- `GET /api/v1/tasks/{task_id}/instances`
- `GET /api/v1/instances/{instance_id}/executions`
- `POST /api/v1/executions/{execution_id}/streams/session`

## Troubleshooting

### Console waits forever and SMS logs show 404 Not Found

Typical log:

- `register execution stream client failed ... connect upstream failed ... HTTP 404`

Meaning: SMS accepted the Console WebSocket, but failed to establish the upstream WebSocket to spearlet (`/api/v1/executions/{execution_id}/streams/ws`). A 404 usually means SMS connected to the wrong host/port.

Check first:

- Node registration fields: `node.ip_address` must not be `0.0.0.0/::`, and `node.http_port` must point to spearlet HTTP gateway.
- Spearlet advertised IP:
  - Local: set `SPEARLET_ADVERTISE_IP=127.0.0.1`
  - Kubernetes: inject `POD_IP` or set `SPEARLET_ADVERTISE_IP=<reachable pod/service IP>`
