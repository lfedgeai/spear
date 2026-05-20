# SPEAR Helm 部署指南

本文档说明如何构建 SPEAR 的容器镜像，并使用 Helm 在 Kubernetes 上部署 SPEAR 集群。

## 组件说明

- SMS：元数据/控制面服务（gRPC + HTTP）。
- SPEARlet：节点 Agent / Worker（gRPC + HTTP）。
- Router Filter：由 SMS 提供的 gRPC 服务（默认内置；预留未来插件扩展点）。

## 前置条件

- Kubernetes 集群
- Helm v3
- 可用的镜像仓库（用于 push 镜像）

## 构建镜像

### 方案 A：单一统一镜像

构建一个同时包含 `sms` 与 `spearlet` 二进制的统一镜像：

```bash
docker build -f deploy/docker/spear/Dockerfile -t <REGISTRY>/spear:<TAG> .
```

可选：为 SPEARlet 相关场景构建包含 Node（以及 llama-server）的镜像：

```bash
docker build -f deploy/docker/spear/Dockerfile --target runtime_with_node -t <REGISTRY>/spear:<TAG> .
docker build -f deploy/docker/spear/Dockerfile --target runtime_with_node_and_llama -t <REGISTRY>/spear:<TAG> .
```

推送镜像：

```bash
docker push <REGISTRY>/spear:<TAG>
```

### 方案 B：拆分镜像

拆分为两个镜像（当你希望镜像更小、依赖范围更收敛时更合适）：

```bash
docker build -f deploy/docker/sms/Dockerfile -t <REGISTRY>/spear-sms:<TAG> .
docker build -f deploy/docker/spearlet/Dockerfile -t <REGISTRY>/spear-spearlet:<TAG> .
```

Cargo registry 说明：

- 这些 Dockerfile 默认使用 Cargo 镜像源（`rsproxy.cn`），主要是为了在某些网络环境下提升依赖下载的稳定性。
- 如果你在中国大陆以外、或者网络访问 crates.io 很稳定，可以关掉镜像源：

```bash
docker build -f deploy/docker/spear/Dockerfile -t <REGISTRY>/spear:<TAG> --build-arg USE_CARGO_MIRROR=0 .
docker build -f deploy/docker/sms/Dockerfile -t <REGISTRY>/spear-sms:<TAG> --build-arg USE_CARGO_MIRROR=0 .
docker build -f deploy/docker/spearlet/Dockerfile -t <REGISTRY>/spear-spearlet:<TAG> --build-arg USE_CARGO_MIRROR=0 .
```

推送镜像：

```bash
docker push <REGISTRY>/spear:<TAG>
docker push <REGISTRY>/spear-sms:<TAG>
docker push <REGISTRY>/spear-spearlet:<TAG>
```

## 使用 Helm 安装

Chart 路径：

- `deploy/helm/spear`

安装命令：

```bash
helm upgrade --install spear deploy/helm/spear \
  --set global.image.repository=<REGISTRY>/spear \
  --set global.image.tag=<TAG>
```

或者（拆分镜像）：

```bash
helm upgrade --install spear deploy/helm/spear \
  --set sms.image.repository=<REGISTRY>/spear-sms \
  --set sms.image.tag=<TAG> \
  --set spearlet.image.repository=<REGISTRY>/spear-spearlet \
  --set spearlet.image.tag=<TAG>
```

## Kind 本地快速验证

如果你想用 kind 在本地验证 chart，可以不推送镜像到仓库，直接加载本机镜像到 kind 节点。

```bash
kind create cluster --name spear

docker build -f deploy/docker/spear/Dockerfile -t spear:local .
kind load docker-image --name spear spear:local

helm upgrade --install spear deploy/helm/spear -n spear --create-namespace \
  --set global.image.repository=spear \
  --set global.image.tag=local

kubectl -n spear get pods -o wide
kubectl -n spear wait --for=condition=Ready pod -l app.kubernetes.io/instance=spear --timeout=300s
```

拆分镜像：

```bash
kind create cluster --name spear

docker build -f deploy/docker/sms/Dockerfile -t spear-sms:local .
docker build -f deploy/docker/spearlet/Dockerfile -t spear-spearlet:local .

kind load docker-image --name spear spear-sms:local spear-spearlet:local

helm upgrade --install spear deploy/helm/spear -n spear --create-namespace \
  --set sms.image.repository=spear-sms --set sms.image.tag=local \
  --set spearlet.image.repository=spear-spearlet --set spearlet.image.tag=local

kubectl -n spear get pods -o wide
kubectl -n spear wait --for=condition=Ready pod -l app.kubernetes.io/instance=spear --timeout=300s
```

健康检查：

```bash
kubectl -n spear port-forward pod/spear-spear-sms-0 18080:8080
curl -fsS http://127.0.0.1:18080/health && echo

kubectl -n spear port-forward pod/spear-spear-spearlet-xxxxx 18081:8081
curl -fsS http://127.0.0.1:18081/health && echo
```

## 常用配置

### 配置 OpenAI backend（Helm values + Kubernetes Secret）

最佳实践：

