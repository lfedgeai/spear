FROM rust:1.88-bookworm AS builder

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY build.rs ./
COPY benches ./benches
COPY assets ./assets
COPY sdk ./sdk
COPY src ./src
COPY proto ./proto

RUN cargo build --release --locked --no-default-features --features debug-server-bin --bin debug-server

FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
  && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /app/target/release/debug-server /usr/local/bin/debug-server

EXPOSE 7777

ENTRYPOINT ["/usr/local/bin/debug-server"]
