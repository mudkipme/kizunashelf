---
title: "Types & fields"
description: "The vault config document: top-level keys, entity types, filename config, and field definitions."
sidebar_position: 2
---

This page documents the vault config document itself — the top-level keys of `KizunaShelf/config.yaml`, the `types` array, and the `fields` that give frontmatter keys their meaning. For the concept behind the model — Markdown first, names are yours, roles carry meaning — see [Types, fields & your schema](../concepts/schema-driven.md). How each role *behaves* is covered by the sibling pages: [Titles, dates & status](./titles-dates-status.md), [External metadata & import](./external.md), and [Home, tags & daily notes](./home-tags-daily-notes.md).

## Top-Level Schema

The vault config lives at `<vaultRoot>/KizunaShelf/config.yaml`:

```yaml
taxonomyRoot: Taxonomy
assetRoot: Assets
dailyNotes: ...
types: [...]
```

| Key | Required | Type | Description |
| --- | --- | --- | --- |
| `taxonomyRoot` | yes | string | Path inside `vaultRoot` that contains typed entity folders. Must be relative. |
| `assetRoot` | no | string | Vault-relative directory where downloaded assets are stored. Defaults to `Assets`. |
| `dailyNotes` | no | object | Daily note paths and date extraction settings. |
| `tags` | no | object | Opt-in built-in tags field — tags exist only when `tags.field` is set (see [Tags](./home-tags-daily-notes.md#tags)). |
| `types` | yes | array | Entity type definitions. |

Path fields under the vault must be relative and cannot contain parent directory components (`..`). This is intentional: the app should not index or create files outside `vaultRoot`.

## Entity Types

Each entry in `types` describes one collection: KizunaShelf reads every Markdown file under `<taxonomyRoot>/<path>/` as one entity of that type. The entity id is normally `<type id>:<file basename>`; if the type defines one or more `id` fields, the first configured `id` field with a value is used as the stable entity key instead of the filename.

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
  log:
    lineFormat: "- {title} {note} #Anime"
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
| `bodySections` | no | array | Declared body sections by heading: external-metadata mappings and the built-in episodes list. See [bodySections](./external.md#bodysections). |
| `log` | no | object | Daily-note logging config for this type (`section`, `lineFormat`). **Its presence opts the type into logging.** See [Daily-note logging](./home-tags-daily-notes.md#daily-note-logging). |
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
| `fieldType` | yes | enum | all | Semantic type. See [Field Types](#field-types) below. |
| `displayName` | no | string | all | UI label. Outside Settings, the UI prefers `displayName` over raw field names. |
| `titleLanguage` | no | string | `title` | Language key for a title field. |
| `titleRole` | no | enum | `title` | Special title role. Currently only `original` — the title used as the language-agnostic fallback for `entity.title`. |
| `externalFields` | no | array | most fields | Maps external provider metadata fields into this frontmatter field. |
| `enumOptions` | no | string[] | `enum`, `enumList` | Allowed or suggested values in editors and filters. |
| `enumRole` | no | enum | `enum` | Semantic role of the enum field. Currently only `status` — marks the one field that represents the entity's lifecycle status. See [Status](./titles-dates-status.md#status). |
| `statusValues` | no | object | `enum` (with `enumRole: status`) | Maps each canonical status (`planning`, `ongoing`, `paused`, `completed`, `dropped`) to the user option strings that mean it. See [Status](./titles-dates-status.md#status). |
| `totalProgressField` | no | string | `progress` | Field that stores the total count for progress. |
| `dateRole` | no | enum | `date`, `season` | Whether the date is for planning, started, or completion. |
| `seasonLanguage` | no | enum | `season` | Season display/parser language: `zh`, `ja`, or `en`. |
| `externalRef` | no | string | `externalRef` | External provider represented by this URL/id field. |
| `externalTypes` | no | string[] | `externalRef` | Provider-specific type filters for external search. |
| `relationType` | no | string | `relation` | Target entity type expected for this relation field. |

### Field Types

Every `fieldType` — and every role enum's value set (`dateRole`, `titleRole`, `enumRole`, canonical statuses, `seasonLanguage`) — is enumerated with its meaning in [Field types & roles](./field-types.md), generated directly from the app so it always matches the version you're running.

### Relation fields

A `fieldType: relation` field holds entity links as ordinary wikilinks:

```yaml
- field: franchise
  fieldType: relation
  displayName: Franchise
  relationType: franchise
```

The frontmatter value can be a single wikilink or a list:

```yaml
franchise: "[[Steins;Gate]]"
related:
- "[[Robotics;Notes]]"
- "[[Steins;Gate 0 (Anime)]]"
```

`relationType` restricts matching to a target entity type; if omitted, KizunaShelf can match any entity basename. Configured relations are indexed in **both directions** (outgoing and incoming), and body and daily-note wikilinks are indexed alongside them — see [Relations](../features/relations.mdx) for how that plays out in the app.

## Design Guidelines

Use stable ids:

- Keep `types[].id` stable. It is part of entity ids and URLs.
- Add an `id` field if you expect filenames to change often.

Prefer semantic fields:

- Use `fieldType: title` for every title-like field, even original titles.
- Use `fieldType: date` or `season` plus `dateRole` for anything you want on the calendar or in the activity feed.
- Use `fieldType: relation` for entity links that should appear in relation views.

Use display names for UI:

- Raw frontmatter keys should stay machine-friendly: `title_en`, `cover_url`, `complete_date`.
- `displayName` should be user-friendly: `Title (English)`, `Cover`, `Completed date`.

Keep config close to the Markdown:

- KizunaShelf works best when the config describes your actual frontmatter instead of forcing every note into a new shape.
- Unconfigured frontmatter is allowed and remains visible in detail pages.
- Add field definitions when you want a property to drive browsing, editing, filtering, calendar views, external matching, or relation indexing.
