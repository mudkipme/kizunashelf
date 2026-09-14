---
title: "Home, tags & daily notes"
description: "Home smart-list metadata, the built-in tags field, and daily notes with logging."
sidebar_position: 6
---

## Home Page

Home displays pinned [smart lists](../features/lists.mdx). It has no entry in `config.yaml`. To pin a list, use **Add to Home** in its detail view, or set this metadata in its `.base` file:

```yaml
kizunashelf:
  showOnHome: true
```

`showOnHome` defaults to false. Other metadata in the file is preserved when toggling it. Suggested lists also carry a `kizunashelf.suggestion` identifier so the app can reuse them after renames or edits; it is managed by the app.

Rows follow filename order. Each uses the first supported view's filters, sort, and limit, with a Home preview of up to 12 items. **See all** opens that list and view. Removing a list from Home keeps its file and criteria.

Onboarding creates suggestions from the schema's status and date roles. **Add suggested lists** on Home makes the same suggestions available afterward without resetting existing lists. See [Home & smart lists](../features/home.mdx).

Obsolete `home` config blocks are ignored when loading older test vaults. No sections are migrated. The structured settings editor omits that block when saving; the strict raw YAML editor requires you to remove it.

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
