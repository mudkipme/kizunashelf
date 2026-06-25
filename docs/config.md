# KizunaShelf Config

KizunaShelf is schema-driven. There are two kinds of configuration:

- **Vault config (the schema)** — stored inside the vault at `<vaultRoot>/KizunaShelf/config.yaml`. It describes which Markdown folders are entity collections and how frontmatter fields should be interpreted. Because it lives in the vault, it travels with the vault and is synced by the vault's own syncing method, so every machine pointing at the vault shares the same schema. This is the bulk of this document.
- **App-level settings** — *where* the vault is and *how this runtime behaves* (writable or read-only). These are **not** a synced file; each runtime sources them differently:
  - **Self-hosted web**: from environment variables only — there is no app config file. One instance serves one vault.
  - **Desktop**: a vault list managed in-app (Obsidian-style switching), stored in the app's data directory.
  - **iOS**: vaults opened from Files via security-scoped bookmarks.

The Rust structs in `crates/kizunashelf/src/types.rs` are the source of truth for the schema, and the starter vault templates (used by web onboarding, desktop, and iOS vault creation) live in `crates/kizunashelf/src/templates.rs`. `config/vault-config.example.yaml` is generated from the "Media Library" starter template (`cargo run -p kizunashelf --example starter_yaml`) — don't edit it by hand.

## First Run

Start the web app during development, pointing it at a vault directory:

```bash
pnpm install
KIZUNASHELF_VAULT_ROOT=/path/to/your/vault pnpm dev
```

Open `http://localhost:5173/`. If that vault has no `KizunaShelf/config.yaml`, KizunaShelf redirects to `/onboarding` to create the schema (the web app never asks for a vault root — that comes from the environment).

On **desktop**, onboarding instead opens a native vault chooser: open an existing folder or create a new vault (a new vault is seeded with a starter media-tracker schema). On **iOS**, you pick the vault folder from Files.

The onboarding schema editor is the same structured editor used by Settings. Fill in:

- `Taxonomy root`: the collection root folder inside the vault; the default convention is `Taxonomy`, but any folder name works.
- `Types`: each collection folder you want KizunaShelf to index.
- `Fields`: frontmatter names for stable IDs, titles, images, enums, dates, external refs, and relations.
- Optional `Home` and `Daily Notes` sections.

Click `Create Vault`. KizunaShelf writes `KizunaShelf/config.yaml` into the vault, reloads the in-memory library, and opens the normal app.

## Design Model

KizunaShelf treats Markdown files as the durable source of truth. The config does not create a database schema; it describes how to read and edit Markdown files that already exist in an Obsidian-style vault.

The model has four layers:

1. `vaultRoot` points to the vault directory on disk.
2. `taxonomyRoot` points to the collection root inside the vault.
3. `types` map subfolders under `taxonomyRoot` to entity types.
4. `fields` map frontmatter keys to semantic roles such as title, cover, date, external reference, or relation.

For example, with:

```yaml
vaultRoot: /home/me/Vault
taxonomyRoot: Taxonomy
types:
- id: anime
  path: Anime
```

KizunaShelf reads Markdown files from:

```text
/home/me/Vault/Taxonomy/Anime/*.md
```

Each file becomes one entity. The entity id is normally:

```text
<type id>:<file basename>
```

If a type defines one or more `id` fields, the first configured `id` field with a value is used as the stable entity key instead of the filename.

## Where Settings Live

### Vault config (the schema)

The vault config always lives at a fixed location relative to the vault root:

```text
<vaultRoot>/KizunaShelf/config.yaml
```

It lives in a **visible** folder (`KizunaShelf/`, not a hidden dot-folder) on purpose: most Obsidian sync methods skip hidden files — including official Obsidian Sync, which has no hidden-file support at all — so a dot-folder config would silently fail to reach your other devices. A visible folder syncs with every method (Obsidian LiveSync, iCloud, Syncthing, …). The folder also reserves room for future app-owned, sync-worthy artifacts (e.g. saved lists under `KizunaShelf/lists/`). It is excluded from directory autocomplete so you don't nest entity collections inside it.

