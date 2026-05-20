#!/usr/bin/env bash
set -euo pipefail

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "missing dependency: $1" >&2
    exit 1
  fi
}

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

require_cmd docker
require_cmd kind
require_cmd kubectl
require_cmd helm

CLUSTER_NAME="${CLUSTER_NAME:-spear-openai}"
REUSE_CLUSTER="${REUSE_CLUSTER:-0}"
KEEP_CLUSTER="${KEEP_CLUSTER:-1}"
KUBECONFIG_FILE="${KUBECONFIG_FILE:-$ROOT_DIR/.tmp/kubeconfig-kind-${CLUSTER_NAME}}"

NAMESPACE="${NAMESPACE:-spear}"
RELEASE_NAME="${RELEASE_NAME:-spear}"

SPEAR_IMAGE_REPO="${SPEAR_IMAGE_REPO:-}"
SMS_IMAGE_REPO="${SMS_IMAGE_REPO:-spear-sms}"
SPEARLET_IMAGE_REPO="${SPEARLET_IMAGE_REPO:-spear-spearlet}"
IMAGE_TAG="${IMAGE_TAG:-local}"
SPEARLET_WITH_NODE="${SPEARLET_WITH_NODE:-1}"
SPEARLET_WITH_LLAMA_SERVER="${SPEARLET_WITH_LLAMA_SERVER:-1}"

ENABLE_WEB_ADMIN="${ENABLE_WEB_ADMIN:-1}"
ENABLE_ROUTER_FILTER="${ENABLE_ROUTER_FILTER:-1}"
ENABLE_E2E="${ENABLE_E2E:-0}"
DEBIAN_SUITE="${DEBIAN_SUITE:-trixie}"
DEBUG="${DEBUG:-1}"
LOG_LEVEL="${LOG_LEVEL:-info}"
LOG_FORMAT="${LOG_FORMAT:-json}"
NO_CACHE="${NO_CACHE:-0}"
PULL_BASE="${PULL_BASE:-1}"

TIMEOUT="${TIMEOUT:-300s}"

mkdir -p "$(dirname "$KUBECONFIG_FILE")"

k() {
  KUBECONFIG="$KUBECONFIG_FILE" kubectl "$@"
}

