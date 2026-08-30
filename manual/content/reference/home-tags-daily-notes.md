---
title: "Home, tags & daily notes"
description: "Home dashboard sections and criteria, the built-in tags field, and daily notes with logging."
sidebar_position: 6
---

## Home Page

The `home` section defines dashboard sections. (The page *title* is not configurable — every client renders a localized "Home".)

```yaml
home:
  sections:
  - id: recent-anime
    title: Recent Anime
    type: anime
    limit: 12
    sort: date:season
    direction: desc
    criteria:
      conjunction: all
      rules:
      - kind: compare
        field: status
        op: eq
        value: Watching
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
| `criteria` | no | object | Smart-list-grade criteria (see below). Absent means every entry of the type matches. |
| `limit` | no | number | Maximum items. Defaults to 12 and is clamped by the server. |
| `sort` | no | string | Sort key. Defaults to `title`. |
| `direction` | no | `asc` or `desc` | Sort direction. Defaults to `asc`. |

Criteria semantics — the same rule model and evaluation engine as smart lists (`.base` files), stored structurally because the vault config is strict-parsed YAML rather than a Bases file:

```yaml
criteria:
  conjunction: all        # all | any | none
  rules:
  - kind: compare         # compare | contains | startsWith | endsWith |
    field: status         #   isEmpty | hasTag | linksTo | inFolder
    op: eq                # eq | ne | gt | gte | lt | lte
    value: Watching       # or: number, boolean, date (YYYY-MM-DD), relative
  - kind: compare
    field: complete_date
    op: gte
    relative: { amount: 30, unit: days }   # "in the last 30 days"
  groups:                 # one nesting level of subgroups
  - conjunction: any
    rules:
    - { kind: compare, field: rating, op: gte, number: 8 }
    - { kind: hasTag, values: [favorites] }
```

Rule kinds mirror the smart-list rule builder: `contains` matches list fields by membership and string fields by substring; `isEmpty` with `negated: true` reads as "has a value"; `hasTag` matches the built-in [tags](#tags) field (and matches nothing while tags are disabled); `linksTo` matches an outgoing wikilink/relation to the named entity. Field meaning is value-driven, exactly like Bases — a comparison is a date comparison because the right-hand side is a date, never because of the field's name.

Common sort keys:

| Sort | Meaning |
| --- | --- |
| `title` | Sort by entity title. |
| `date:<field>` | Sort by a configured date/season field. |
| `relationCount` | Sort by number of related entities. |
| `path` | Sort by Markdown path. |

## Tags

`tags` is a **built-in, universal field**: a free-form list of labels every entity can have, independent of its type. You don't declare it per type — but the feature is **opt-in**: it exists only when the vault config sets `tags.field`. When enabled, tags are edited with a search-and-add combobox over the whole vault's tag vocabulary, shown next to the type on the detail view (not in "Details"), and filterable on the Library page. Without a `tags` block, none of that UI appears, no tags are derived, and a frontmatter key named `tags` is just an ordinary field.

```yaml
tags:
  field: tags   # the frontmatter key holding the tag list; setting it enables the feature
```

Enable it from **Settings → Tags** (toggle it on and name the key — `tags` is the seeded default), or add the block to `config.yaml` by hand.

| Key | Required | Type | Description |
| --- | --- | --- | --- |
| `field` | yes | string | Frontmatter key that holds an entity's tag list. Setting it turns the tags feature on; there is no default — omit the `tags` block to keep tags disabled. |

Tags are the one place the engine treats a field by a fixed *role* across all types rather than deriving meaning purely from per-type schema. To keep that honest, the feature is opt-in and the **name is config**, not hardcoded: behavior reads `tags.field`, so you choose whether the vault has tags at all and, if so, which key holds them. A per-type schema field that happens to share the configured name is ignored in favor of the built-in; with tags disabled, that same schema field behaves like any other (a relation field named `tags` builds relations, for example).

> **Why this is a vault-level field, not a `fieldType`.** A field earns built-in status only when it is (1) genuinely cross-type and universal, (2) declared explicitly in vault config, (3) read from config rather than hardcoded, and (4) doing something the per-type schema can't express (here: a single global vocabulary and facet). Concepts that are per-type and schema-expressible (rating, status, …) stay ordinary schema fields.

## Daily Notes

Daily notes let KizunaShelf find entity mentions outside taxonomy files.

```yaml
dailyNotes:
  paths:
  - Daily Notes
  dateFormat: YYYY-MM-DD
  template: Templates/Daily Note.md
  log:
    section: Log
    lineFormat: "- {title} {note}"
