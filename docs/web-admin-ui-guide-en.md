# Web Admin UI Guide

This document explains how to use the current `spear-next` Web Admin, covering nodes, files, tasks, AI backends, and related control-plane workflows.

## Access & Auth

- Enable: run SMS with `--enable-web-admin --web-admin-addr 127.0.0.1:8081`
- URL: `http://127.0.0.1:8081/admin`
- Admin Token:
  - Enter in Settings and click `Save`
  - Token is stored in `localStorage('ADMIN_TOKEN')`

## Top Settings

- Theme: light/dark

## Nodes

- List supports search, time sorting, pagination, and details modal
- SSE: backend `GET /admin/api/nodes/stream` for live updates
- Toolbar includes search and refresh

## Files

- Choose files: click `Choose files` (native `<input type="file" multiple>`)
- Upload: click `Upload`; success toast shows completion
- Actions:
  - `Download` the object
  - `Copy URI` copies `smsfile://<id>`
  - `Delete` removes and refreshes list (React Query invalidate + local filter)

## Task Creation (Tasks → Create Task)

- The dialog is grouped into:
  - `Task Basics`
  - `MCP tools`
  - `Routing`
  - `Executable`
- Long dialogs use a sticky header, scrollable body, and fixed footer actions.
- Executable type: `No Executable | Binary | Script | Container | WASM | Process`
- Executable URI accepts `smsfile://<id>` for embedded file artifacts
- Parameters: `Capabilities` (comma), `Args` (comma), `Env` (`key=value` per line)
 - MCP tools (optional):
   - Enable MCP tools and pick per-task servers from the MCP registry
   - “Default” servers map to `Task.config["mcp.default_server_ids"]`
   - “Allowed” servers map to `Task.config["mcp.allowed_server_ids"]` (upper bound)
   - Tool filters map to `Task.config["mcp.tool_allowlist"]` / `["mcp.tool_denylist"]`

## AI Models

- AI Models page is split into `Local` and `Remote`
- List supports search and availability filtering (available/unavailable)
- Click a row to navigate to the model detail page and inspect per-node instances
- AI Models is read-only. Create, edit, place, enable, disable, and delete backends from `AI Backends`.
- Remote AI Models includes a shortcut to the shared credentials page at `AI Backends → Credentials`

## AI Backends

- The page is the write surface for backend definitions, placements, and credentials.
- The top-level view can switch between:
  - `Backends`
  - `Model Views`
- Creation is split into two explicit flows:
  - `Create Remote Backend`
  - `Create Local Backend`
- Editing remains available from each backend row.

### Create Remote Backend

- Use this for OpenAI-, Ollama-, or other external endpoint-backed adapters.
- Common fields:
  - `Display name`
  - `Provider`
  - `Model`
  - `Backend kind`
  - `Base URL`
  - `Credential ref`
  - `Operations`
  - `Features`
  - `Transports`
  - `Placement`
- For supported remote providers such as `openai`, `openai_compatible`, and `ollama`, Web Admin now runs node-side preflight before creation.
- The preflight is executed from the selected target SPEARlet node instead of from the browser or SMS process.
- For multi-node placement, the default policy is sampled strict verification before create, followed by full node-side verification during post-create placement reconciliation.
- If endpoint reachability, credential validation, or model access fails on the sampled node set, creation stops immediately with the node-side error.

### Create Local Backend

- Use this for node-local runtimes such as `llamacpp` and `vllm`.
- Placement defaults to `Single Node`.
- For `local + llamacpp`, the UI surfaces runtime fields directly instead of requiring manual JSON editing:
  - `Model URL`
  - `Model Path`
  - `Skip Download`
  - `Download Timeout (s)`
  - `Threads`
  - `Context Size`
- These fields are persisted back into backend metadata for the current runtime implementation.
- When `Model URL` is used, Web Admin now runs a node-side preflight before creation.
- The preflight is executed from the selected target node, not from the browser or SMS process.
- If the target node cannot access the URL, creation fails immediately and returns the node-side error.

### Placement

- Placement is configured during backend creation.
- Supported scopes:
  - `All Nodes`
  - `Single Node`
  - `Selected Nodes`
- `All Nodes` creation does not block on synchronous validation of every node by default. Instead it samples up to three nodes before create and lets the assignment controller complete full verification asynchronously on each node afterward.
- Per-placement overrides:
  - `Weight override`
  - `Priority override`

### Backend Detail Page

- The detail page shows:
  - backend summary
  - placements
  - node status
  - read-model views

## Credentials

- Entry: `AI Backends` → `Credentials`
- Use this page to create, rotate, disable, and delete reusable secret refs for AI backends
- Credentials are shared control-plane resources and are no longer nested under the AI Models area

## Known Issues & Fixes

- None tracked in this doc; use issues in repo.

## Related Docs

- [web-admin-overview-en.md](./web-admin-overview-en.md) for the current page and API surface
- [backend-support-matrix-en.md](./backend-support-matrix-en.md) for supported backend kinds, providers, and operations
- [ai-backend-unified-control-plane-design-en.md](./ai-backend-unified-control-plane-design-en.md) for the control-plane model behind AI Backends / AI Models
- [ui-tests-guide-en.md](./ui-tests-guide-en.md) for frontend test guidance
- [ollama-discovery-en.md](./ollama-discovery-en.md) for Ollama-specific discovery behavior
