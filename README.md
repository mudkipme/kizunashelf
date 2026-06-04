# KizunaShelf

KizunaShelf is a read-only ACGN asset browser for an Obsidian `Taxonomy/` vault.

The v0.2 app keeps Markdown files as the source of truth, builds an in-memory relation index from configurable frontmatter fields, and exposes a compact web UI for browsing entities, timelines, review queues, and links. The API is implemented in Rust so the same backend can run as a self-hosted web app server or be embedded by Tauri.

## Development

```bash
pnpm install
pnpm dev
```

`pnpm dev` starts the Rust API and Vite together. Vite proxies `/api` to the Rust server on port `8787`. Use `pnpm dev:api` and `pnpm dev:web` if you want separate terminals.

## Desktop

```bash
pnpm dev:desktop
pnpm build:desktop
```

The desktop app uses Tauri and calls the Rust API router in-process, so it does not open an HTTP listener. On startup it looks for `kizunashelf.config.json` in `KIZUNASHELF_CONFIG`, `$XDG_CONFIG_HOME`, `~/.config`, `$XDG_CONFIG_DIR`, `$XDG_CONFIG_DIRS`, and on macOS also under `~/Library/Application Support`. If none of those files exist, it opens the onboarding flow and writes the new config to the first candidate path.

Linux desktop builds require Tauri's WebKitGTK system packages. On Fedora-like systems install `webkit2gtk4.1-devel`, `openssl-devel`, `libappindicator-gtk3-devel`, `librsvg2-devel`, and `libxdo-devel`; see the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for other distributions.

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
