# syntax=docker/dockerfile:1
FROM rust:1-alpine AS base

ENV PNPM_HOME="/pnpm"
ENV PATH="$PNPM_HOME:$PATH"
WORKDIR /app

RUN apk add --no-cache build-base ca-certificates nodejs npm \
  && npm install -g pnpm@11.6.0

FROM base AS deps

COPY package.json pnpm-lock.yaml pnpm-workspace.yaml tsconfig.base.json Cargo.toml Cargo.lock ./
COPY crates/kizunashelf/Cargo.toml crates/kizunashelf/Cargo.toml
COPY apps/web/package.json apps/web/package.json
COPY packages/api-contract/package.json packages/api-contract/package.json

RUN --mount=type=cache,target=/pnpm/store \
  pnpm install --frozen-lockfile

FROM deps AS build

COPY apps apps
COPY packages packages
COPY config config
COPY crates crates

# Build the web bundle from the committed contract. CI enforces that the
# generated contract stays in sync (see .woodpecker/docker.yml), so there is no
# need to regenerate it here — doing so would run a debug `cargo run` of the
# schema binary and compile the crate a second time.
RUN pnpm -r build

# Cache the cargo registry/git and the target dir across image builds. The
# release binary is copied out of the cache-mounted target so it survives into
# the final image layer (cache mounts are not part of the layer).
RUN --mount=type=cache,target=/usr/local/cargo/registry \
  --mount=type=cache,target=/usr/local/cargo/git \
  --mount=type=cache,target=/app/target \
  cargo build --release -p kizunashelf \
  && cp target/release/kizunashelf-api /usr/local/bin/kizunashelf-api

FROM alpine:3.22 AS runner

RUN apk add --no-cache ca-certificates

ENV HOST="0.0.0.0"
ENV PORT="8787"
ENV KIZUNASHELF_SETTINGS_WRITABLE="false"
# Single vault, mounted at /vault (override with KIZUNASHELF_VAULT_ROOT). Run one
# container per vault.
ENV KIZUNASHELF_VAULT_ROOT="/vault"
VOLUME ["/vault"]
WORKDIR /app

COPY --from=build /usr/local/bin/kizunashelf-api /usr/local/bin/kizunashelf-api
COPY --from=build /app/apps/web/dist apps/web/dist
COPY config config

EXPOSE 8787

CMD ["kizunashelf-api"]