> **Official Obsidian Sync users:** Markdown always syncs, but `.yaml` is a non-Markdown extension, so enable **Settings → Sync → Sync all other types** (per device) for the config to travel. You may also want **Settings → Files & links → Detect all file extensions** so Obsidian shows the file at all. Other sync tools (LiveSync, iCloud, Syncthing) need no extra setting once the file is out of the dot-folder.

An example vault config (generated from the "Media Library" starter template) is available at `config/vault-config.example.yaml`. Onboarding is shown only when the vault has no `KizunaShelf/config.yaml` yet; when the vault already contains a synced one, any machine pointing at the vault picks up the schema automatically. Onboarding's template picker is served from the core by `GET /api/vault-templates`.

### App-level settings (per runtime)

There is no app config file. The vault root and write mode are sourced per runtime:

- **Self-hosted web** — environment variables only. `KIZUNASHELF_VAULT_ROOT` (default `/vault`) selects the single vault; `KIZUNASHELF_CONTENT_WRITABLE` and `KIZUNASHELF_SETTINGS_WRITABLE` control write modes. See [Runtime Environment](#runtime-environment).
- **Desktop** — a vault list (name + path + active selection) is stored as `vaults.json` in the platform app-data directory (e.g. `~/Library/Application Support/me.mudkip.kizunashelf-desktop/` on macOS). Vaults are content-writable.
- **iOS** — the vault list is stored as security-scoped bookmarks; vaults are content-writable.

### Provider token cache

The provider token cache holds derived OAuth tokens (re-derivable, never user secrets):

- **Web** — a single file outside the vault so it is never synced, defaulting to the system temp directory (`<tmp>/.kizunashelf.tokens.json`); override with `KIZUNASHELF_TOKEN_CACHE`.
- **Desktop / iOS** — the OS keychain, alongside provider credentials.

## Settings Editor

The Settings page at `/settings` edits the vault schema (every field below lives in the vault config). There is no longer an "App" section — the vault root and write mode are runtime settings (env vars on web; the vault switcher on desktop), not editable here.

- Vault: `taxonomyRoot`, `assetRoot`
- Daily notes: `paths`, `dateFormat`
- Home: `title`, section `id`, `title`, `type`, `limit`, `sort`, `direction`, and filters
- Types: `id`, `label`, `icon`, `path`, `filename`, `externalPriority`, `fields`
- Type fields: ordered field entries with `field`, `fieldType`, optional display metadata, enum options, date roles, title language, external source, and relation type
- Field types: `id`, `title`, `image`, `imageList`, `enum`, `enumList`, `progress`, `totalProgress`, `rating`, `bool`, `season`, `date`, `externalRef`, `relation`, `text`, `textList`

On **desktop**, Settings additionally shows a **Vaults** switcher (open / create / switch / forget vaults) and a **Provider Credentials** editor backed by the OS keychain. These are hidden on the web app, where credentials come from environment variables.

On the web app, path fields are normal text inputs with autocomplete suggestions from the API. In the desktop app, the same fields also show a folder button that opens the native folder picker.

Settings writes and path suggestions can be disabled with `KIZUNASHELF_SETTINGS_WRITABLE=false`. In production web mode, Settings writes default to enabled only for loopback hosts.

## Runtime Environment

The self-hosted web server is configured entirely through environment variables (there is no app config file):

| Variable | Description |
| --- | --- |
| `KIZUNASHELF_VAULT_ROOT` | Absolute path to the single vault this instance serves. Defaults to `/vault` (the conventional Docker mount). |
| `HOST` | Bind host. Defaults to `127.0.0.1`. Set `HOST=0.0.0.0` only when you intentionally want to expose it beyond the local machine. |
| `PORT` | Bind port. Defaults to `8787`. |
| `KIZUNASHELF_CONTENT_WRITABLE` | Enables entity create/edit/delete. Defaults to `true` for loopback hosts and `false` otherwise. |
| `KIZUNASHELF_SETTINGS_WRITABLE` | Enables schema (Settings) writes and path suggestions. Defaults to `true` for loopback hosts and `false` otherwise. |
| `KIZUNASHELF_CACHE_TTL_MS` | In-memory library cache TTL. Defaults to `10000`. |
| `KIZUNASHELF_TOKEN_CACHE` | Path for the provider OAuth token cache. Defaults to `<tmp>/.kizunashelf.tokens.json` (outside the vault). |
| `KIZUNASHELF_WEB_DIST` | Alternate web build path. |
| `KIZUNASHELF_SERVE_WEB` | Set to `false` to serve only the API. |
| `KIZUNASHELF_ALLOW_PRIVATE_ASSET_HOSTS` | Set to `true` to let asset downloads reach private/loopback/link-local addresses (e.g. a LAN image host). Off by default; the SSRF guard blocks them (the `198.18.0.0/15` benchmarking range is always allowed). |
| `KIZUNASHELF_IGDB_CLIENT_ID` | IGDB client id for external matching. |
| `KIZUNASHELF_IGDB_CLIENT_SECRET` | IGDB client secret for external matching. |
| `KIZUNASHELF_TVDB_API_KEY` | TheTVDB API key for external matching. |
| `KIZUNASHELF_TVDB_PIN` | Optional TheTVDB PIN. |
| `KIZUNASHELF_TMDB_API_KEY` | TMDB API key (v3) for movie/TV/person matching. |
| `KIZUNASHELF_SPOTIFY_CLIENT_ID` | Spotify client id for album/artist matching. |
| `KIZUNASHELF_SPOTIFY_CLIENT_SECRET` | Spotify client secret. |
| `KIZUNASHELF_DISCOGS_TOKEN` | Discogs personal access token for release/master matching. |
| `KIZUNASHELF_MAL_CLIENT_ID` | MyAnimeList API client id for anime/manga matching. |
| `KIZUNASHELF_COMICVINE_API_KEY` | Comic Vine API key for comic matching. |
| `KIZUNASHELF_HARDCOVER_API_KEY` | Hardcover API token (the full `Bearer …` value) for book matching. |

These provider-credential variables apply to the **web** runtime only. Each is derived mechanically from the credential key a provider declares in its catalog (`KIZUNASHELF_<UPPER_KEY>`), so a new credentialed provider needs no change here. The desktop and iOS apps read credentials from the OS keychain (entered in Settings → Provider Credentials, rendered from the same catalog), not from the environment. Run multiple instances — each with its own `KIZUNASHELF_VAULT_ROOT` and `PORT` — to serve multiple vaults.

The server does not enable wildcard CORS by default. Use the Vite dev proxy during development, or serve the built web app from the Rust process for production.

Mount your vault into the container (it defaults to `/vault`):

```bash
docker run -p 8787:8787 -v /path/to/vault:/vault kizunashelf
```

The Docker image sets `HOST=0.0.0.0` by default, so content and Settings writes default to off (non-loopback). Set `KIZUNASHELF_CONTENT_WRITABLE=true` and/or `KIZUNASHELF_SETTINGS_WRITABLE=true` only when you intentionally want writes available from the published container.

## Top-Level Schema

### App-level settings

These are not a config file; they are supplied by the runtime (see [Where Settings Live](#where-settings-live)):

| Setting | Web source | Desktop / iOS source | Description |
| --- | --- | --- | --- |
| vault root | `KIZUNASHELF_VAULT_ROOT` | active vault in the vault list | Absolute path to the vault root. |
| content writable | `KIZUNASHELF_CONTENT_WRITABLE` | always enabled | Enables entity create/edit/delete operations. |

### Vault config (`<vaultRoot>/KizunaShelf/config.yaml`)

```yaml
taxonomyRoot: Taxonomy
assetRoot: Assets
dailyNotes: ...
home: ...
types: [...]
```

| Key | Required | Type | Description |
| --- | --- | --- | --- |
| `taxonomyRoot` | yes | string | Path inside `vaultRoot` that contains typed entity folders. Must be relative. |
| `assetRoot` | no | string | Vault-relative directory where downloaded assets are stored. Defaults to `Assets`. |
| `dailyNotes` | no | object | Daily note paths and date extraction settings. |
| `tags` | no | object | Built-in tags field (see [Tags](#tags)). |
| `home` | no | object | Home dashboard sections. |
| `types` | yes | array | Entity type definitions. |

Path fields under the vault must be relative and cannot contain parent directory components (`..`). This is intentional: the app should not index or create files outside `vaultRoot`.

## Entity Types

Each entry in `types` describes one collection.

```yaml
types:
- id: anime
  label: Anime
  icon: TV
  path: Anime
  externalPriority:
  - bangumi
  filename:
    titleLanguage: zh
  bodySections:
  - heading: Summary
    kind: external
    externalFields:
    - { source: bangumi, field: summary }
  fields:
  - field: title
    fieldType: title
    displayName: Title
    titleLanguage: zh
```

| Key | Required | Type | Description |
| --- | --- | --- | --- |
| `id` | yes | string | Stable type id used in entity ids, URLs, API filters, and relation matching. Prefer lowercase ids such as `anime`, `games`, `book`. |
| `label` | yes | string | Human-readable label shown in the UI. |
| `icon` | no | string | Optional icon text for this type. |
| `path` | yes | string | Folder under `taxonomyRoot` that contains this type's Markdown files. |
| `externalPriority` | no | string[] | Preferred external metadata providers for match/search workflows. |
| `filename` | no | object | How the Markdown filename participates in titles. |
| `bodySections` | no | array | Declared body sections by heading: external-metadata mappings and the built-in episodes list. See [bodySections](#bodysections). |
| `fields` | no | array | Frontmatter field definitions. |

### Filename Config

The filename is often the most stable title source in an Obsidian vault. `filename` describes how it should be treated.

```yaml
filename:
  titleLanguage: zh
```

| Key | Type | Description |
| --- | --- | --- |
| `titleLanguage` | string | Adds the filename basename to `entity.titles` under this language key. Use ISO-like language keys such as `zh`, `ja`, or `en`. |
| `titleRole` | enum | Currently only `original`. Marks the filename basename as the `original` title — i.e. the language-agnostic fallback for `entity.title`. Use this for "the filename is the canonical/original title." |

Setting `filename.titleLanguage` makes the file basename selectable as that language's title (it is added to `entity.titles`). Setting `filename.titleRole: original` makes the basename the original-title fallback (it then wins over a `titleRole: original` field). If neither a title field nor the filename is marked `original`, `entity.title` falls back to the first title field, then any title, then the basename.

## Fields

Fields describe frontmatter keys. They do not need to cover every frontmatter property; unconfigured fields can still exist and will be shown as additional frontmatter.

```yaml
fields:
- field: cover_url
  fieldType: image
  displayName: Cover
```

| Key | Required | Type | Applies to | Description |
| --- | --- | --- | --- | --- |
| `field` | yes | string | all | Frontmatter key name. |
| `fieldType` | yes | enum | all | Semantic type. See field types below. |
| `displayName` | no | string | all | UI label. Outside Settings, the UI prefers `displayName` over raw field names. |
| `titleLanguage` | no | string | `title` | Language key for a title field. |
| `titleRole` | no | enum | `title` | Special title role. Currently only `original` — the title used as the language-agnostic fallback for `entity.title`. |
| `externalFields` | no | array | most fields | Maps external provider metadata fields into this frontmatter field. |
| `enumOptions` | no | string[] | `enum`, `enumList` | Allowed or suggested values in editors and filters. |
| `totalProgressField` | no | string | `progress` | Field that stores the total count for progress. |
| `dateRole` | no | enum | `date`, `season` | Whether the date is for planning, started, or completion. |
| `seasonLanguage` | no | enum | `season` | Season display/parser language: `zh`, `ja`, or `en`. |
| `externalRef` | no | string | `externalRef` | External provider represented by this URL/id field. |
| `externalTypes` | no | string[] | `externalRef` | Provider-specific type filters for external search. |
| `relationType` | no | string | `relation` | Target entity type expected for this relation field. |

### Field Types

| `fieldType` | Meaning |
| --- | --- |
| `id` | Stable id/key field for the entity. Useful when filenames may change. |
| `title` | A title or alternate title. Contributes to `entity.title` and/or `entity.titles`. |
| `image` | Single cover/image URL. |
| `imageList` | List of image URLs. The first useful image may be used as the cover. |
| `enum` | Single value from a known option set. |
| `enumList` | Multiple values from a known option set. |
| `progress` | Current progress count, usually paired with `totalProgress`. |
| `totalProgress` | Total count for a progress field. |
| `rating` | Numeric rating. |
| `bool` | Boolean flag. |
| `season` | Season or release window such as `2025`, `2025 Spring`, or localized season strings. Can be used in date views. |
| `date` | Date-like field. Exact dates are normalized for calendar links. |
| `externalRef` | URL or id pointing to an external source such as Bangumi, IGDB, TheTVDB, or MusicBrainz. |
| `relation` | Wikilink or list of wikilinks to other entities. |
| `text` | Generic scalar text. |
| `textList` | Generic list of text values. |

## Title Design

Titles are intentionally more structured than ordinary text fields.

KizunaShelf exposes:

| Entity property | Purpose |
| --- | --- |
| `entity.title` | Language-agnostic fallback title, used in search and wherever no viewer language applies. |
| `entity.titles` | Title map keyed by language, used for language switching, subtitle display, and search. |

There is no `defaultTitle` flag. The **displayed** title is chosen by the viewer's
language: the web app has a global language selector (defaulting to the browser
language); iOS uses the system language. Each surface resolves a title as:

1. The viewer language's title — `entity.titles[language]`
2. Otherwise `entity.title` (the language-agnostic fallback below)

The core computes `entity.title` as the fallback, in this order:

1. The `titleRole: original` title field, when present
2. Otherwise the first configured `fieldType: title` field
3. Otherwise the first value in `entity.titles` (e.g. a `filename.titleLanguage` basename)
4. Otherwise the filename basename

So the effective resolution is **selected language → original → other titles**.

All configured title fields are title data:

```yaml
fields:
- field: title
  fieldType: title
  titleLanguage: zh
- field: title_en
  fieldType: title
  titleLanguage: en
- field: title_original
  fieldType: title
  displayName: Title (Original)
  titleRole: original
```

Title keys are chosen like this:

| Field config | `entity.titles` key |
| --- | --- |
| `titleLanguage: en` | `en` |
| no `titleLanguage` | raw field key, for example `title_original` |
| `filename.titleLanguage: zh` | `zh` |

This means a `titleRole: original` field is still a title even if it is not language-specific. It appears in `entity.titles` under its field key and the UI can label it using `displayName`.

## Dates and Calendar Design

Dates are not just display fields. They drive:

- date badges in entity cards
- the entity detail Dates panel
- calendar views
- planning and completion views
- timeline analytics

Use `dateRole` to tell KizunaShelf what kind of date a field represents:

```yaml
- field: release_date
  fieldType: date
  displayName: Release date
  dateRole: planning

- field: complete_date
  fieldType: date
  displayName: Completed date
  dateRole: completed
```

Supported roles:

| `dateRole` | Meaning |
| --- | --- |
| `planning` | Future, release, airing, publish, or schedule date. |
| `started` | The date you started the entity (began watching, reading, or playing). Drives the "Just Started" planning list. |
| `completed` | Finished, watched, read, played, or completed date. |

`fieldType: season` can also use `dateRole`. It is useful when a collection uses seasons instead of exact dates.

```yaml
- field: season
  fieldType: season
  displayName: Season
  dateRole: planning
  seasonLanguage: zh
```

`seasonLanguage` supports:

| Value | Meaning |
| --- | --- |
| `zh` | Chinese season labels. |
| `ja` | Japanese season labels. |
| `en` | English season labels. |

Exact dates such as `2025-04-20` are normalized for calendar links. Broader values such as seasons and years are still useful for planning/timeline views.

## Relations

Relations are frontmatter wikilinks to other entities.

```yaml
- field: franchise
  fieldType: relation
  displayName: Franchise
  relationType: franchise
```

The frontmatter value can be a single wikilink:

```yaml
franchise: "[[Star Saga]]"
```

or a list:

```yaml
related:
- "[[Moon Quest]]"
- "[[Star Voyager]]"
```

`relationType` restricts matching to a target entity type. If omitted, KizunaShelf can match any entity basename.

Relations are directional:

- Outgoing: this entity links to another entity.
- Incoming: another entity links to this entity.

The library index stores both directions for resolved frontmatter relations, which lets the detail page show forward and backward relations separately.

Body wikilinks are also indexed as `body` relations when they point at known entities. Daily note wikilinks are indexed as `daily-note` relations.

## External Metadata

External metadata support has two pieces:

1. `externalRef` fields store links/ids to providers.
2. `externalFields` map provider metadata into local fields.
3. `bodySections` of `kind: external` map provider metadata into Markdown body sections.

Each provider exposes one `search` entry point that either resolves a pasted
URL/id it recognizes or runs a free-text query. Providers that only resolve
URLs/ids (no catalog search API) return nothing for free-text and resolve when
handed their URL/id — the catalog's `searchSupported` flag tells clients which
is which. The provider list, each provider's fields/types, and its credential
requirements all live in the Rust core and are exposed via
`/api/external/providers`; clients render from that rather than hard-coding.

Supported providers (keyless unless noted):

- **Bangumi** — books, anime, music, games, real (search + resolve).
- **TMDB** — movies, TV, people (search + resolve). Requires a TMDB API key.
- **IGDB** — games (search + resolve). Requires an IGDB (Twitch) client id and secret.
- **TheTVDB** — series, movies (search + resolve). Requires a TheTVDB API key; a PIN is optional.
- **Spotify** — albums, artists (search + resolve). Requires a Spotify client id and secret.
- **MusicBrainz** — releases, artists, release groups (search + resolve).
- **Discogs** — releases, masters (search + resolve). Requires a Discogs token.
- **Google Books** — books (search + resolve).
- **Open Library** — books, works (search + resolve).
- **Hardcover** — books (search + resolve, via the GraphQL API). Requires a Hardcover API token.
- **MyAnimeList** — anime, manga (search + resolve). Requires a MyAnimeList API client id.
- **MangaUpdates** — manga (search + resolve).
- **Comic Vine** — comic volumes (search + resolve). Requires a Comic Vine API key.
- **Apple Podcasts** — podcasts (search + resolve, via the iTunes API).
- **BoardGameGeek** — board games and expansions (search + resolve, via the BGG XML API).
- **Steam** — games (resolve a store URL/app id only; Steam has no public catalog search).

Credentials are supplied per runtime: the web app reads them from `KIZUNASHELF_*` environment variables (see the env table above — the var name is `KIZUNASHELF_<UPPER_KEY>` for each catalog credential key); the desktop and iOS apps store them in the OS keychain (entered under Settings → Provider Credentials, which renders its fields from the provider catalog).

```yaml
externalPriority:
- bangumi
- igdb

fields:
- field: bgm_url
  fieldType: externalRef
  displayName: BGM
  externalRef: bangumi
  externalTypes:
  - "2"

- field: title_en
  fieldType: title
  displayName: Title (English)
  titleLanguage: en
  externalFields:
  - source: igdb
    field: name
```

### `externalPriority`

At the type level, `externalPriority` controls provider order in external match/search workflows. Providers not configured through an `externalRef` field or an external `bodySections` entry are ignored.

### `externalRef`

`externalRef` declares which provider a field represents. The field value is stored in Markdown frontmatter, usually as a URL.

`externalTypes` is provider-specific. For example, a provider may use numeric or string type ids to distinguish anime, games, shows, or other records.

### `externalFields`

`externalFields` maps provider result metadata into local frontmatter fields. This is how "apply selected metadata" knows where to place values.

For example:

```yaml
- field: cover_url
  fieldType: image
  displayName: Cover
  externalFields:
  - source: bangumi
    field: cover_url
  - source: igdb
    field: cover_url
```

### `bodySections`

`bodySections` declares named sections of an entity's Markdown **body**, each addressed by its
heading. A section's `kind` chooses its behavior. (This generalizes the former `bodyMappings`.)

| Key | Required | Type | Description |
| --- | --- | --- | --- |
| `heading` | yes | string | The Markdown heading (text only) the section lives under. |
| `kind` | yes | `external` \| `episodes` | What the section is. |
| `externalFields` | for `external` | `{ source, field }[]` | Provider fields that fill this heading on match. One heading can list multiple sources — the matched candidate's provider is chosen (like a field's `externalFields`). |
| `tracking` | for `episodes` | `checklist` \| `progress` \| `none` | How watch/read state is tracked. Default `checklist`. |

```yaml
bodySections:
  # External-metadata section: "Summary" filled from whichever provider matched.
  - heading: Summary
    kind: external
    externalFields:
      - { source: bangumi, field: summary }
      - { source: thetvdb, field: overview }
  # Episodes section: an ordered, checkable list under "Episodes".
  - heading: Episodes
    kind: episodes
    tracking: checklist
```

When an external section is applied, KizunaShelf replaces the matching heading section if it
exists, else appends one; other body content is preserved.

### Episodes / tracks / chapters

An `episodes` body section is the built-in episode tracker. The section's body is an ordered
Markdown list, optionally grouped by **season/disc sub-headings**, with the item number written
in the item text (so `0`, `12.5`, specials work — Markdown ordered-list markers can't). With
`tracking: checklist`, items are task-list checkboxes that record exactly which are watched
(handling skips a `progress` field can't); the engine derives a watched/total roll-up shown in
the library and on the detail page.

```markdown
## Episodes
### Season 1
- [x] 1 · Pilot
- [ ] 12.5 · Recap (special)
### Season 2
- [ ] 1 · New Dawn
```

Multiple seasons can live as sub-headings in **one** entity, or as **separate** entities linked
by relations — the engine mirrors whatever the files contain and never merges or splits them.
The list is plain Markdown: edit it directly in Obsidian, or toggle items on the detail page.

## Home Page

The `home` section defines dashboard sections.

```yaml
home:
  title: Home
  sections:
  - id: recent-anime
    title: Recent Anime
    type: anime
    limit: 12
    sort: date:season
    direction: desc
    filters:
    - field: status
      values:
      - Watching
```

| Key | Required | Type | Description |
| --- | --- | --- | --- |
| `title` | no | string | Home page title. Defaults to `Home`. |
| `sections` | no | array | Ordered list of home sections. |

Each section:

| Key | Required | Type | Description |
| --- | --- | --- | --- |
| `id` | yes | string | Stable section id. |
| `title` | yes | string | Section heading. |
| `type` | yes | string | Entity type id to show. |
| `filters` | no | array | Frontmatter filters. |
| `limit` | no | number | Maximum items. Defaults to 12 and is clamped by the server. |
| `sort` | no | string | Sort key. Defaults to `title`. |
| `direction` | no | `asc` or `desc` | Sort direction. Defaults to `asc`. |

Filter semantics:

```yaml
filters:
- field: status
  values: [Watching, Playing]
```

- If `values` is empty, the field only needs to be present and non-null.
- If `values` is non-empty, scalar fields must equal one of the values.
- Array fields match when any item matches.

Common sort keys:

| Sort | Meaning |
| --- | --- |
| `title` | Sort by entity title. |
| `date:<field>` | Sort by a configured date/season field. |
| `relationCount` | Sort by number of related entities. |
| `path` | Sort by Markdown path. |

## Tags

`tags` is a **built-in, universal field**: a free-form list of labels every entity can
have, independent of its type. You don't declare it per type — it's always available, edited
with a search-and-add combobox over the whole vault's tag vocabulary, shown next to the type
on the detail view (not in "Details"), and filterable on the Library page.

```yaml
tags:
  field: tags   # the frontmatter key holding the tag list; defaults to "tags"
```

| Key | Required | Type | Description |
| --- | --- | --- | --- |
| `field` | no | string | Frontmatter key that holds an entity's tag list. Defaults to `tags`. |

Tags are the one place the engine treats a field by a fixed *role* across all types rather
than deriving meaning purely from per-type schema. To keep that honest, the **name is still
config**, not hardcoded: behavior reads `tags.field` (default `tags`), so you can rename or
relocate it vault-wide. A per-type schema field that happens to share this name is ignored in
favor of the built-in.

> **Why this is a vault-level field, not a `fieldType`.** A field earns built-in status only
> when it is (1) genuinely cross-type and universal, (2) declared in vault config with a
> default, (3) read from config rather than hardcoded, and (4) doing something the per-type
> schema can't express (here: a single global vocabulary and facet). Concepts that are
> per-type and schema-expressible (rating, status, …) stay ordinary schema fields.

## Daily Notes

Daily notes let KizunaShelf find entity mentions outside taxonomy files.

```yaml
dailyNotes:
  paths:
  - Daily Notes
  dateFormat: YYYY-MM-DD
```

| Key | Required | Type | Description |
| --- | --- | --- | --- |
| `paths` | no | string[] | Folders under `vaultRoot` to scan for daily note Markdown files. Defaults to `Daily Notes` when daily note features need paths. |
| `dateFormat` | no | moment format string | [Moment.js-style](https://momentjs.com/docs/#/parsing/string-format/) date format (the same syntax Obsidian's Daily Notes uses) describing the file path relative to the daily-notes folder, without the `.md` extension. Supports subfolders, e.g. `YYYY/MM/YYYY-MM-DD`. Defaults to `YYYY-MM-DD`. |

Daily note scanning:

- only reads Markdown files
- strips frontmatter before searching body wikilinks
- matches wikilinks against entity basenames
- extracts dates through `dateFormat`

The default date format matches filenames like:

```text
2025-04-20.md
```

## Complete Example

App-level settings (web): point the instance at the vault and allow writes.

```bash
KIZUNASHELF_VAULT_ROOT=/home/me/Vault
KIZUNASHELF_CONTENT_WRITABLE=true
```

(On desktop/iOS this vault is just an entry in the in-app vault list — no env vars.)

Vault config (`/home/me/Vault/KizunaShelf/config.yaml`):

```yaml
taxonomyRoot: Taxonomy

dailyNotes:
  paths:
  - Daily Notes
  dateFormat: YYYY-MM-DD

home:
  title: Home
  sections:
  - id: watching
    title: Watching
    type: anime
    limit: 12
    sort: date:season
    direction: desc
    filters:
    - field: status
      values:
      - Watching

types:
- id: anime
  label: Anime
  icon: TV
  path: Anime
  externalPriority:
  - bangumi
  filename:
    titleLanguage: zh
  fields:
  - field: id
    fieldType: id
    displayName: ID
  - field: title
    fieldType: title
    displayName: Title
    titleLanguage: zh
  - field: title_original
    fieldType: title
    displayName: Title (Original)
    titleRole: original
  - field: title_en
    fieldType: title
    displayName: Title (English)
    titleLanguage: en
  - field: cover_url
    fieldType: image
    displayName: Cover
  - field: status
    fieldType: enum
    displayName: Status
    enumOptions:
    - Backlog
    - Watching
    - Completed
  - field: season
    fieldType: season
    displayName: Season
    dateRole: planning
    seasonLanguage: zh
  - field: complete_date
    fieldType: date
    displayName: Completed date
    dateRole: completed
  - field: bgm_url
    fieldType: externalRef
    displayName: BGM
    externalRef: bangumi
    externalTypes:
    - "2"
  - field: franchise
    fieldType: relation
    displayName: Franchise
    relationType: franchise

- id: franchise
  label: Franchise
  path: Franchise
  filename:
    titleLanguage: zh
  fields:
  - field: title
    fieldType: title
    displayName: Title
    titleLanguage: zh
  - field: related
    fieldType: relation
    displayName: Related
    relationType: anime
```

## Design Guidelines

Use stable ids:

- Keep `types[].id` stable. It is part of entity ids and URLs.
- Add an `id` field if you expect filenames to change often.

Prefer semantic fields:

- Use `fieldType: title` for every title-like field, even original titles.
- Use `fieldType: date` or `season` plus `dateRole` for anything you want in calendar/planning views.
- Use `fieldType: relation` for entity links that should appear in relation views.

Use display names for UI:

- Raw frontmatter keys should stay machine-friendly: `title_en`, `cover_url`, `complete_date`.
- `displayName` should be user-friendly: `Title (English)`, `Cover`, `Completed date`.

Keep config close to the Markdown:

- KizunaShelf works best when the config describes your actual frontmatter instead of forcing every note into a new shape.
- Unconfigured frontmatter is allowed and remains visible in detail pages.
- Add field definitions when you want a property to drive browsing, editing, filtering, calendar views, external matching, or relation indexing.