cleanup() {
  local rc="$?"
  set +e
  if [[ "$rc" -ne 0 ]]; then
    echo "failed (rc=$rc). hints:" >&2
    echo "  KUBECONFIG=$KUBECONFIG_FILE kubectl -n $NAMESPACE get pods -o wide" >&2
    echo "  KUBECONFIG=$KUBECONFIG_FILE kubectl -n $NAMESPACE get events --sort-by=.metadata.creationTimestamp | tail -n 80" >&2
  fi
  if [[ "$KEEP_CLUSTER" != "1" && "$REUSE_CLUSTER" != "1" ]]; then
    KUBECONFIG="$KUBECONFIG_FILE" kind delete cluster --name "$CLUSTER_NAME" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

if kind get clusters | grep -qx "$CLUSTER_NAME"; then
  if [[ "$REUSE_CLUSTER" != "1" ]]; then
    KUBECONFIG="$KUBECONFIG_FILE" kind delete cluster --name "$CLUSTER_NAME"
    KUBECONFIG="$KUBECONFIG_FILE" kind create cluster --name "$CLUSTER_NAME"
  fi
else
  KUBECONFIG="$KUBECONFIG_FILE" kind create cluster --name "$CLUSTER_NAME"
fi

KUBECONFIG="$KUBECONFIG_FILE" kind export kubeconfig --name "$CLUSTER_NAME" --kubeconfig "$KUBECONFIG_FILE" >/dev/null
if ! k cluster-info >/dev/null 2>&1; then
  echo "kind kubeconfig created but cluster not reachable: $CLUSTER_NAME" >&2
  echo "try: kind get clusters; docker ps | grep kind" >&2
  exit 1
fi

DOCKER_BUILD_FLAGS=()
if [[ "${PULL_BASE}" == "1" ]]; then
  DOCKER_BUILD_FLAGS+=(--pull)
fi
if [[ "${NO_CACHE}" == "1" ]]; then
  DOCKER_BUILD_FLAGS+=(--no-cache)
fi

if [[ -n "${SPEAR_IMAGE_REPO}" ]]; then
  SMS_IMAGE_REPO="${SPEAR_IMAGE_REPO}"
  SPEARLET_IMAGE_REPO="${SPEAR_IMAGE_REPO}"

  if [[ "${SPEARLET_WITH_NODE}" == "1" ]]; then
    SPEAR_TARGET="runtime_with_node"
    if [[ "${SPEARLET_WITH_LLAMA_SERVER}" == "1" ]]; then
      SPEAR_TARGET="runtime_with_node_and_llama"
    fi
    docker build "${DOCKER_BUILD_FLAGS[@]}" -f deploy/docker/spear/Dockerfile \
      --build-arg "DEBIAN_SUITE=${DEBIAN_SUITE}" \
      --target "${SPEAR_TARGET}" \
      -t "${SPEAR_IMAGE_REPO}:${IMAGE_TAG}" .
  else
    docker build "${DOCKER_BUILD_FLAGS[@]}" -f deploy/docker/spear/Dockerfile --build-arg "DEBIAN_SUITE=${DEBIAN_SUITE}" -t "${SPEAR_IMAGE_REPO}:${IMAGE_TAG}" .
  fi
else
  docker build "${DOCKER_BUILD_FLAGS[@]}" -f deploy/docker/sms/Dockerfile --build-arg "DEBIAN_SUITE=${DEBIAN_SUITE}" -t "${SMS_IMAGE_REPO}:${IMAGE_TAG}" .
  if [[ "${SPEARLET_WITH_NODE}" == "1" ]]; then
    SPEARLET_TARGET="runtime_with_node"
    if [[ "${SPEARLET_WITH_LLAMA_SERVER}" == "1" ]]; then
      SPEARLET_TARGET="runtime_with_node_and_llama"
    fi
    docker build "${DOCKER_BUILD_FLAGS[@]}" -f deploy/docker/spearlet/Dockerfile \
      --build-arg "DEBIAN_SUITE=${DEBIAN_SUITE}" \
      --target "${SPEARLET_TARGET}" \
      -t "${SPEARLET_IMAGE_REPO}:${IMAGE_TAG}" .
  else
    docker build "${DOCKER_BUILD_FLAGS[@]}" -f deploy/docker/spearlet/Dockerfile --build-arg "DEBIAN_SUITE=${DEBIAN_SUITE}" -t "${SPEARLET_IMAGE_REPO}:${IMAGE_TAG}" .
  fi
fi

KIND_IMAGES=(
  "${SMS_IMAGE_REPO}:${IMAGE_TAG}"
  "${SPEARLET_IMAGE_REPO}:${IMAGE_TAG}"
)
KUBECONFIG="$KUBECONFIG_FILE" kind load docker-image --name "$CLUSTER_NAME" "${KIND_IMAGES[@]}"

k get namespace "$NAMESPACE" >/dev/null 2>&1 || k create namespace "$NAMESPACE" >/dev/null

if [[ -n "${OPENAI_API_KEY:-}" ]]; then
  k -n "$NAMESPACE" create secret generic openai-api-key \
    --from-literal=OPENAI_API_KEY="$OPENAI_API_KEY" \
    --dry-run=client -o yaml | k apply -f - >/dev/null
  echo "openai-api-key secret applied (from local OPENAI_API_KEY)"
else
  echo "OPENAI_API_KEY not set locally; skip creating openai-api-key secret"
fi

HELM_ARGS=(
  upgrade
  --install
  "$RELEASE_NAME"
  deploy/helm/spear
  -n
  "$NAMESPACE"
  --create-namespace
  -f
  deploy/helm/spear/values-openai.yaml
  --set
  "sms.config.logging.level=$([[ \"$DEBUG\" == \"1\" ]] && echo debug || echo \"$LOG_LEVEL\")"
  --set
  "sms.config.logging.format=$([[ \"$DEBUG\" == \"1\" ]] && echo pretty || echo \"$LOG_FORMAT\")"
  --set
  "spearlet.config.logging.level=$([[ \"$DEBUG\" == \"1\" ]] && echo debug || echo \"$LOG_LEVEL\")"
  --set
  "spearlet.config.logging.format=$([[ \"$DEBUG\" == \"1\" ]] && echo pretty || echo \"$LOG_FORMAT\")"
  --set
  "sms.image.repository=${SMS_IMAGE_REPO}"
  --set
  "sms.image.tag=${IMAGE_TAG}"
  --set
  "spearlet.image.repository=${SPEARLET_IMAGE_REPO}"
  --set
  "spearlet.image.tag=${IMAGE_TAG}"
)

if [[ "$ENABLE_WEB_ADMIN" != "1" ]]; then
  HELM_ARGS+=(
    --set
    "sms.config.enableWebAdmin=false"
  )
fi

if [[ "${ENABLE_ROUTER_FILTER}" == "1" ]]; then
  HELM_ARGS+=(
    --set
    "routerFilter.enabled=true"
  )
else
  HELM_ARGS+=(
    --set
    "routerFilter.enabled=false"
  )
fi
if [[ "${ENABLE_E2E}" == "1" ]]; then
  HELM_ARGS+=(
    --set
    "e2e.enabled=true"
  )
fi

KUBECONFIG="$KUBECONFIG_FILE" helm "${HELM_ARGS[@]}"

k -n "$NAMESPACE" rollout status "statefulset/${RELEASE_NAME}-spear-sms" --timeout="$TIMEOUT"
k -n "$NAMESPACE" wait --for=condition=Ready "pod/${RELEASE_NAME}-spear-sms-0" --timeout="$TIMEOUT"

k -n "$NAMESPACE" wait --for=condition=Ready pod -l app.kubernetes.io/component=spearlet --timeout="$TIMEOUT"
if [[ -n "${OPENAI_API_KEY:-}" ]]; then
  POD="$(k -n "$NAMESPACE" get pod -l app.kubernetes.io/component=spearlet -o jsonpath='{.items[0].metadata.name}' 2>/dev/null || true)"
  if [[ -n "$POD" ]]; then
    if ! k -n "$NAMESPACE" exec "$POD" -c spearlet -- sh -lc 'test -n "${OPENAI_API_KEY:-}"' >/dev/null 2>&1; then
      echo "warning: OPENAI_API_KEY secret exists but not injected into spearlet env" >&2
      echo "hint: ensure deploy/helm/spear/values-openai.yaml sets spearlet.extraEnv -> openai-api-key secret" >&2
    fi
  fi
fi

echo "kind cluster ready:"
echo "  export KUBECONFIG=$KUBECONFIG_FILE"
echo "  kubectl -n $NAMESPACE get pods -o wide"
echo "SPEAR Console:"
echo "  kubectl -n $NAMESPACE port-forward svc/${RELEASE_NAME}-spear-sms 18080:8080"
echo "  open http://127.0.0.1:18080/console"
if [[ "$ENABLE_WEB_ADMIN" == "1" ]]; then
  echo "web admin:"
  echo "  kubectl -n $NAMESPACE port-forward svc/${RELEASE_NAME}-spear-sms 18082:8081"
  echo "  open http://127.0.0.1:18082/"
else
  echo "web admin: disabled (ENABLE_WEB_ADMIN=0)"
fi
if [[ "$KEEP_CLUSTER" != "1" && "$REUSE_CLUSTER" != "1" ]]; then
  echo "note: cluster will be deleted on script exit (set KEEP_CLUSTER=1 to keep it)"
fi
