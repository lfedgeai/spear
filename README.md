# SPEAR Next

SPEAR Next is the Rust/async implementation of SPEAR’s core platform.
It provides a control plane, node runtime, browser console, and admin UI for running and operating task/execution workloads.

Chinese README: [README.zh.md](./README.zh.md)

## What SPEAR Is

SPEAR is designed for scenarios where you need to:

- register and manage tasks centrally
- run workloads on node-side runtimes
- expose browser-facing endpoints and live sessions
- operate files, executions, and AI backends from a web UI

In this repository, the two core runtime services are:

- **SMS**: control plane, metadata service, HTTP/gRPC gateway, Console host, Web Admin host
- **SPEARlet**: node agent and execution runtime that connects to SMS and runs workloads

## Architecture

![SPEAR architecture](docs/diagrams/spear-architecture.png)

Typical request path:

1. A user connects through Console, API, or endpoint gateway.
2. SMS resolves metadata, tasks, routing, and session state.
3. SPEARlet executes the workload on a node.
4. Results, streams, logs, and status flow back through SMS.

## First-Time Setup

### Prerequisites

- Docker
- Docker Compose v2 via `docker compose`

For most first-time users, Docker Compose is the recommended way to start.

## Run With Docker Compose

### HTTP Deployment

Start the default local stack:

```bash
docker compose -f deploy/docker/compose.local.yaml up -d --build
```

Default URLs:

- Console: `http://127.0.0.1:18080/console`
- SMS API / health: `http://127.0.0.1:18080/health`
- SMS Swagger: `http://127.0.0.1:18080/swagger-ui/`
- Web Admin: `http://127.0.0.1:18082/`
- SPEARlet health: `http://127.0.0.1:18081/health`
- Debug Server: `http://127.0.0.1:17777/`

Stop the stack:

```bash
docker compose -f deploy/docker/compose.local.yaml down
```

Remove local data:

```bash
docker compose -f deploy/docker/compose.local.yaml down -v
```

### HTTPS Deployment

SPEAR supports a local HTTPS overlay for browser-facing pages.
The current Compose setup keeps internal service traffic on HTTP and terminates TLS at Caddy, which is the recommended local deployment pattern in this repository.

Start HTTP + HTTPS together:

```bash
docker compose \
  -f deploy/docker/compose.local.yaml \
  -f deploy/docker/compose.https.yaml \
  up -d --build
```

Default HTTPS URLs:

- Web Admin: `https://127.0.0.1:18443/admin`
- Console: `https://127.0.0.1:18444/console`
- Debug Server: `https://127.0.0.1:18445/`

Detailed HTTPS notes:

- [deploy/docker/README-https.md](./deploy/docker/README-https.md)

### What The Compose Stack Starts

- `sms`: control plane and browser-facing gateway
- `spearlet`: node runtime connected to SMS
- `debug-server`: runtime debug log viewer
- `https-proxy`: optional Caddy-based TLS entrypoint when `compose.https.yaml` is included

## Where To Start In The UI

After the stack is up:

- Open **Console** to verify browser access and endpoint sessions
- Open **Web Admin** to inspect nodes, tasks, files, executions, and AI backends

If you are using SPEAR for the first time, the usual flow is:

1. Start the Compose stack
2. Open Web Admin
3. Upload files or register tasks
4. Open Console and connect to an endpoint or execution

## Repository Map

These paths matter most for first-time exploration:

- `src/apps/sms`: SMS binary entrypoint
- `src/apps/spearlet`: SPEARlet binary entrypoint
- `src/debug_server`: Rust debug server
- `web-admin/`: admin frontend source
- `web-console/`: console frontend source
- `deploy/docker/`: local Docker Compose deployment files
- `docs/`: architecture, usage, and design documentation
- `samples/`: WASM and streaming examples

## Key Docs

- Architecture overview: [docs/project-architecture-overview-en.md](./docs/project-architecture-overview-en.md)
- Docs index: [docs/INDEX.md](./docs/INDEX.md)
- Web Admin overview: [docs/web-admin-overview-en.md](./docs/web-admin-overview-en.md)
- Web Admin guide: [docs/web-admin-ui-guide-en.md](./docs/web-admin-ui-guide-en.md)
- Console overview: [docs/spear-console-overview-en.md](./docs/spear-console-overview-en.md)
- Samples guide: [docs/samples-build-guide-en.md](./docs/samples-build-guide-en.md)
- HTTPS deployment: [deploy/docker/README-https.md](./deploy/docker/README-https.md)

## License

Apache-2.0. See [LICENSE](./LICENSE).
