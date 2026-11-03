# syntax=docker/dockerfile:1

FROM node:24-alpine AS web
WORKDIR /app
COPY package.json package-lock.json ./
COPY apps/web/package.json apps/web/package.json
RUN npm ci
COPY apps/web apps/web
RUN npm --workspace apps/web run build

FROM rust:1-slim-bookworm AS api
WORKDIR /app
RUN apt-get update \
    && apt-get install -y --no-install-recommends pkg-config \
    && rm -rf /var/lib/apt/lists/*
COPY apps/api/Cargo.toml apps/api/Cargo.lock ./
COPY apps/api/migrations migrations
RUN mkdir src \
    && echo "fn main() {}" > src/main.rs \
    && cargo build --release \
    && rm -rf src
COPY apps/api/src src
RUN touch src/main.rs && cargo build --release

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl libsqlite3-0 \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --create-home cockpit \
    && mkdir -p /data /app/web \
    && chown -R cockpit:cockpit /data /app

COPY --from=api /app/target/release/wartungscockpit-api /app/wartungscockpit-api
COPY --from=web /app/apps/web/dist /app/web

USER cockpit
ENV PORT=8080 \
    DATABASE_URL="sqlite:///data/cockpit.db?mode=rwc" \
    STATIC_DIR=/app/web

EXPOSE 8080
VOLUME ["/data"]

HEALTHCHECK --interval=30s --timeout=3s --start-period=15s --retries=3 \
    CMD curl -fsS http://127.0.0.1:8080/api/v1/health || exit 1

ENTRYPOINT ["/app/wartungscockpit-api"]
