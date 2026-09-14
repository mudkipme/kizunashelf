---
title: "Configuration overview"
description: "What the vault config is, where it lives, and the map of this reference."
sidebar_position: 1
---

KizunaShelf is schema-driven — the model itself (Markdown first, names are yours, roles carry meaning) is explained in [Schema-Driven by design](../concepts/schema-driven.md). There are two kinds of configuration:

- **Vault config (the schema)** — stored inside the vault at `<vaultRoot>/KizunaShelf/config.yaml`. It describes which Markdown folders are entity collections and how frontmatter fields should be interpreted. Because it lives in the vault, it travels with the vault and is synced by the vault's own syncing method, so every machine pointing at the vault shares the same schema (see [Syncing KizunaShelf](../guides/syncing.md)). Its options are documented across the per-topic pages of this reference (see [the map below](#the-reference-at-a-glance)).
- **App-level settings** — *where* the vault is and *how this runtime behaves* (writable or read-only). These are **not** a synced file and not part of the schema: on desktop and iOS they're plain in-app UI (the vault switcher and Provider Credentials), and for the self-hosted web app they're the environment variables documented in [Self-hosting](../start/self-hosting.md#configuration).

## The reference at a glance

| Page | Covers |
| --- | --- |
| [Types & fields](./schema.md) | The config.yaml document: top-level keys, entity types, filename config, field definitions, and design guidelines. |
| [Titles, dates & status](./titles-dates-status.md) | Title resolution, the language preference, date roles, seasons, and the canonical status model. |
| [External metadata & import](./external.md) | Provider wiring (`externalRef`, `externalFields`, `bodySections`), episode tracking and sync, Quick Capture, and library import. |
| [Home, tags & daily notes](./home-tags-daily-notes.md) | Home smart-list metadata, the built-in tags field, daily notes and logging. |
| [Field types & roles](./field-types.md) | Every `fieldType` and role enum (generated from the app). |
| [External providers](./providers.md) | Every provider, its credentials, types, and mappable fields (generated from the app). |
| [Type presets](./presets.md) | Every built-in type preset (generated from the app). |

## First Run

Start the web app pointed at a vault — a single `docker run` away; see [Self-hosting](../start/self-hosting.md) — and open it in your browser. If that vault has no `KizunaShelf/config.yaml`, KizunaShelf redirects to `/onboarding` to create the schema (the web app never asks for a vault root — that comes from the environment).

On **desktop**, onboarding instead opens a native vault chooser: open an existing folder or create a new, empty vault. On **iOS**, the native vault manager can create a managed vault under On My iPhone or open an existing folder from Files, iCloud Drive, or another File Provider.

Onboarding is a **type-preset picker**. Choose a title language, then pick the built-in types you want — movies, TV, anime, manga, games, books, music, and more (the full catalog, with every preset's fields and provider wiring, is the generated [Type presets](./presets.md) page). Each preset is one fully-wired type: its title/cover/date/status/progress fields and external-provider mappings are already set (the presets reuse the same provider catalog as external matching, so they never drift). You can add or drop types and refine any field in the structured editor — the same editor Settings uses — before saving.

Click `Create Vault`. KizunaShelf resolves the chosen presets (stamping your language onto title fields and keeping only the relations whose target type you also picked), writes `KizunaShelf/config.yaml` into the vault, reloads the in-memory library, creates suggested smart lists pinned to Home, and opens the normal app. You can recreate suggestions later using **Add suggested lists** on Home. Everything a preset sets is ordinary schema you can edit later; nothing is special-cased.

## Where the Vault Config Lives

The vault config always lives at a fixed location relative to the vault root:

```text
<vaultRoot>/KizunaShelf/config.yaml
```

It lives in a **visible** folder (`KizunaShelf/`, not a hidden dot-folder) on purpose, so that it syncs with every method — see [Syncing KizunaShelf](../guides/syncing.md) for why, and for how to sync a vault across devices. The folder also holds other app-owned, sync-worthy artifacts (e.g. saved lists under `KizunaShelf/Lists/`). It is excluded from directory autocomplete so you don't nest entity collections inside it.

Onboarding is shown only when the vault has no `KizunaShelf/config.yaml` yet; when the vault already contains a synced one, any machine pointing at the vault picks up the schema automatically. The type presets that back onboarding (and the "add a built-in type" picker in Settings) are cataloged on the [Type presets](./presets.md) page.

## Settings Editor

The web/desktop Settings page at `/settings` edits the vault schema (every field below lives in the vault config). There is no "App" section — the vault root and write mode are runtime settings, not synced schema. iOS exposes the same schema through its native **More → Vault Schema** editor.

Web/desktop offers both a structured **Form** and a raw **YAML** editor. The raw editor validates the complete document strictly and writes the accepted text verbatim, preserving comments and formatting. The structured editor works with the typed schema and reserializes it on save. iOS currently provides the structured native editor; edit `KizunaShelf/config.yaml` in a text editor when you need direct YAML/comment control.

- Vault: `taxonomyRoot`, `assetRoot`
- Daily notes: `paths`, `dateFormat`
- Types: `id`, `label`, `icon`, `path`, `filename`, `externalPriority`, `fields`
- Type fields: ordered field entries with `field`, `fieldType` (every type is listed in [Field types & roles](./field-types.md)), optional display metadata, enum options, date roles, title language, external source, and relation type

On **desktop**, Settings additionally shows a **Vaults** switcher (open / create / switch / forget vaults) and a **Provider Credentials** editor backed by the OS keychain. On **iOS**, vault switching lives under **More → Switch Vault…**, and **More → Provider Credentials** renders the same core-owned credential catalog into Keychain-backed native fields. These controls are absent from the web app, where the active vault and credentials come from environment variables.

On the web app, path fields are normal text inputs with autocomplete suggestions from the API. In the desktop app, the same fields also show a folder button that opens the native folder picker. The iOS schema editor uses the same vault-relative suggestion endpoint through a native suggestion field.

On self-hosted web, Settings writes and path suggestions can be disabled with `KIZUNASHELF_SETTINGS_WRITABLE=false`; they default to enabled only for loopback hosts. Desktop and iOS treat an opened vault as schema-writable.

## Complete Example

A vault config (`/home/me/Vault/KizunaShelf/config.yaml`):

```yaml
taxonomyRoot: Taxonomy

dailyNotes:
  paths:
  - Daily Notes
  dateFormat: YYYY-MM-DD

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
