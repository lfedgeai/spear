# SPEAR Next

SPEAR Next 是 SPEAR 核心平台的 Rust/async 实现。
它提供控制面、节点运行时、浏览器 Console，以及 Web Admin，用于运行和管理 task / execution 类工作负载。

English README: [README.md](./README.md)

## SPEAR 是什么

SPEAR 适合下面这类场景：

- 在中心控制面注册和管理任务
- 在节点侧运行 workload
- 通过浏览器访问 endpoint、会话和流式结果
- 通过 Web UI 运维文件、执行记录和 AI backend

这个仓库里的两个核心服务是：

- **SMS**：控制面、元数据服务、HTTP/gRPC 网关、Console 托管服务、Web Admin 托管服务
- **SPEARlet**：节点侧 agent 与执行运行时，连接到 SMS 并实际运行 workload

## 架构

![SPEAR 架构](docs/diagrams/spear-architecture.png)

一个典型请求链路是：

1. 用户从 Console、API 或 endpoint gateway 发起请求
2. SMS 负责元数据、任务、路由和会话状态
3. SPEARlet 在节点上执行 workload
4. 结果、流式数据、日志和状态再回到 SMS

## 首次上手

### 前置依赖

- Docker
- Docker Compose v2，也就是 `docker compose`

对于第一次接触 SPEAR 的使用者，最推荐的方式就是直接用 Docker Compose 启动。

## 用 Docker Compose 运行

### HTTP 部署

启动默认本地栈：

```bash
docker compose -f deploy/docker/compose.local.yaml up -d --build
```

默认访问地址：

- Console：`http://127.0.0.1:18080/console`
- SMS API / health：`http://127.0.0.1:18080/health`
- SMS Swagger：`http://127.0.0.1:18080/swagger-ui/`
- Web Admin：`http://127.0.0.1:18082/`
- SPEARlet health：`http://127.0.0.1:18081/health`
- Debug Server：`http://127.0.0.1:17777/`

停止：

```bash
docker compose -f deploy/docker/compose.local.yaml down
```

清理本地数据：

```bash
docker compose -f deploy/docker/compose.local.yaml down -v
```

### HTTPS 部署

SPEAR 支持通过本地 HTTPS overlay 暴露浏览器页面。
当前仓库推荐的本地模式是：内部服务仍走 HTTP，由 Caddy 做 TLS 终止。这也是这里的最佳实践实现方式。

同时启动 HTTP 基础栈和 HTTPS overlay：

```bash
docker compose \
  -f deploy/docker/compose.local.yaml \
  -f deploy/docker/compose.https.yaml \
  up -d --build
```

默认 HTTPS 地址：

- Web Admin：`https://127.0.0.1:18443/admin`
- Console：`https://127.0.0.1:18444/console`
- Debug Server：`https://127.0.0.1:18445/`

HTTPS 详细说明：

- [deploy/docker/README-https.md](./deploy/docker/README-https.md)

### Compose 会启动哪些服务

- `sms`：控制面和浏览器入口
- `spearlet`：节点运行时
- `debug-server`：运行时调试日志查看服务
- `https-proxy`：当叠加 `compose.https.yaml` 时启用的 Caddy TLS 入口

## 启动后先看什么

服务启动后，建议先访问：

- **Console**：确认浏览器入口和 endpoint/session 是否可连接
- **Web Admin**：查看 nodes、tasks、files、executions、AI backends

第一次使用 SPEAR 的推荐顺序是：

1. 启动 Compose 栈
2. 打开 Web Admin
3. 上传文件或注册任务
4. 打开 Console，连接 endpoint 或 execution

## 仓库里最值得先看的目录

- `src/apps/sms`：SMS 二进制入口
- `src/apps/spearlet`：SPEARlet 二进制入口
- `src/debug_server`：Rust 版 debug server
- `web-admin/`：管理端前端源码
- `web-console/`：控制台前端源码
- `deploy/docker/`：本地 Docker Compose 部署文件
- `docs/`：架构、使用和设计文档
- `samples/`：WASM 与流式示例

## 关键文档

- 架构总览：[docs/project-architecture-overview-zh.md](./docs/project-architecture-overview-zh.md)
- 文档索引：[docs/INDEX.md](./docs/INDEX.md)
- Web Admin 概览：[docs/web-admin-overview-zh.md](./docs/web-admin-overview-zh.md)
- Web Admin 使用指南：[docs/web-admin-ui-guide-zh.md](./docs/web-admin-ui-guide-zh.md)
- Console 概览：[docs/spear-console-overview-zh.md](./docs/spear-console-overview-zh.md)
- Samples 构建指南：[docs/samples-build-guide-zh.md](./docs/samples-build-guide-zh.md)
- HTTPS 部署说明：[deploy/docker/README-https.md](./deploy/docker/README-https.md)

## License

Apache-2.0，见 [LICENSE](./LICENSE)。
