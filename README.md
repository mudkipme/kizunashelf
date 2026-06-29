# KizunaShelf

> A shelf for everything you love — and it stays yours.

Intro in other languages: [简体中文](docs/intro.zh.md) / [日本語](docs/intro.ja.md)

KizunaShelf starts with the familiar shape of a media tracker: TV shows, movies, books, games, anime, albums, and every small obsession waiting on your shelf. But it is not built around one fixed idea of what a "thing" should be. It is schema-driven from the ground up. You decide the types, fields, dates, titles, covers, states, ratings, progress, external links, and relationships. If your world needs characters, goods, cards, voice actors, artists, live events, or trains, shoes, museums, coffee beans, or something nobody else would think to model, KizunaShelf gives you the grammar to describe it.

The shelf is not flat. A game can belong to a franchise. An anime can be based on a novel. A character can point to a voice actor. A remake can look back at the original. Every relation is written as a link, then read back as a map: outgoing, incoming, resolved, unresolved, and grouped into the quiet shape of the things you care about.

Time matters here. Release dates, seasons, future plans, and past records all become part of the same calendar. You can look ahead to the game you want to buy, the movie you plan to watch, or the event you hope to attend; then look back at what you finished across the last few months. When your daily notes mention an entity through a wikilink, KizunaShelf can connect that ordinary day back to the things you love. That's the *kizuna*: the bond between you and what you keep.

And the files remain yours. KizunaShelf follows the idea of files over apps: Markdown is the source of truth, readable by Obsidian, SilverBullet, Zed, VS Code, any text editor, or no app at all. Even if you stop using KizunaShelf someday, the notes, frontmatter, links, memories, and kizuna are still there.

KizunaShelf is a shelf for what matters to you, and a record book for the life that gathered around it.

## What It Does

- Builds a typed catalog from Markdown files in an Obsidian-style vault.
- Lets each collection define its own titles, covers, states, ratings, progress, dates, external refs, and relations.
- Provides library browsing, entity detail pages, calendar views, relation views, analytics, and cleanup queues.
- Supports external metadata matching for configured providers.
- Runs as a self-hosted web app, a Tauri desktop app, or a native iOS app.

KizunaShelf treats Markdown files as the source of truth. When content writes are enabled, it can edit entity frontmatter/body and create or delete entity files. In read-only mode, those content write features are disabled.

The vault schema (`KizunaShelf/config.yaml`) always lives inside the vault and is shared across machines. App-level settings are sourced per runtime:

- **Self-hosted web**: a single vault configured entirely through environment variables — there is no app config file. For multiple vaults, run multiple instances (the image is small).
- **Desktop**: multiple vaults managed in-app (Obsidian-style switching); provider credentials are stored in the OS keychain.
- **iOS**: vaults are opened from Files (On My iPhone / iCloud / a file provider); credentials are stored in the Keychain.

## Quick Start

```bash
pnpm install
KIZUNASHELF_VAULT_ROOT=/path/to/your/vault pnpm dev
```

Open `http://localhost:5173/`. The web app points at the single vault named by `KIZUNASHELF_VAULT_ROOT`; if that vault has no `KizunaShelf/config.yaml` yet, KizunaShelf redirects to onboarding to create the schema.

See [docs/config.md](docs/config.md) for the full schema, per-runtime configuration, Settings behavior, and provider credentials, and [docs/syncing.md](docs/syncing.md) for how to sync a vault across devices.

## Development

`pnpm dev` starts the Rust API and Vite together. Vite proxies `/api` to the Rust server on port `8787`.

Use separate terminals if preferred:

```bash
pnpm dev:api
pnpm dev:web
```

Useful checks:

```bash
pnpm build
pnpm typecheck
pnpm test
```

Generate the OpenAPI document and TypeScript/Zod validators with:

```bash
pnpm contract:generate
```

The generated contract lives in `packages/api-contract`.

## Production

To self-host the production web app (build, serve, Docker, configuration, multiple vaults, and authentication), see [docs/selfhosting.md](docs/selfhosting.md). KizunaShelf has no built-in authentication — that guide explains how to put it behind a reverse-proxy auth solution (Authentik, Authelia, tinyauth).

For desktop development and builds:

```bash
pnpm dev:desktop
pnpm build:desktop
```

The desktop app uses Tauri and calls the Rust API router in-process, so it does not open an HTTP listener. It manages multiple vaults in-app (open or create, then switch between them) and stores provider credentials in the OS keychain (macOS Keychain, Windows Credential Manager, or the Linux Secret Service).
