# SPEAR unified container image / SPEAR 统一容器镜像

ARG RUST_VERSION=1.91
ARG DEBIAN_SUITE=trixie
ARG LLAMA_CPP_REF=master
ARG CMAKE_VERSION=4.2.3
ARG NODE_IMAGE=node:20-bookworm-slim

FROM ${NODE_IMAGE} AS node_runtime

FROM ${NODE_IMAGE} AS web_builder
WORKDIR /work
COPY web-admin/ web-admin/
COPY web-console/ web-console/
COPY assets/ assets/
RUN cd /work/web-admin \
  && (if [ -f package-lock.json ]; then npm ci --silent; else npm install --silent; fi) \
  && npm run build
RUN cd /work/web-console \
  && (if [ -f package-lock.json ]; then npm ci --silent; else npm install --silent; fi) \
  && npm run build

FROM rust:${RUST_VERSION}-bookworm AS llama_builder
ARG LLAMA_CPP_REF
ARG CMAKE_VERSION
WORKDIR /work
RUN git clone --depth 1 --branch "v${CMAKE_VERSION}" https://github.com/Kitware/CMake.git /tmp/cmake-src \
  && cd /tmp/cmake-src \
  && ./bootstrap --prefix=/opt/cmake -- -DCMAKE_BUILD_TYPE=Release \
  && make -j "$(nproc)" \
  && make install \
  && ln -sf /opt/cmake/bin/cmake /usr/local/bin/cmake \
  && ln -sf /opt/cmake/bin/ctest /usr/local/bin/ctest \
  && ln -sf /opt/cmake/bin/cpack /usr/local/bin/cpack \
  && cmake --version | head -n 1 \
  && rm -rf /tmp/cmake-src
WORKDIR /work/llama.cpp
RUN set -e; \
  arch="$(uname -m)"; \
  extra=""; \
  if [ "$arch" = "aarch64" ] || [ "$arch" = "arm64" ]; then \
    extra="-DGGML_NATIVE=OFF -DGGML_CPU_ALL_VARIANTS=OFF -DGGML_CPU_ARM_ARCH=armv8.2-a+fp16+fp16fml+dotprod"; \
  fi; \
  git clone --depth 1 --branch "${LLAMA_CPP_REF}" https://github.com/ggml-org/llama.cpp . \
  && cmake -B build -DCMAKE_BUILD_TYPE=Release -DBUILD_SHARED_LIBS=OFF $extra \
  && cmake --build build --config Release -j "$(nproc)" -t llama-server

FROM rust:${RUST_VERSION}-bookworm AS builder

WORKDIR /work

ARG USE_CARGO_MIRROR=1
ARG CARGO_MIRROR_REGISTRY="sparse+https://rsproxy.cn/index/"

RUN mkdir -p /usr/local/cargo \
  && if [ "${USE_CARGO_MIRROR}" = "1" ]; then \
    printf '%s\n' \
      '[source.crates-io]' \
      'replace-with = "mirror"' \
      '' \
      '[source.mirror]' \
      "registry = \"${CARGO_MIRROR_REGISTRY}\"" \
      '' \
      '[http]' \
      'multiplexing = false' \
      > /usr/local/cargo/config.toml; \
  else \
    printf '%s\n' \
      '[http]' \
      'multiplexing = false' \
      > /usr/local/cargo/config.toml; \
  fi

RUN apt-get update -o Acquire::Retries=3 \
  && apt-get install -y --no-install-recommends clang libclang-dev \
  && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
COPY build.rs ./
COPY benches/ benches/
COPY static/ static/
COPY proto/ proto/
COPY assets/ assets/
COPY src/ src/

# Local path dependencies (SDK crates) / 本地 path 依赖（SDK crates）
COPY sdk/rust/crates/spear-ssf/ sdk/rust/crates/spear-ssf/

COPY --from=web_builder /work/assets/admin assets/admin
COPY --from=web_builder /work/assets/console assets/console

RUN cargo build --release --locked --bin sms --bin spearlet

FROM debian:${DEBIAN_SUITE}-slim AS runtime

RUN apt-get update -o Acquire::Retries=3 \
  && apt-get install -y --no-install-recommends ca-certificates libssl3 libstdc++6 libgomp1 \
  && rm -rf /var/lib/apt/lists/*

COPY --from=builder /work/target/release/sms /usr/local/bin/sms
COPY --from=builder /work/target/release/spearlet /usr/local/bin/spearlet
COPY --from=builder /work/target/release/build/wasmedge-sys-*/out/standalone/WasmEdge-*/lib64/ /usr/local/lib/
COPY deploy/docker/entrypoints/spear.sh /entrypoint.sh

ENV LD_LIBRARY_PATH=/usr/local/lib

RUN chmod +x /entrypoint.sh

USER 10001:10001

ENTRYPOINT ["/entrypoint.sh"]

FROM runtime AS runtime_with_node
COPY --from=node_runtime /usr/local/bin/node /usr/local/bin/node
COPY --from=node_runtime /usr/local/lib/node_modules/ /usr/local/lib/node_modules/
USER 0
RUN ln -sf ../lib/node_modules/npm/bin/npm-cli.js /usr/local/bin/npm \
  && ln -sf ../lib/node_modules/npm/bin/npx-cli.js /usr/local/bin/npx
USER 10001:10001

FROM runtime_with_node AS runtime_with_node_and_llama
COPY --from=llama_builder /work/llama.cpp/build/bin/llama-server /usr/local/bin/llama-server
