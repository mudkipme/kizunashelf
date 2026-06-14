# KizunaShelf Config

KizunaShelf is schema-driven. Configuration is split across two YAML files:

- **App config** — stored on this machine (e.g. `~/.config/kizunashelf.yaml`). It tells the app where your vault lives and how this machine runs. It is local and not synced.
- **Vault config** — stored inside the vault at `<vaultRoot>/.kizunashelf/config.yaml`. It describes which Markdown folders are entity collections and how frontmatter fields should be interpreted. Because it lives in the vault, it travels with the vault and is synced by the vault's own syncing method, so every machine pointing at the vault shares the same schema.

The Rust structs in `crates/kizunashelf/src/types.rs` are the source of truth for the schema. `config/kizunashelf.example.yaml` (app) and `config/vault-config.example.yaml` (vault) are the working examples.

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

## Config File Location

### App config

The server reads `KIZUNASHELF_CONFIG` if it is set. Otherwise it uses:

```text
config/kizunashelf.yaml
```

An example app config is available at `config/kizunashelf.example.yaml`.

The desktop app searches for the app config `kizunashelf.yaml` in this order:

1. `KIZUNASHELF_CONFIG`
2. `$XDG_CONFIG_HOME/kizunashelf.yaml`
3. `~/.config/kizunashelf.yaml`
4. `$XDG_CONFIG_DIR/kizunashelf.yaml`
5. Each `$XDG_CONFIG_DIRS` entry
6. On macOS, `~/Library/Application Support/kizunashelf.yaml`
7. On macOS, `~/Library/Application Support/KizunaShelf/kizunashelf.yaml`

If none of those files exist, desktop opens onboarding and writes the new app config to the first candidate path.

### Vault config

The vault config always lives at a fixed location relative to the configured `vaultRoot`:

```text
<vaultRoot>/.kizunashelf/config.yaml
```

KizunaShelf resolves it from the app config's `vaultRoot`. An example vault config is available at `config/vault-config.example.yaml`.

Onboarding is shown when either file is missing. When `vaultRoot` already points at a vault that contains a synced `.kizunashelf/config.yaml`, a fresh machine only needs the app config — the vault schema is picked up automatically.

The provider token cache (`.kizunashelf.tokens.json`) is written next to the app config and stays local; it is never placed inside the vault.

## Settings Editor

The Settings page at `/settings` can edit every config field:

- App: `vaultRoot`, `contentWritable` (`readConcurrency` is round-tripped but not surfaced in the UI)
- Vault: `taxonomyRoot`, `assetRoot`
- Daily notes: `paths`, `datePattern`, `snippetMaxLength`
- Home: `title`, section `id`, `title`, `type`, `limit`, `sort`, `direction`, and filters
- Types: `id`, `label`, `icon`, `path`, `filename`, `externalPriority`, `fields`
- Type fields: ordered field entries with `field`, `fieldType`, optional display metadata, enum options, date roles, title language, external source, and relation type
- Field types: `id`, `title`, `image`, `imageList`, `enum`, `enumList`, `progress`, `totalProgress`, `rating`, `bool`, `season`, `date`, `externalRef`, `relation`, `text`, `textList`

On the web app, path fields are normal text inputs with autocomplete suggestions from the API. In the desktop app, the same fields also show a folder button that opens the native folder picker.

Settings writes and path suggestions can be disabled with `KIZUNASHELF_SETTINGS_WRITABLE=false`. In production web mode, Settings writes default to enabled only for loopback hosts.

## Runtime Environment

The production web server is configured through environment variables:

| Variable | Description |
| --- | --- |
| `HOST` | Bind host. Defaults to `127.0.0.1`. Set `HOST=0.0.0.0` only when you intentionally want to expose it beyond the local machine. |
| `PORT` | Bind port. Defaults to `8787`. |
| `KIZUNASHELF_CONFIG` | Config file path. |
| `KIZUNASHELF_CACHE_TTL_MS` | In-memory library cache TTL. Defaults to `10000`. |
| `KIZUNASHELF_WEB_DIST` | Alternate web build path. |
| `KIZUNASHELF_SERVE_WEB` | Set to `false` to serve only the API. |
| `KIZUNASHELF_SETTINGS_WRITABLE` | Enables Settings writes and path suggestions. Defaults to `true` for loopback hosts and `false` for non-loopback hosts. |
| `KIZUNASHELF_IGDB_CLIENT_ID` | IGDB client id for external matching. |
| `KIZUNASHELF_IGDB_CLIENT_SECRET` | IGDB client secret for external matching. |
| `KIZUNASHELF_TVDB_API_KEY` | TheTVDB API key for external matching. |
| `KIZUNASHELF_TVDB_PIN` | Optional TheTVDB PIN. |

The server does not enable wildcard CORS by default. Use the Vite dev proxy during development, or serve the built web app from the Rust process for production.

The Docker image sets `HOST=0.0.0.0` and `KIZUNASHELF_SETTINGS_WRITABLE=false` by default. Set `KIZUNASHELF_SETTINGS_WRITABLE=true` only when you intentionally want onboarding/settings writes available from the published container.

## Top-Level Schema

### App config (`~/.config/kizunashelf.yaml`)

```yaml
vaultRoot: /path/to/ObsidianVault
contentWritable: true
readConcurrency: 8
```

| Key | Required | Type | Description |
| --- | --- | --- | --- |
| `vaultRoot` | yes | string | Absolute path to the vault root. |
| `contentWritable` | no | boolean | Enables entity create/edit/delete operations when true. |
| `readConcurrency` | no | number | Maximum concurrent entity file reads. Defaults to 8 and is clamped from 1 to 16. |

