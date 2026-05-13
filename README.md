# KizunaShelf

KizunaShelf is a read-only ACGN asset browser for an Obsidian `Taxonomy/` vault.

The v0.1 MVP keeps Markdown files as the source of truth, builds an in-memory relation index from configurable frontmatter fields, and exposes a compact web UI for browsing entities and links.

## Development

```bash
pnpm install
pnpm --filter @kizunashelf/api dev
pnpm --filter @kizunashelf/web dev
```

After building, run the production server with:

```bash
pnpm build
pnpm serve
```

`pnpm serve` serves the Hono API and the built Vite app from one process. It listens on `0.0.0.0:8787` by default for VM/container access. Set `HOST` or `PORT` to override, and set `KIZUNASHELF_SERVE_WEB=false` to serve only the API.

The default config reads:

```text
/var/home/mudkip/Containers/obsidian/config/Obsidian/Mudkip/Taxonomy
```

Change `config/kizunashelf.config.json` or set `KIZUNASHELF_CONFIG` for another vault.
