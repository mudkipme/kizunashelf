# KizunaShelf

KizunaShelf is a read-only personal catalog and relationship browser for an Obsidian vault.

Use it to organize any typed collection you keep in Markdown: media backlogs, books, games, shows, music, projects, people, places, research notes, or custom archives. The app keeps your Markdown files as the source of truth, builds an in-memory index from configurable frontmatter fields, and gives you one place to browse, track, review, and connect everything.

KizunaShelf is fully schema-driven. You decide which folders are entity types, which frontmatter fields hold titles or images, which fields represent dates or status, and which fields should become relationships. The Rust API can run as a self-hosted web server or be embedded directly inside the Tauri desktop app.

## Features

- Custom collection types for whatever you track, each with its own label, icon, folder, title languages, metadata fields, dates, external refs, and relation fields.
- Home dashboard with configurable sections for current, upcoming, active, completed, or otherwise status-driven lists.
- Library browser for every configured type, with pagination, search, status filters, relation/cover filters, title-language selection, sorting, and grid/list layouts.
- Entity detail pages that show frontmatter, rendered Markdown body, dates, external refs, local relationships, and linked entities.
- Calendar views for dated entities and daily notes, including month navigation and planning views.
- Relation explorer with grouped relation fields, target summaries, unresolved links, and local graph context.
- Statistics and analytics for collection totals, type distribution, status distribution, relations, and dated timelines.
- Review queues for missing metadata, missing covers, and unresolved relations.
- Settings editor and first-run onboarding for the full `kizunashelf.config.json` schema.
- Web path inputs with directory autocomplete and desktop-native folder selection through Tauri's dialog plugin.
- Self-hosted web mode, Docker-friendly production server mode, and desktop mode from the same backend.

KizunaShelf treats the vault content as read-only. The Settings and onboarding flows write the app config file, not the Markdown vault.

## First Run

Start the web app during development:

```bash
pnpm install
pnpm dev
```

Open `http://localhost:5173/`. If the configured `kizunashelf.config.json` does not exist, KizunaShelf redirects to `/onboarding`.

The onboarding page is the same structured editor used by Settings. Fill in:

- `Vault root`: the absolute path to the Obsidian vault.
- `Taxonomy root`: the collection root folder inside the vault; the default convention is `Taxonomy`, but any folder name works.
- `Types`: each collection folder you want KizunaShelf to index.
- `Fields`: frontmatter names for titles, images, statuses, dates, external refs, and relations.
- Optional `Home` and `Daily Notes` sections.

Click `Create Config`. KizunaShelf writes the config file, reloads the in-memory library, and opens the normal app.

## Configuration

The server reads `KIZUNASHELF_CONFIG` when it is set. Otherwise it uses:

```text
config/kizunashelf.config.json
```

The desktop app searches for `kizunashelf.config.json` in this order:

1. `KIZUNASHELF_CONFIG`
2. `$XDG_CONFIG_HOME/kizunashelf.config.json`
3. `~/.config/kizunashelf.config.json`
4. `$XDG_CONFIG_DIR/kizunashelf.config.json`
5. Each `$XDG_CONFIG_DIRS` entry
6. On macOS, `~/Library/Application Support/kizunashelf.config.json`
7. On macOS, `~/Library/Application Support/KizunaShelf/kizunashelf.config.json`

If none of those files exist, desktop opens onboarding and writes the new config to the first candidate path.

The Settings page at `/settings` can edit every config field:

- Core: `vaultRoot`, `taxonomyRoot`, `relationshipFields`, `readConcurrency`
- Daily notes: `paths`, `datePattern`, `snippetMaxLength`
- Home: `title`, section `id`, `title`, `type`, `status`, `limit`, `sort`, `direction`
- Types: `id`, `label`, `icon`, `path`, `defaultTitleLanguage`
- Type fields: `titleLanguages`, `subtitle`, `image`, `status`, `dateRoles.planning`, `dateRoles.completed`, `externalRefs`, `relations`

On the web app, path fields are normal text inputs with autocomplete suggestions from the API. In the desktop app, the same fields also show a folder button that opens the native folder picker.

## Development

```bash
pnpm install
pnpm dev
```

`pnpm dev` starts the Rust API and Vite together. Vite proxies `/api` to the Rust server on port `8787`. Use separate terminals if preferred:

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

Rust integration tests build temporary vault fixtures at runtime and exercise the API router directly.

## Production Web Server

Build and serve the production app:

```bash
pnpm build
pnpm serve
```

`pnpm serve` runs the Rust API and serves the built Vite app from one process. It listens on `0.0.0.0:8787` by default. Environment variables:

- `HOST`: bind host, default `0.0.0.0`
- `PORT`: bind port, default `8787`
- `KIZUNASHELF_CONFIG`: config file path
- `KIZUNASHELF_CACHE_TTL_MS`: in-memory library cache TTL, default `10000`
- `KIZUNASHELF_WEB_DIST`: alternate web build path
- `KIZUNASHELF_SERVE_WEB=false`: serve only the API

## Docker

Build the image from the repository root:

```bash
docker build -t kizunashelf .
```

Run it with a mounted config and vault. The exact paths depend on your host; the important part is that `KIZUNASHELF_CONFIG` points at the mounted config file and `vaultRoot` inside that config points at the mounted vault path as seen inside the container.

## Desktop

```bash
pnpm dev:desktop
pnpm build:desktop
```

The desktop app uses Tauri and calls the Rust API router in-process, so it does not open an HTTP listener. It uses the same onboarding and Settings UI as the web app. Path fields can be typed manually, autocompleted, or selected with the native folder picker.

Linux desktop builds require Tauri's WebKitGTK system packages. On Fedora-like systems install `webkit2gtk4.1-devel`, `openssl-devel`, `libappindicator-gtk3-devel`, `librsvg2-devel`, and `libxdo-devel`; see the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for other distributions.

## API Contract

Rust structs are the source of truth for the HTTP response schema. Generate the OpenAPI document and TypeScript/Zod validators with:

```bash
pnpm contract:generate
```

The generated contract lives in `packages/api-contract`.

## API Surface

The Rust API exposes:

- `GET /api/health`
- `GET /api/config`
- `GET /api/settings/config`
- `PUT /api/settings/config`
- `GET /api/settings/path-suggestions`
- `GET /api/home`
- `GET /api/stats`
- `GET /api/analytics`
- `GET /api/cleanup-queues`
- `GET /api/calendar`
- `GET /api/entities`
- `GET /api/entities/:id`
- `GET /api/entities/:id/dates`
- `GET /api/entities/:id/relations`
- `GET /api/relations`
- `GET /api/relation-groups`
