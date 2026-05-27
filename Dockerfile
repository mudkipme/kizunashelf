FROM rust:1-alpine AS base

ENV PNPM_HOME="/pnpm"
ENV PATH="$PNPM_HOME:$PATH"
WORKDIR /app

RUN apk add --no-cache build-base ca-certificates nodejs npm \
  && npm install -g pnpm@10.28.0

FROM base AS deps

COPY package.json pnpm-lock.yaml pnpm-workspace.yaml tsconfig.base.json Cargo.toml Cargo.lock ./
COPY crates/kizunashelf/Cargo.toml crates/kizunashelf/Cargo.toml
COPY apps/web/package.json apps/web/package.json
COPY packages/api-contract/package.json packages/api-contract/package.json

RUN pnpm install --frozen-lockfile

FROM deps AS build

COPY apps apps
COPY packages packages
COPY config config
COPY crates crates

RUN pnpm contract:generate \
  && pnpm -r build \
  && cargo build --release -p kizunashelf

FROM alpine:3.22 AS runner

RUN apk add --no-cache ca-certificates

ENV HOST="0.0.0.0"
ENV PORT="8787"
WORKDIR /app

COPY --from=build /app/target/release/kizunashelf-api /usr/local/bin/kizunashelf-api
COPY --from=build /app/apps/web/dist apps/web/dist
COPY config config

EXPOSE 8787

CMD ["kizunashelf-api"]