### Vault config (`<vaultRoot>/.kizunashelf/config.yaml`)

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
    defaultTitle: true
  bodyMappings:
  - source: bangumi
    field: summary
    heading: Summary
  fields:
  - field: title
    fieldType: title
    displayName: Title
    titleLanguage: zh
    defaultTitle: true
```

| Key | Required | Type | Description |
| --- | --- | --- | --- |
| `id` | yes | string | Stable type id used in entity ids, URLs, API filters, and relation matching. Prefer lowercase ids such as `anime`, `games`, `book`. |
| `label` | yes | string | Human-readable label shown in the UI. |
| `icon` | no | string | Optional icon text for this type. |
| `path` | yes | string | Folder under `taxonomyRoot` that contains this type's Markdown files. |
| `externalPriority` | no | string[] | Preferred external metadata providers for match/search workflows. |
| `filename` | no | object | How the Markdown filename participates in titles. |
| `bodyMappings` | no | array | Maps external provider metadata fields into Markdown body sections by heading. |
| `fields` | no | array | Frontmatter field definitions. |

### Filename Config

The filename is often the most stable title source in an Obsidian vault. `filename` describes how it should be treated.

```yaml
filename:
  titleLanguage: zh
  defaultTitle: true
```

| Key | Type | Description |
| --- | --- | --- |
| `titleLanguage` | string | Adds the filename basename to `entity.titles` under this language key. Use ISO-like language keys such as `zh`, `ja`, or `en`. |
| `defaultTitle` | boolean | Uses the filename basename as the entity's primary `title`. |

If `filename.defaultTitle` is true, the primary title is the Markdown file basename. Other configured title fields can still appear as alternate titles.

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
| `titleRole` | no | enum | `title` | Special title role. Currently only `original`. |
| `defaultTitle` | no | boolean | `title` | Uses this title field as primary `entity.title` when present. |
| `externalFields` | no | array | most fields | Maps external provider metadata fields into this frontmatter field. |
| `enumOptions` | no | string[] | `enum`, `enumList` | Allowed or suggested values in editors and filters. |
| `totalProgressField` | no | string | `progress` | Field that stores the total count for progress. |
| `dateRole` | no | enum | `date`, `season` | Whether the date is for planning or completion. |
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
| `entity.title` | Primary title used in lists, cards, headings, and search. |
| `entity.titles` | Alternate title map used for language switching, subtitle display, and search. |

Primary title selection follows this order:

1. `filename.defaultTitle: true`
2. First `fieldType: title` field with `defaultTitle: true`
3. Filename basename if `filename` is configured
4. First configured `fieldType: title`
5. First value in `entity.titles`
6. Filename basename

All configured title fields are title data:

```yaml
fields:
- field: title
  fieldType: title
  titleLanguage: zh
  defaultTitle: true
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
| `planning` | Future, release, airing, publish, start, or schedule date. |
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
3. `bodyMappings` map provider metadata into Markdown body sections.

Supported providers:

- Bangumi: works without extra credentials.
- IGDB: requires `KIZUNASHELF_IGDB_CLIENT_ID` and `KIZUNASHELF_IGDB_CLIENT_SECRET`.
- TheTVDB: requires `KIZUNASHELF_TVDB_API_KEY`; `KIZUNASHELF_TVDB_PIN` is optional.

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

At the type level, `externalPriority` controls provider order in external match/search workflows. Providers not configured through an `externalRef` field or `bodyMappings` are ignored.

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

### `bodyMappings`

`bodyMappings` maps provider result metadata into Markdown body sections. The heading is matched from the schema, not inferred from an entity type id or provider name.

When applying a selected body mapping, KizunaShelf replaces the matching heading section if it exists. If the heading does not exist, it appends a new section. Other body content is preserved.

```yaml
bodyMappings:
- source: bangumi
  field: summary
  heading: Summary
- source: thetvdb
  field: overview
  heading: Summary
```

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

## Daily Notes

Daily notes let KizunaShelf find entity mentions outside taxonomy files.

```yaml
dailyNotes:
  paths:
  - Daily Notes
  datePattern: '^(?:Daily Notes/)?(?<date>\d{4}-\d{2}-\d{2})\.md$'
  snippetMaxLength: 260
```

| Key | Required | Type | Description |
| --- | --- | --- | --- |
| `paths` | no | string[] | Folders under `vaultRoot` to scan for daily note Markdown files. Defaults to `Daily Notes` when daily note features need paths. |
| `datePattern` | no | regex string | Regex used to extract a date from the relative path or basename. Use a named group `date` or the first capture group. |
| `snippetMaxLength` | no | number | Maximum snippet length shown around daily-note mentions. Defaults to 260 and is clamped from 80 to 600. |

Daily note scanning:

- only reads Markdown files
- strips frontmatter before searching body wikilinks
- matches wikilinks against entity basenames
- extracts dates through `datePattern`

The default date pattern matches filenames like:

```text
2025-04-20.md
```

## Complete Example

App config (`~/.config/kizunashelf.yaml`):

```yaml
vaultRoot: /home/me/Vault
contentWritable: true
readConcurrency: 8
```

Vault config (`/home/me/Vault/.kizunashelf/config.yaml`):

```yaml
taxonomyRoot: Taxonomy

dailyNotes:
  paths:
  - Daily Notes
  datePattern: '^(?:Daily Notes/)?(?<date>\d{4}-\d{2}-\d{2})\.md$'
  snippetMaxLength: 260

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
    defaultTitle: true
  fields:
  - field: id
    fieldType: id
    displayName: ID
  - field: title
    fieldType: title
    displayName: Title
    titleLanguage: zh
    defaultTitle: true
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
    defaultTitle: true
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
