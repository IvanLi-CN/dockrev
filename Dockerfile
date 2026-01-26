# syntax=docker/dockerfile:1

FROM docker:29-cli AS docker-cli

FROM alpine:3.20 AS runtime-base
RUN apk add --no-cache ca-certificates \
  && update-ca-certificates \
  && mkdir -p /usr/local/libexec/docker/cli-plugins

COPY --from=docker-cli /usr/local/bin/docker /usr/local/bin/docker
COPY --from=docker-cli /usr/local/libexec/docker/cli-plugins/docker-compose /usr/local/libexec/docker/cli-plugins/docker-compose
RUN ln -sf /usr/local/libexec/docker/cli-plugins/docker-compose /usr/local/bin/docker-compose

ARG APP_EFFECTIVE_VERSION
ENV APP_EFFECTIVE_VERSION="${APP_EFFECTIVE_VERSION}"

EXPOSE 50883
CMD ["/usr/local/bin/dockrev"]

FROM oven/bun:1.3.6-alpine AS web-builder
WORKDIR /app

COPY web/package.json web/bun.lock ./web/
RUN cd web && bun install --frozen-lockfile

COPY web ./web
RUN cd web && bun run build

FROM rust:1.91-bookworm AS builder
WORKDIR /src

ARG TARGETARCH

RUN apt-get update \
  && apt-get install -y --no-install-recommends musl-tools pkg-config ca-certificates \
  && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY src ./src
COPY --from=web-builder /app/web/dist ./web/dist

RUN case "${TARGETARCH}" in \
    amd64) target="x86_64-unknown-linux-musl" ;; \
    arm64) target="aarch64-unknown-linux-musl" ;; \
    *) echo "unsupported TARGETARCH=${TARGETARCH}" >&2; exit 1 ;; \
  esac \
  && rustup target add "${target}" \
  && cargo build -p dockrev-api --bin dockrev --release --locked --target "${target}" \
  && cargo build -p dockrev-supervisor --bin dockrev-supervisor --release --locked --target "${target}" \
  && cp "target/${target}/release/dockrev" /src/dockrev \
  && cp "target/${target}/release/dockrev-supervisor" /src/dockrev-supervisor

FROM runtime-base AS runtime-prebuilt
ARG TARGETARCH
COPY dist/ci/docker/${TARGETARCH}/dockrev /usr/local/bin/dockrev
COPY dist/ci/docker/${TARGETARCH}/dockrev-supervisor /usr/local/bin/dockrev-supervisor

FROM runtime-base AS runtime
COPY --from=builder /src/dockrev /usr/local/bin/dockrev
COPY --from=builder /src/dockrev-supervisor /usr/local/bin/dockrev-supervisor
