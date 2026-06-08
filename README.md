# KizunaShelf

KizunaShelf is a personal catalog editor and relationship browser for an Obsidian vault.

Use it to organize any typed collection you keep in Markdown: media backlogs, books, games, shows, music, projects, people, places, research notes, or custom archives. The app keeps your Markdown files as the source of truth, builds an in-memory index from configurable frontmatter fields, and gives you one place to browse, track, review, and connect everything.

KizunaShelf is fully schema-driven. You decide which folders are entity types, which frontmatter fields hold titles, images, enums, dates, external refs, and relationships. The Rust API can run as a self-hosted web server or be embedded directly inside the Tauri desktop app.

## Features

- Custom collection types for whatever you track, each with its own label, icon, folder, title languages, metadata fields, dates, external refs, and relation fields.
- Home dashboard with configurable type sections, limits, sorting, and title-language display.
- Library browser for every configured type, with pagination, search, relation/cover filters, title-language selection, sorting, and grid/list layouts.
- Entity detail pages that show frontmatter, rendered Markdown body, dates, external refs, local relationships, and linked entities.
- External metadata matching for configured entity types, with provider search, candidate comparison, selectable field application, and refresh from existing external refs.
- Calendar views for dated entities and daily notes, including month navigation and planning views.
- Relation explorer with grouped relation fields, target summaries, unresolved links, and local graph context.
- Statistics and analytics for collection totals, type distribution, relation fields, and dated timelines.
- Review queues for missing metadata, missing covers, and unresolved relations.
- Settings editor and first-run onboarding for the full `kizunashelf.yaml` schema.
- Web path inputs with directory autocomplete and desktop-native folder selection through Tauri's dialog plugin.
- Self-hosted web mode, Docker-friendly production server mode, and desktop mode from the same backend.

KizunaShelf treats Markdown files as the source of truth. When `contentWritable` is enabled it can edit entity frontmatter/body and create or delete entity files; when read-only mode is selected those content write features are disabled.

## First Run

Start the web app during development:

```bash
pnpm install
pnpm dev
```

Open `http://localhost:5173/`. If the configured `kizunashelf.yaml` does not exist, KizunaShelf redirects to `/onboarding`.

The onboarding page is the same structured editor used by Settings. Fill in:

- `Vault root`: the absolute path to the Obsidian vault.
- `Taxonomy root`: the collection root folder inside the vault; the default convention is `Taxonomy`, but any folder name works.
- `Types`: each collection folder you want KizunaShelf to index.
- `Fields`: frontmatter names for stable IDs, titles, images, enums, dates, external refs, and relations.
- Optional `Home` and `Daily Notes` sections.

Click `Create Config`. KizunaShelf writes the config file, reloads the in-memory library, and opens the normal app.

## Configuration

The server reads `KIZUNASHELF_CONFIG` when it is set. Otherwise it uses:

```text
config/kizunashelf.yaml
```

An example config is available at `config/kizunashelf.config.example.yaml`.
For the full schema and design notes, see `docs/config.md`.

The desktop app searches for `kizunashelf.yaml` in this order:

1. `KIZUNASHELF_CONFIG`
2. `$XDG_CONFIG_HOME/kizunashelf.yaml`
3. `~/.config/kizunashelf.yaml`
4. `$XDG_CONFIG_DIR/kizunashelf.yaml`
5. Each `$XDG_CONFIG_DIRS` entry
6. On macOS, `~/Library/Application Support/kizunashelf.yaml`
7. On macOS, `~/Library/Application Support/KizunaShelf/kizunashelf.yaml`

If none of those files exist, desktop opens onboarding and writes the new config to the first candidate path.

The Settings page at `/settings` can edit every config field:

- Core: `vaultRoot`, `taxonomyRoot`, `readConcurrency`
- Daily notes: `paths`, `datePattern`, `snippetMaxLength`
- Home: `title`, section `id`, `title`, `type`, `limit`, `sort`, `direction`
- Types: `id`, `label`, `icon`, `path`, `filename`, `fields`
- Type fields: ordered field entries with `field`, `fieldType`, optional display metadata, enum options, date roles, title language, external source, and relation type
- Field types: `id`, `title`, `image`, `imageList`, `enum`, `enumList`, `progress`, `totalProgress`, `rating`, `bool`, `season`, `date`, `externalRef`, `relation`, `text`, `textList`

On the web app, path fields are normal text inputs with autocomplete suggestions from the API. In the desktop app, the same fields also show a folder button that opens the native folder picker.

## External Metadata Matching

Entity creation and entity detail pages can search supported external sources and apply selected metadata back into Markdown frontmatter. Matching is enabled per entity type by defining `externalRef` fields and optional `externalFields` mappings in `kizunashelf.yaml`.

Supported providers:

- Bangumi: works without extra credentials.
- IGDB: requires `KIZUNASHELF_IGDB_CLIENT_ID` and `KIZUNASHELF_IGDB_CLIENT_SECRET`.
- TheTVDB: requires `KIZUNASHELF_TVDB_API_KEY`; `KIZUNASHELF_TVDB_PIN` is optional.

Provider order is controlled by each type's `externalPriority`. `externalRef` fields store the selected provider URL/id, and `externalFields` describe how candidate metadata such as titles, covers, release dates, summaries, and totals should map into local fields. See `docs/config.md` for the full schema.

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

`pnpm serve` runs the Rust API and serves the built Vite app from one process. It listens on `127.0.0.1:8787` by default. Set `HOST=0.0.0.0` only when you intentionally want to expose it beyond the local machine. Environment variables:

- `HOST`: bind host, default `127.0.0.1`
- `PORT`: bind port, default `8787`
- `KIZUNASHELF_CONFIG`: config file path
- `KIZUNASHELF_CACHE_TTL_MS`: in-memory library cache TTL, default `10000`
- `KIZUNASHELF_WEB_DIST`: alternate web build path
- `KIZUNASHELF_SERVE_WEB=false`: serve only the API
- `KIZUNASHELF_SETTINGS_WRITABLE`: enables Settings writes and path suggestions. Defaults to `true` for loopback hosts and `false` for non-loopback hosts.
- `KIZUNASHELF_IGDB_CLIENT_ID` and `KIZUNASHELF_IGDB_CLIENT_SECRET`: enable IGDB external matching.
- `KIZUNASHELF_TVDB_API_KEY` and optional `KIZUNASHELF_TVDB_PIN`: enable TheTVDB external matching.

The server does not enable wildcard CORS by default. Use the Vite dev proxy during development, or serve the built web app from the Rust process for production.

## Docker

Build the image from the repository root:

```bash
docker build -t kizunashelf .
```

Run it with a mounted config and vault. The exact paths depend on your host; the important part is that `KIZUNASHELF_CONFIG` points at the mounted config file and `vaultRoot` inside that config points at the mounted vault path as seen inside the container.

The Docker image sets `HOST=0.0.0.0` and `KIZUNASHELF_SETTINGS_WRITABLE=false` by default. Set `KIZUNASHELF_SETTINGS_WRITABLE=true` only when you intentionally want onboarding/settings writes available from the published container.

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
- `GET /api/external/search`
- `GET /api/entities`
- `GET /api/entities/:id`
- `GET /api/entities/:id/dates`
- `GET /api/entities/:id/relations`
- `GET /api/relations`
- `GET /api/relation-groups`

`/api/health` includes `diagnosticCount` and the first 20 diagnostics, including malformed frontmatter warnings detected while indexing Markdown files.