- 不要把 OpenAI API key 写进 ConfigMap 或 Helm values（避免进 Git、避免被渲染到 manifest 里）。
- 把 key 放到 Kubernetes Secret（或外部 Secret 系统）里，通过环境变量注入到 SPEARlet。
- 通过 Helm values 配置 `spearlet.llm.credentials` + `credential_ref` 与 `spearlet.llm.backends`。

配置注意事项：

- `spearlet.config.llm.backends` 下每个 backend 都必须配置 `hosting`，且只允许 `local` 或 `remote`。

#### 1) 通过 Secret 提供 OpenAI API key

方式 A：直接创建 Kubernetes Secret（简单、手动）。

```bash
kubectl -n spear create secret generic openai-api-key \
  --from-literal=OPENAI_API_KEY='***'
```

方式 B：生产推荐使用外部 Secret 系统（例如 ESO / Vault / 云厂商 Secret Manager），同步到 Kubernetes Secret，名称为 `openai-api-key`，字段为 `OPENAI_API_KEY`。

#### 2) 用 values 覆盖文件配置 SPEARlet 的 LLM

仓库已提供示例文件（不包含 secret 明文）：

- `deploy/helm/spear/values-openai.yaml`

你也可以复制一份到自己的部署仓库中，再按需调整：

```yaml
spearlet:
  config:
    llm:
      defaultPolicy: weighted_random
      credentials:
        - name: openai_chat
          kind: env
          apiKeyEnv: OPENAI_API_KEY
      backends:
        - name: openai-chat
          kind: openai_chat_completion
          baseUrl: https://api.openai.com/v1
          model: gpt-4o-mini
          credentialRef: openai_chat
          hosting: remote
          weight: 100
          priority: 0
          ops: [chat_completions]
          features: [supports_tools, supports_json_schema]
          transports: [http]

  extraEnv:
    - name: OPENAI_API_KEY
      valueFrom:
        secretKeyRef:
          name: openai-api-key
          key: OPENAI_API_KEY
```

#### 3) 部署 / 升级

```bash
helm upgrade --install spear deploy/helm/spear -n spear --create-namespace \
  -f deploy/helm/spear/values-openai.yaml \
  --set sms.image.repository=<REGISTRY>/spear-sms --set sms.image.tag=<TAG> \
  --set spearlet.image.repository=<REGISTRY>/spear-spearlet --set spearlet.image.tag=<TAG>
```

#### 4) kind 一键本地集群（可选）

如果你希望快速在本机创建一个可用于验证 OpenAI backend 的 kind 集群，并且自动把本机的 `OPENAI_API_KEY` 注入到集群内的 Secret：

```bash
OPENAI_API_KEY=... ./scripts/kind-openai-quickstart.sh
```

相关文档：

- [LLM credential_ref 设计与落地](./implementation/llm-credentials-implementation-zh.md)
- [Backends 配置模型](./backend-adapter/backends-zh.md)

### Web Admin 文件上传目录（只读根文件系统注意事项）

Helm chart 默认启用 `readOnlyRootFilesystem: true`。因此，Web Admin 的文件上传不能写到容器根目录下的相对路径。

当前 SMS 会将上传文件写入 `files_dir`（独立配置）：

- 本地默认值：`./data/files`
- Helm chart 默认值：`/var/lib/spear/files`（同一个 PVC 会额外挂载到该目录，确保可写）

如需覆盖上传目录，可以：

- 在 `config.toml` 中设置 `files_dir = "/some/writable/path"`
- 或通过环境变量 `SMS_FILES_DIR` 设置

### 启用 SMS Web Admin

```bash
helm upgrade --install spear deploy/helm/spear \
  --set sms.config.enableWebAdmin=true
```

访问 Web Admin：

```bash
kubectl -n spear port-forward svc/spear-spear-sms 18082:8081
```

然后在浏览器打开：

- http://127.0.0.1:18082/

### 启用 Router Filter

默认情况下，Router Filter 服务端由 SMS 提供（内置，默认 no-op）。如需让 SPEARlet 在路由阶段发起过滤调用，可启用：

```bash
helm upgrade --install spear deploy/helm/spear \
  --set routerFilter.enabled=true
```

如需覆盖 Router Filter 服务端地址（host:port），可设置：

```bash
helm upgrade --install spear deploy/helm/spear \
  --set routerFilter.enabled=true \
  --set routerFilter.addr="<HOST>:<PORT>"
```

### 启用 SPEARlet Kubernetes runtime 的 RBAC

SPEARlet 的 Kubernetes runtime 会在容器内调用 `kubectl` 创建 Job/Pod，需要开启 RBAC：

```bash
helm upgrade --install spear deploy/helm/spear \
  --set spearlet.config.kubernetesRuntime.enabled=true \
  --set spearlet.rbac.create=true
```

注意：

- 当前提供的 `deploy/docker/spearlet/Dockerfile` 侧重本地验证，默认不内置 `kubectl`，以减少镜像构建时对外网依赖。
- 若生产环境需要 Kubernetes runtime，请先构建一个包含 `kubectl` 的 SPEARlet 镜像（或通过挂载方式提供 `kubectl`），再开启该能力。

## 卸载

```bash
helm uninstall spear
```
