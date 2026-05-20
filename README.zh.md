# SPEAR Next

SPEAR Next 是 SPEAR 核心服务的 Rust/async 实现：

- **SMS**：元数据/控制面服务。
- **SPEARlet**：节点侧代理与运行时。

English README: [README.md](./README.md)

## 目录结构

- `src/apps/sms`：SMS 二进制入口
- `src/apps/spearlet`：SPEARlet 二进制入口
- `web-admin/`：Web Admin 前端源码
- `assets/admin/`：构建后的 Web Admin 静态资源（由 SMS 内嵌/托管）
- `samples/wasm-c/`：基于 C 的 WASM 示例（WASI）
- `docs/`：设计与使用文档

## 架构示意图

![SPEAR 架构](docs/diagrams/spear-architecture.png)

## 快速开始

### 前置依赖

- Rust toolchain（建议使用最新 stable）
- Docker（macOS/Windows 使用 Docker Desktop，Linux 使用 Docker Engine）
- Docker Compose v2（`docker compose`）

说明：本项目使用 `protoc-bin-vendored`，通常无需手动安装 `protoc`。

### 使用 Docker Compose 本地运行（SMS + SPEARlet）

这是推荐的跨平台本地部署方式（无需 Kubernetes）。会启动：

- SMS（gRPC + HTTP 网关 + 可选 Web Admin）
- SPEARlet（节点 Agent/Runtime），通过 Compose 网络连接 SMS

使用仓库自带 Compose 文件：

- `deploy/docker/compose.local.yaml`

启动：

```bash
docker compose -f deploy/docker/compose.local.yaml up -d --build
```

常用地址（默认宿主机端口）：

- SMS health：`http://127.0.0.1:18080/health`
- SMS Swagger：`http://127.0.0.1:18080/swagger-ui/`
- SPEAR Console（由 SMS 托管）：`http://127.0.0.1:18080/console`
- SMS Web Admin：`http://127.0.0.1:18082/`
- SPEARlet health：`http://127.0.0.1:18081/health`

停止：

```bash
docker compose -f deploy/docker/compose.local.yaml down
```

清理本地数据（volumes）：

```bash
docker compose -f deploy/docker/compose.local.yaml down -v
```

常见构建/网络问题：

- 如果 Docker Hub 无法访问，可通过环境变量覆盖基础镜像（示例使用镜像站）：

```bash
export NODE_IMAGE=docker.m.daocloud.io/library/node:20-bookworm-slim
export RUST_IMAGE=docker.m.daocloud.io/library/rust:1.91-bookworm
export DEBIAN_IMAGE=docker.m.daocloud.io/library/debian:trixie-slim
docker compose -f deploy/docker/compose.local.yaml up -d --build
```

- 如果你要使用 Local AI Models（llama.cpp），SPEARlet 需要包含 `llama-server`。Compose 默认会使用包含 `llama-server` 的构建 target，也可以显式覆盖：

```bash
SPEARLET_BUILD_TARGET=runtime_with_node_and_llama docker compose -f deploy/docker/compose.local.yaml up -d --build spearlet
```

### 构建

```bash
make build

# release
make build-release

# 指定 Rust features（例如 sled / rocksdb）
make FEATURES=sled build

# 启用本机麦克风采集实现（可选）
make FEATURES=mic-device build

# macOS 便捷入口（等价于 FEATURES+=mic-device）
make mac-build
```

### 运行 SMS

```bash
./target/debug/sms

# 启用 Web Admin
./target/debug/sms --enable-web-admin --web-admin-addr 127.0.0.1:8081
```

常用地址：

- HTTP 网关：`http://127.0.0.1:8080`
- Swagger UI：`http://127.0.0.1:8080/swagger-ui/`
- OpenAPI：`http://127.0.0.1:8080/api/openapi.json`
- gRPC：`127.0.0.1:50051`
- Web Admin（启用后）：`http://127.0.0.1:8081/admin`

### 运行 SPEARlet

当提供 `--sms-grpc-addr` 后，SPEARlet 会连接 SMS 并默认自动注册。

```bash
./target/debug/spearlet --sms-grpc-addr 127.0.0.1:50051
```

## 配置

### 配置文件路径

- SMS：`~/.sms/config.toml`（或 `--config <path>`）
- SPEARlet：`~/.spear/config.toml`（或 `--config <path>`）

仓库内示例：

- SMS：`config/sms/config.toml`
- SPEARlet：`config/spearlet/config.toml`

### 配置优先级

1. CLI `--config`
2. 家目录配置（`~/.sms/config.toml` 或 `~/.spear/config.toml`）
3. 环境变量（`SMS_*`、`SPEARLET_*`）
4. 代码默认值

### 密钥/凭证

不要把密钥写入配置文件。使用 `spearlet.llm.credentials[].api_key_env` 引用环境变量，并在 backend 上通过 `credential_ref` 进行绑定。

LLM backend 注意事项：

- `[[spearlet.llm.backends]] hosting` 为必填，只允许 `local` 或 `remote`。
- `credential_ref` 为可选：配置后要求对应 env 存在（否则 backend 会被过滤）；不配置则视为“无需鉴权”（适用于自建代理等场景）。

### Ollama 模型导入

SPEARlet 支持在启动时从本机 Ollama 导入模型并生成对应的 LLM backend。

- 文档：`docs/ollama-discovery-zh.md`

## 路由与排障

- **按模型路由**：当某些 backend 配置了 `model = "..."` 时，guest 只设置 `model` 也能完成路由（无需显式指定 `backend`）。
- **如何确认最终路由到哪个 backend**：
  - `cchat_recv` 返回 JSON 顶层包含 `_spear.backend` / `_spear.model`
  - Router 在选中 backend 后会输出 `router selected backend` 的 debug 日志

## Web Admin

Web Admin 提供 Nodes/Tasks/Files/AI Models 等页面。

- AI Models 提供跨节点聚合视图，并区分 Local/Remote
- Local AI Models 支持在节点上创建/删除 model deployment

本地模型拉取（llamacpp）：

- `model` 只是展示用 key；当本地模型文件不存在时，实际下载取决于 `params.model_url`。
- 支持参数：
  - `model_url`：指向 `.gguf` 的 http/https URL（支持大文件）。
  - `download_timeout_s`：总下载超时预算（秒，默认 3600）。
  - `model_path`：绝对路径，或相对于 `spearlet.local_models_dir` 的相对路径。
  - `skip_download=1`：模型文件不存在时直接失败（不下载）。

文档：

- `docs/web-admin-overview-zh.md`
- `docs/web-admin-ui-guide-zh.md`

## WASM 示例

```bash
make samples
```

产物输出到 `samples/build/`（C）与 `samples/build/js/`（WASM-JS；兼容：`samples/build/rust/`）。

文档：

- `docs/samples-build-guide-zh.md`

## 开发

```bash
make help
make dev
make ci
```

UI 测试（Playwright）：

```bash
make test-ui
```

## 文档索引

- `docs/INDEX.md`

## License

Apache-2.0，见 `LICENSE`。
