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
   - On `open`, Console sends SSF v1 CTRL(OPEN) frames (`msgType=1`) for every `stream_id` it plans to use. DATA/COMMIT is not allowed before CTRL(OPEN) (no backward compatibility).
   - Text input (default `stream_id=1`): send one or more DATA frames (`msgType=2`), then a COMMIT frame (`msgType=3`) to mark the end of the input segment.
   - Voice input (optional `stream_id=2`): on press start send CTRL(UTTERANCE_BEGIN), keep sending DATA(audio chunks), and on release send COMMIT. Full spec: [spear-console-voice-input-design-en.md](./spear-console-voice-input-design-en.md).

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

## Shared Frontend Styling

- Console and Admin now share the same theme token source: [`theme.css`](../web-admin/src/shared/theme.css).
- Shared theme tokens now also cover semantic state and surface tokens used by Console, such as success/warning colors, overlay tone, and shadow color, so dark/light switching stays visually coherent beyond the base background/foreground pair.
- Console and Admin also share the same theme-mode application hook: `web-admin/src/shared/theme-mode.ts`, which keeps `class`, `data-theme`, `color-scheme`, and persisted theme state aligned.
- Console reuses Admin-oriented UI primitives from `web-admin/src/shared/components/ui/`, including shared `Button`, `Input`, `Textarea`, `Card`, and `Select`.
- Voice controls in Console should also reuse the shared button language where possible, so microphone actions, split controls, and mode menus stay visually aligned with Admin controls.
- `web-admin/src/index.css` and `web-console/src/index.css` both import the shared theme entry, so light/dark colors, typography baseline, spacing rhythm, and control height stay aligned.
- Header status chips should follow the shared radius language as well, avoiding pill-only shapes when surrounding controls use the Admin box radius.
- Auxiliary labels such as sidebar eyebrows, section labels, and panel captions should keep the same restrained weight and tracking rhythm as Admin secondary text.
- Section titles, panel titles, dialog titles, and supporting descriptions should share one typography baseline, instead of each Console area carrying its own local title scale.
- In practice, this means top-level Console titles should resolve to the same `text-sm / font-semibold / leading-5` baseline used by Admin header titles, while supporting descriptions should resolve to `text-xs / leading-4`.
- Body text, readonly values, status text, helper text, and badge text should avoid smaller local Console-only scales such as `11px`; they should resolve to the same shared `text-xs` or `text-sm` baselines used by Admin, with consistent line-height instead of ad hoc `1.45~1.65` ranges.
- Information-dense components such as readonly blocks, connection details, code blocks, endpoint rows, and chat bubbles should also reuse the same compact density rules, so their internal padding and text rhythm do not drift away from the Admin system.
- Spacing should prefer a compact shared rhythm built around a small set of steps such as `4 / 8 / 12`, instead of mixing many nearby one-off values across headers, cards, sections, and drawers.
- Console-specific layout classes in `web-console/src/main.css` should only handle page structure and product-specific interaction patterns; base control visuals should continue to come from the shared primitives whenever possible.
- As shared primitives expand, legacy control-specific utility classes in `web-console/src/main.css` should be removed instead of re-skinned in place.

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
