---
title: "External metadata & import"
description: "Provider wiring, body sections, episode tracking and sync, Quick Capture, and library import."
sidebar_position: 5
---

## External Metadata

External metadata support has two pieces:

1. `externalRef` fields store links/ids to providers.
2. `externalFields` map provider metadata into local fields.
3. `bodySections` of `kind: external` map provider metadata into Markdown body sections.

Each provider exposes one `search` entry point that either resolves a pasted URL/id it recognizes or runs a free-text query. Providers that only resolve URLs/ids (no catalog search API) return nothing for free-text and resolve when handed their URL/id — the catalog's `searchSupported` flag tells clients which is which. The provider list, each provider's fields/types, and its credential requirements all live in the Rust core and are exposed via `/api/external/providers`; clients render from that rather than hard-coding.

The full provider catalog — every provider with its `externalRef` id, searchability, episode/track sync support, credentials (including each credential's exact `KIZUNASHELF_*` env var), `externalTypes` values, and mappable fields — is the [External providers](./providers.md) page, generated directly from the app so it always matches the version you're running.

Credentials are supplied per runtime: the web app reads them from `KIZUNASHELF_*` environment variables (listed per provider on the [External providers](./providers.md) page; the var name is `KIZUNASHELF_<UPPER_KEY>` for each catalog credential key); the desktop and iOS apps store them in the OS keychain (entered under Settings → Provider Credentials, which renders its fields from the provider catalog). Credential caveats worth knowing:

- **Discogs** accepts a personal access token **or** an app consumer key + secret (either works; the token wins when both are set). The personal token grants access to *its owner's* account — prefer the consumer pair for anything shared.
- **Hardcover**'s token is a personal account token (the full `Bearer …` value) — it can read and mutate its owner's Hardcover data, so it's per-user only, never bundled or distributed.
- **Comic Vine** keys are read-only but rate-limited per key — per-user only.
- **Google Books** works keyless in principle, but keyless access shares an exhausted quota and returns 429 — a key is effectively required.
- **BoardGameGeek** requires a registered application token — BGG returns 401 without one (register at [boardgamegeek.com/using_the_xml_api](https://boardgamegeek.com/using_the_xml_api)).

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

`bodySections` declares named sections of an entity's Markdown **body**, each addressed by its heading. A section's `kind` chooses its behavior. (This generalizes the former `bodyMappings`.)

| Key | Required | Type | Description |
| --- | --- | --- | --- |
| `heading` | yes | string | The Markdown heading (text only) the section lives under. |
| `kind` | yes | `external` \| `episodes` | What the section is. |
| `externalFields` | for `external` | `{ source, field }[]` | Provider fields that fill this heading on match. One heading can list multiple sources — the matched candidate's provider is chosen (like a field's `externalFields`). |
| `tracking` | for `episodes` | `checklist` \| `none` | How watch/read state is tracked. Default `checklist`. |

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

When an external section is applied, KizunaShelf replaces the matching heading section if it exists, else appends one; other body content is preserved.

### Episodes / tracks / chapters

An `episodes` body section is the built-in episode tracker. The section's body is an ordered Markdown list, optionally grouped by **season/disc sub-headings**, with the item number written in the item text (so `0`, `12.5`, specials work — Markdown ordered-list markers can't). With `tracking: checklist`, items are task-list checkboxes that record exactly which are watched (handling skips a `progress` field can't); the engine derives a watched/total roll-up shown in the library and on the detail page.

```markdown
## Episodes
### Season 1
- [x] 1 · Pilot
- [ ] 12.5 · Recap (special)
### Season 2
- [ ] 1 · New Dawn
```

Multiple seasons can live as sub-headings in **one** entity, or as **separate** entities linked by relations — the engine mirrors whatever the files contain and never merges or splits them. The list is plain Markdown: edit it directly in Obsidian, or toggle items on the detail page.

**Syncing from a provider.** When an entity links an `externalRef` to a source that exposes a list, the detail page's episodes panel offers a **Sync** action that pulls the provider's list into a checkable preview and merges the ticked items in — new items are added and ticked existing ones have their title updated, while your watched ticks and hand-added entries are always kept. Which providers can supply a list is the *Episode/track sync* column of the [External providers](./providers.md) table; series providers map episodes, music providers map tracks (grouped by disc), and Comic Vine maps a volume's issues. The provider must be configured (credentials set, if it needs any) and linked on the entity; when more than one such source is linked, the dialog lets you choose which to sync from.

## Quick Capture and Import

The external-metadata wiring above powers two ways to create entities from provider data. Both create real Markdown files and are gated by content-write mode; both reuse the same **"in library" detection** so they never make a duplicate.

### Quick Capture

Quick Capture (`/entities/new`) is the single-add flow: search the providers a type maps (or paste a provider URL), pick a match, and one click creates the entity. The server re-runs the schema mapping (it never trusts client-sent values), fills every mapped field and body section, downloads covers, and imports the episode list — covers and episodes are fail-safe, so a flaky provider never blocks the creation. If the candidate already resolves to a library entity (by external ref or a loose title match), Quick Capture opens that entity instead of adding a copy.

### Import

Import (`/entities/import`) brings in a whole library from another service. It is a three-step, **review-before-write** job: *fetch & plan* (pull the source and resolve every item against your schema), *review* (see what will be created, skipped as already-in-library, or needs manual attention — and pick a target type where a bucket maps to more than one), then *commit* (create the approved entities). Nothing is written until you commit, and re-running the same import is safe — anything already created is skipped.

Only **public profiles** are supported. Each source resolves its items to one of the built-in providers, so **a type must declare an `externalRef` field for that provider** to receive them (e.g. an `externalRef: myanimelist` field to import MyAnimeList/AniList/Kitsu, `externalRef: tmdb` for Trakt/IMDb, `externalRef: steam` for Steam, `externalRef: openlibrary` for Goodreads, `externalRef: bangumi` for Bangumi). The item's provider *type* (anime, movie, game, …) is matched against that field's `externalTypes`.

| Source | Input | Resolves to | Credential |
| --- | --- | --- | --- |
| Bangumi | username | `bangumi` | — |
| MyAnimeList | username | `myanimelist` | MyAnimeList client id (same as search) |
| AniList | username | `myanimelist` (via each entry's MAL id) | — |
| Kitsu | username | `myanimelist` (via each entry's MAL mapping) | — |
| Trakt | username (slug) | `tmdb` | Trakt client id |
| Steam | SteamID64 | `steam` | Steam Web API key |
| IMDb | CSV export | `tmdb` (via `/find`) | TMDB API key |
| Goodreads | CSV export | `openlibrary` (via ISBN) | — |
| Yamtrack | CSV export | `myanimelist`, `tmdb` | — |

Import credentials are supplied like provider credentials — `KIZUNASHELF_*` env vars on web, the OS keychain on desktop/iOS (keys `trakt_client_id`, `steam_api_key`). A source with a missing required credential is shown but disabled, with the reason.

**Your data maps through schema roles, not field names.** For each imported item, the source's status is translated to a canonical (`planning`/`ongoing`/`paused`/`completed`/`dropped`) and written to the type's [`enumRole: status`](./titles-dates-status.md#status) field via its `statusValues`; the score goes to the first [`rating`](./field-types.md) field (normalized to 0–10); started/finished dates go to the [`dateRole`](./titles-dates-status.md#dates-and-calendar-design) `started`/`completed` fields; notes become an unmanaged `## Notes` body section; and watched progress ticks the first *N* items of the [episodes](#episodes--tracks--chapters) section. A role you haven't wired is simply skipped. Which of these run is controlled by per-import toggles (import user data, import episodes, mark progress).

Import does **not** download covers — imported image fields keep their remote URLs. Fetch the local copies afterward with the batch cover downloader (the *Download remote covers* panel on the Review page), which runs one job over a type or the whole library.
