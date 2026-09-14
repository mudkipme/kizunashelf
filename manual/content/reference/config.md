---
title: "Configuration overview"
description: "What the vault config is, where it lives, and the map of this reference."
sidebar_position: 1
---

There are two kinds of configuration:

- **Vault config:** `KizunaShelf/config.yaml` defines entity folders, fields, and roles. It is shared with the vault. For the concept, see [Types, fields & your schema](../concepts/schema-driven.md).
- **App settings:** vault selection, write permissions, and credentials belong to the runtime. Desktop and iOS expose native controls; self-hosted web uses [environment variables](./web-server.md#configuration).

## The reference at a glance

| Page | Covers |
| --- | --- |
| [Types & fields](./schema.md) | The config.yaml document: top-level keys, entity types, filename config, field definitions, and design guidelines. |
| [Titles, dates & status](./titles-dates-status.md) | Title resolution, the language preference, date roles, seasons, and the canonical status model. |
| [External metadata & import](./external.md) | Provider wiring (`externalRef`, `externalFields`, `bodySections`), episode tracking and sync, Quick Capture, and library import. |
| [Home, tags & daily notes](./home-tags-daily-notes.md) | Home smart-list metadata, the built-in tags field, daily notes and logging. |
| [Field types & roles](./field-types.md) | Every `fieldType` and role enum (generated from the app). |
| [External providers](./providers.md) | Every provider, its credentials, types, and mappable fields (generated from the app). |
| [Web server](./web-server.md) | Environment variables, authentication, logs, and caches. |
| [Type presets](./presets.md) | Every built-in type preset (generated from the app). |

## First Run

Follow the [Quickstart](../start/quickstart.md) to create a vault from presets. The [preset catalog](./presets.md) lists the fields and provider mappings each preset supplies.

## Where the Vault Config Lives

The schema is always `<vaultRoot>/KizunaShelf/config.yaml`. Lists are stored alongside it in `KizunaShelf/Lists/`. See [Syncing your vault](../guides/syncing.md) for what to include when sharing a vault between devices.

## Settings Editor

Use the [schema editor](../features/schema.mdx) for the editing workflow. Structured saves serialize the typed schema; raw YAML saves validate strictly and preserve the accepted text, including comments. App settings such as the selected vault and credentials are separate from this file.

## Complete Example

This schema accompanies [Inside an entity file](../concepts/anatomy-of-an-entity.md). It uses English filenames, an original title, and an optional Chinese title. It is a small example, not the complete Anime preset.

```yaml
taxonomyRoot: Taxonomy
assetRoot: Assets

dailyNotes:
  paths:
  - Daily Notes
  dateFormat: YYYY-MM-DD

types:
- id: anime
  label: Anime
  path: Anime
  filename:
    titleLanguage: en
  log:
    lineFormat: "- {title} {note} #Anime"
  bodySections:
  - heading: Episodes
    kind: episodes
    tracking: checklist
  fields:
  - field: title_original
    fieldType: title
    titleRole: original
  - field: title_zh
    fieldType: title
    titleLanguage: zh
  - field: cover_url
    fieldType: image
  - field: status
    fieldType: enum
    enumRole: status
    enumOptions: [Backlog, Watching, Completed]
    statusValues:
      planning: [Backlog]
      ongoing: [Watching]
      completed: [Completed]
  - field: season
    fieldType: season
    dateRole: planning
    seasonLanguage: en
  - field: complete_date
    fieldType: date
    dateRole: completed
  - field: franchise
    fieldType: relation
    relationType: franchise

- id: franchise
  label: Franchise
  path: Franchise
  filename:
    titleLanguage: en
  fields: []
```
