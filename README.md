# SPEAR Next

SPEAR Next is the Rust/async implementation of SPEAR’s core services:

- **SMS**: the metadata/control-plane server.
- **SPEARlet**: the node-side agent/runtime.

Chinese README: [README.zh.md](./README.zh.md)

## Repository layout

- `src/apps/sms`: SMS binary entrypoint
- `src/apps/spearlet`: SPEARlet binary entrypoint
- `web-admin/`: Web Admin frontend source
- `assets/admin/`: built Web Admin static assets embedded/served by SMS
- `samples/wasm-c/`: C-based WASM samples (WASI)
- `docs/`: design notes and usage guides

## Architecture

![SPEAR architecture](docs/diagrams/spear-architecture.png)

## Quick start

### Prerequisites

- Rust toolchain (latest stable recommended)
- Docker (Docker Desktop on macOS/Windows, or Docker Engine on Linux)
- Docker Compose v2 (`docker compose`)

This repo uses `protoc-bin-vendored`, so you typically don’t need to install `protoc` manually.

### Run locally with Docker Compose (SMS + SPEARlet)

This is the recommended cross-platform local setup (no Kubernetes required). It runs:

- SMS (gRPC + HTTP gateway + optional Web Admin)
- SPEARlet (agent/runtime) connecting to SMS via the Compose network

Use the provided Compose file:

- `deploy/docker/compose.local.yaml`
- The local Compose stack builds SMS with the `rocksdb` feature enabled and persists both admin metadata and event KV under the `sms-data` volume.

Start:

```bash
docker compose -f deploy/docker/compose.local.yaml up -d --build
```

Useful endpoints (default host ports):

- SMS health: `http://127.0.0.1:18080/health`
- SMS Swagger: `http://127.0.0.1:18080/swagger-ui/`
- SPEAR Console (served by SMS): `http://127.0.0.1:18080/console`
- SMS Web Admin: `http://127.0.0.1:18082/`
- SPEARlet health: `http://127.0.0.1:18081/health`

Stop:

```bash
docker compose -f deploy/docker/compose.local.yaml down
```

Remove local data (volumes):

```bash
docker compose -f deploy/docker/compose.local.yaml down -v
```

Common build/network notes:

- If Docker Hub is not reachable, override base images via environment variables (example mirrors):

```bash
export NODE_IMAGE=docker.m.daocloud.io/library/node:20-bookworm-slim
export RUST_IMAGE=docker.m.daocloud.io/library/rust:1.91-bookworm
export DEBIAN_IMAGE=docker.m.daocloud.io/library/debian:trixie-slim
docker compose -f deploy/docker/compose.local.yaml up -d --build
```

- If you use Local AI Models (llama.cpp), SPEARlet needs `llama-server`. The Compose file defaults to a build target that includes it. You can override:

```bash
SPEARLET_BUILD_TARGET=runtime_with_node_and_llama docker compose -f deploy/docker/compose.local.yaml up -d --build spearlet
```

### Build

```bash
make build

# release
make build-release

# build with Rust features (e.g. sled / rocksdb)
make FEATURES=sled build

# enable local microphone capture implementation (optional)
make FEATURES=mic-device build

# macOS shortcut (equivalent to FEATURES+=mic-device)
make mac-build
```

### Run SMS

```bash
./target/debug/sms

# enable Web Admin
./target/debug/sms --enable-web-admin --web-admin-addr 127.0.0.1:8081
```

Useful endpoints:

- HTTP gateway: `http://127.0.0.1:8080`
- Swagger UI: `http://127.0.0.1:8080/swagger-ui/`
- OpenAPI spec: `http://127.0.0.1:8080/api/openapi.json`
- gRPC: `127.0.0.1:50051`
- Web Admin (when enabled): `http://127.0.0.1:8081/admin`

### Run SPEARlet

SPEARlet connects to SMS once you provide `--sms-grpc-addr` (then it auto-registers by default).

