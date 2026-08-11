# syntax=docker/dockerfile:1@sha256:87999aa3d42bdc6bea60565083ee17e86d1f3339802f543c0d03998580f9cb89

# Stage 1: Build both binaries
FROM rust:1.94.1-slim@sha256:cf09adf8c3ebaba10779e5c23ff7fe4df4cccdab8a91f199b0c142c53fef3e1a AS builder
WORKDIR /build
COPY . .
RUN cargo build --release --bin modelrelay-server --bin modelrelay-worker

# Stage 2: Minimal runtime image
FROM debian:bookworm-slim@sha256:abd67ffcfa541b485a3dff59865ab629aa048a6c613e639d36e7456b0b229241
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /build/target/release/modelrelay-server /usr/local/bin/modelrelay-server
COPY --from=builder /build/target/release/modelrelay-worker /usr/local/bin/modelrelay-worker
EXPOSE 8080
CMD ["modelrelay-server"]
