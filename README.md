# KizunaShelf

KizunaShelf is a read-only ACGN asset browser for an Obsidian `Taxonomy/` vault.

The v0.1 MVP keeps Markdown files as the source of truth, builds an in-memory relation index from configurable frontmatter fields, and exposes a compact web UI for browsing entities and links. The API is implemented in Rust so the same backend can run as a self-hosted web app server or be embedded by Tauri.

## Development

```bash
pnpm install
pnpm dev
```

`pnpm dev` starts the Rust API and Vite together. Vite proxies `/api` to the Rust server on port `8787`. Use `pnpm dev:api` and `pnpm dev:web` if you want separate terminals.

After building, run the production server with:

```bash
pnpm build
pnpm serve
```

`pnpm serve` runs the Rust API and serves the built Vite app from one process. It listens on `0.0.0.0:8787` by default for VM/container access. Set `HOST` or `PORT` to override, set `KIZUNASHELF_WEB_DIST` to point at another web build, and set `KIZUNASHELF_SERVE_WEB=false` to serve only the API.

## API Contract

Rust structs are the source of truth for the HTTP response schema. Generate the OpenAPI document and TypeScript/Zod validators with:

```bash
pnpm contract:generate
```

The generated contract lives in `packages/api-contract`. The web client imports those Zod schemas and validates every `fetchJson` response at runtime.

## Tests

```bash
pnpm test
```

Rust integration tests build temporary vault fixtures at runtime and exercise the API router directly.

The default config reads:

```text
/var/home/mudkip/Containers/obsidian/config/Obsidian/Mudkip/Taxonomy
```

Change `config/kizunashelf.config.json` or set `KIZUNASHELF_CONFIG` for another vault.