```bash
./target/debug/spearlet --sms-grpc-addr 127.0.0.1:50051
```

## Configuration

### Config file locations

- SMS: `~/.sms/config.toml` (or `--config <path>`)
- SPEARlet: `~/.spear/config.toml` (or `--config <path>`)

Repo-shipped examples:

- SMS: `config/sms/config.toml`
- SPEARlet: `config/spearlet/config.toml`

### Priority

1. CLI `--config` file
2. Home config (`~/.sms/config.toml` or `~/.spear/config.toml`)
3. Environment variables (`SMS_*`, `SPEARLET_*`)
4. Built-in defaults

### Secrets

Do not put secrets into config files. Use `spearlet.ai.credentials[].api_key_env` to reference environment variables and bind them from backends via `credential_ref`.

AI backend notes:

- `[[spearlet.ai.backends]] hosting` is required and must be `local` or `remote`.
- `credential_ref` is optional. If set, the referenced env var must exist (otherwise the backend is filtered). If not set, the backend is treated as “no-auth” (useful for self-hosted proxies).

### Ollama discovery

SPEARlet can import models from a local Ollama on startup and materialize them as AI backends.

- Docs: `docs/ollama-discovery-en.md`

## Routing and debugging

- **Route by model**: if some backends are configured with `model = "..."`, requests can be routed by setting only `model` (no explicit `backend` required).
- **Observe the selected backend**:
  - `cchat_recv` JSON includes a top-level `_spear.backend` / `_spear.model`.
  - Router emits a `router selected backend` debug log after selection.

## Web Admin

Web Admin provides Nodes, Tasks, Files, AI Backends, AI Models, Credentials, MCP, and Execution History pages.

- AI Backends is the control-plane write surface for backend definitions, placements, and credentials.
- AI Models provides a read-only aggregated view across nodes, split into Local and Remote.
- The AI Backends page now exposes separate create flows:
  - `Create Remote Backend`
  - `Create Local Backend`
- Placement is created during backend creation:
  - remote backends default to `All Nodes`
  - local backends default to `Single Node`
- Backend detail pages expose:
  - placements
  - per-node status
  - read-model views

Local model provisioning (`llamacpp`):

- `model` is the routing / display key; the node-local runtime uses metadata-backed runtime parameters.
- The Web Admin local dialog surfaces the most common `llamacpp` fields directly and persists them into backend metadata.
- Common fields:
  - `model_url`: http/https URL to a `.gguf` file.
  - `model_path`: absolute path, or relative to `spearlet.local_models_dir`.
  - `skip_download=1`: fail if the model file is missing instead of downloading.
  - `download_timeout_s`: total download budget in seconds (default: 3600).
  - `threads`: forwarded to `llama-server --threads`.
  - `ctx_size`: forwarded to `llama-server --ctx-size`.
- Advanced metadata keys still supported by runtime:
  - `server_mode`
  - `server_cmd`
  - `server_cmd_args`
  - `ready_probe`
  - `start_timeout_s`

Local `vllm` remains a scaffolded / external-endpoint-oriented path rather than a fully managed local process mode.

Docs:

- `docs/web-admin-overview-en.md`
- `docs/web-admin-ui-guide-en.md`

## WASM samples

```bash
make samples
```

Artifacts are written to `samples/build/` (WASM-C), `samples/build/js/` (WASM-JS), and `samples/build/rust/` (WASM-Rust).

Docs:

- `docs/samples-build-guide-en.md`
- `samples/README-en.md`
- `samples/wasm-js/README-en.md`
- `samples/wasm-rust/README-en.md`
- `sdk/rust/crates/spear-boa/README.md`
- `sdk/rust/crates/spear-wasm-helper/README.md`
- `docs/spear-console-voice-input-design-en.md`

## Development

```bash
make help
make dev
make ci
```

UI tests (Playwright):

```bash
make test-ui
```

## Documentation

- `docs/INDEX.md`

## License

Apache-2.0. See `LICENSE`.
