# SPEAR Helm Deployment Guide

This guide describes how to build SPEAR container images and deploy a SPEAR cluster on Kubernetes using Helm.

## Components

- SMS: metadata/control-plane service (gRPC + HTTP).
- SPEARlet: node agent / worker (gRPC + HTTP).
- Router Filter: a gRPC service provided by SMS (built-in by default; reserved for future plugin extensions).

## Prerequisites

- Kubernetes cluster
- Helm v3
- A container registry you can push to

## Build images

### Option A: single unified image

Build one image that contains both `sms` and `spearlet` binaries:

```bash
docker build -f deploy/docker/spear/Dockerfile -t <REGISTRY>/spear:<TAG> .
```

Optional: build with Node (and llama-server) included for SPEARlet-related workflows:

```bash
docker build -f deploy/docker/spear/Dockerfile --target runtime_with_node -t <REGISTRY>/spear:<TAG> .
docker build -f deploy/docker/spear/Dockerfile --target runtime_with_node_and_llama -t <REGISTRY>/spear:<TAG> .
```

Push:

```bash
docker push <REGISTRY>/spear:<TAG>
```

### Option B: split images

Build two images (useful when you want smaller images and tighter dependency scope):

```bash
docker build -f deploy/docker/sms/Dockerfile -t <REGISTRY>/spear-sms:<TAG> .
docker build -f deploy/docker/spearlet/Dockerfile -t <REGISTRY>/spear-spearlet:<TAG> .
```

Local path dependency note:

- `spear-next` depends on `spear-ssf` via a local `path` dependency (`sdk/rust/crates/spear-ssf`).
- The Dockerfiles explicitly copy this SDK crate into the build stage. If you add more local `path` dependencies, update the Dockerfiles accordingly.

Cargo registry note:

- The Dockerfiles use a Cargo registry mirror by default (`rsproxy.cn`) to improve reliability in some network environments.
- If you are outside mainland China (or your network can access crates.io reliably), you can disable it:

```bash
docker build -f deploy/docker/spear/Dockerfile -t <REGISTRY>/spear:<TAG> --build-arg USE_CARGO_MIRROR=0 .
docker build -f deploy/docker/sms/Dockerfile -t <REGISTRY>/spear-sms:<TAG> --build-arg USE_CARGO_MIRROR=0 .
docker build -f deploy/docker/spearlet/Dockerfile -t <REGISTRY>/spear-spearlet:<TAG> --build-arg USE_CARGO_MIRROR=0 .
```

Push them:

```bash
docker push <REGISTRY>/spear:<TAG>
docker push <REGISTRY>/spear-sms:<TAG>
docker push <REGISTRY>/spear-spearlet:<TAG>
```

## Install with Helm

The chart is located at:

- `deploy/helm/spear`

Install:

```bash
helm upgrade --install spear deploy/helm/spear \
  --set global.image.repository=<REGISTRY>/spear \
  --set global.image.tag=<TAG>
```

Or (split images):

```bash
helm upgrade --install spear deploy/helm/spear \
  --set sms.image.repository=<REGISTRY>/spear-sms \
  --set sms.image.tag=<TAG> \
  --set spearlet.image.repository=<REGISTRY>/spear-spearlet \
  --set spearlet.image.tag=<TAG>
```

## Kind quick test

If you want to validate the chart locally with kind, you can avoid pushing images to a registry.

Unified image:

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

Split images:

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

Health checks:

```bash
kubectl -n spear port-forward pod/spear-spear-sms-0 18080:8080
curl -fsS http://127.0.0.1:18080/health && echo

kubectl -n spear port-forward pod/spear-spear-spearlet-xxxxx 18081:8081
curl -fsS http://127.0.0.1:18081/health && echo
```

## Common settings

### Configure OpenAI backend (Helm values + Kubernetes Secret)

Best practice:

- Keep OpenAI API keys out of ConfigMaps and Helm values files.
- Put the key in Kubernetes Secret (or an external secret system), and inject it as an environment variable.
- Configure `spearlet.ai.credentials` + `credential_ref` and `spearlet.ai.backends` via Helm values.

Config note:

- For every backend item under `spearlet.config.ai.backends`, `hosting` is required and must be `local` or `remote`.

#### 1) Provide the OpenAI API key via Secret

Option A: create a Kubernetes Secret (simple, manual).

```bash
kubectl -n spear create secret generic openai-api-key \
  --from-literal=OPENAI_API_KEY='***'
```

Option B: use an external secret system (recommended for production), and sync to a Kubernetes Secret named `openai-api-key` with key `OPENAI_API_KEY`.

#### 2) Configure SPEARlet AI via a values override file

This repo provides a ready-to-use example file (no secret plaintext included):

- `deploy/helm/spear/values-openai.yaml`

You can also copy it to your deployment repo and adjust it as needed:

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

#### 3) Deploy / upgrade

```bash
helm upgrade --install spear deploy/helm/spear -n spear --create-namespace \
  -f deploy/helm/spear/values-openai.yaml \
  --set sms.image.repository=<REGISTRY>/spear-sms --set sms.image.tag=<TAG> \
  --set spearlet.image.repository=<REGISTRY>/spear-spearlet --set spearlet.image.tag=<TAG>
```

#### 4) One-command kind cluster (optional)

If you want a quick local kind cluster to validate the OpenAI backend, and automatically inject your local `OPENAI_API_KEY` into the cluster as a Secret:

```bash
OPENAI_API_KEY=... ./scripts/kind-openai-quickstart.sh
```

Related docs:

- [LLM credentials implementation](./implementation/llm-credentials-implementation-en.md)
- [Backends config model](./backend-adapter/backends-en.md)

### Web Admin file uploads (read-only root filesystem)

The Helm chart enables `readOnlyRootFilesystem: true` by default. That means Web Admin file uploads must not write into the container root filesystem (relative paths).

SMS writes uploaded files into `files_dir` (separate config):

- Local default: `./data/files`
- Helm chart default: `/var/lib/spear/files` (the same PVC is also mounted there to keep it writable)

To override the upload directory:

- Set `files_dir = "/some/writable/path"` in `config.toml`, or
- Set `SMS_FILES_DIR` as an environment variable.

### Enable SMS Web Admin

```bash
helm upgrade --install spear deploy/helm/spear \
  --set sms.config.enableWebAdmin=true
```

Access Web Admin:

```bash
kubectl -n spear port-forward svc/spear-spear-sms 18082:8081
```

Then open:

- http://127.0.0.1:18082/

### Enable Router Filter

By default, Router Filter server is provided by SMS (built-in, no-op by default). To enable SPEARlet routing-time filter calls:

```bash
helm upgrade --install spear deploy/helm/spear \
  --set routerFilter.enabled=true
```

To override the router filter server address (host:port), set:

```bash
helm upgrade --install spear deploy/helm/spear \
  --set routerFilter.enabled=true \
  --set routerFilter.addr="<HOST>:<PORT>"
```

### Enable SPEARlet Kubernetes runtime RBAC

SPEARlet Kubernetes runtime uses `kubectl` inside the container to create Jobs/Pods. Enable RBAC:

```bash
helm upgrade --install spear deploy/helm/spear \
  --set spearlet.config.kubernetesRuntime.enabled=true \
  --set spearlet.rbac.create=true
```

Note:

- The provided `deploy/docker/spearlet/Dockerfile` is optimized for local testing and does not bundle `kubectl` to reduce external network dependencies during image build.
- If you need Kubernetes runtime in production, build a SPEARlet image that includes `kubectl` (or mount `kubectl` into the container) before enabling this feature.

## Uninstall

```bash
helm uninstall spear
```