```

| Key | Required | Type | Description |
| --- | --- | --- | --- |
| `paths` | no | string[] | Folders under `vaultRoot` to scan for daily note Markdown files. Defaults to `Daily Notes` when daily note features need paths. |
| `dateFormat` | no | moment format string | [Moment.js-style](https://momentjs.com/docs/#/parsing/string-format/) date format (the same syntax Obsidian's Daily Notes uses) describing the file path relative to the daily-notes folder, without the `.md` extension. Supports subfolders, e.g. `YYYY/MM/YYYY-MM-DD`. Defaults to `YYYY-MM-DD`. |
| `template` | no | string | Vault-relative path to a template used to seed a daily note that doesn't exist yet (date tokens substituted). Absent → a new note starts empty. |
| `log` | no | object | Global defaults for daily-note logging: `section` (heading to write log lines under) and `lineFormat` (the line template). Per-type `log` blocks override these. See [Daily-note logging](#daily-note-logging). |

Daily note scanning:

- only reads Markdown files
- strips frontmatter before searching body wikilinks
- matches wikilinks against entity basenames
- extracts dates through `dateFormat`

The default date format matches filenames like:

```text
2025-04-20.md
```

### Daily-note logging

The Log action appends a line to the day's daily note. The *shape* of that line is schema-driven, configured in two places:

- **`dailyNotes.log`** — global defaults: `section` (the heading to write under, as raw heading text — no `#`, default h2, the same convention as [`bodySections`](./external.md#bodysections)) and `lineFormat` (the line template).
- **`types[].log`** — per-type override, and the **opt-in**: a type is loggable *only if* it declares a `log` block. Same `section` / `lineFormat` keys. The type's hashtag is written as a **literal inside `lineFormat`** (e.g. `- {title} {note} #Anime`), never a separate field — so it's explicit, never inferred from the type name.

> Logging is **independent of the episode checklist.** Checking an episode (`/episodes/watch`) only stamps that episode's `✅` completion date; it never writes a daily-note line, and logging never reads or ticks episodes. The two show up together only in the read-only activity feed, which aggregates both.

Resolution for a type: `type.log.<x>` → `dailyNotes.log.<x>` → built-in (`Log` for the section, `- {title} {note}` for the line). A blank or whitespace-only value is treated as unset.

`lineFormat` tokens (empty tokens collapse with surrounding whitespace, so a note-less log renders the bare `- [[Title]] #Tag`):

| Token | Meaning |
| --- | --- |
| `{title}` | the entity — **always rendered as a `[[wikilink]]`** (that link is what makes the line appear in the activity feed and as a backlink). Write `{title}`; `[[{title}]]` also works and isn't double-bracketed. |
| `{note}` | freeform text the user typed (put an episode number here if you want one) |
| `{date}` | the log's date (client-supplied — the user's local date) |

Example — anime tags its logs, games log under a different heading:

```yaml
dailyNotes:
  log:
    section: Log
    lineFormat: "- {title} {note}"

types:
- id: anime
  log:
    lineFormat: "- {title} {note} #Anime"   # note "12" → "- [[上伊那牡丹…]] 12 #Anime"
- id: games
  log:
    section: Played
    lineFormat: "- {title} {note} #Game"     # → "- [[PRAGMATA]] #Game"
```
