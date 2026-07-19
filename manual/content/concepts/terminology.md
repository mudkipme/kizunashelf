+++
title = "Terminology"
description = "The standard names for things, used consistently across the manual and the app."
weight = 3
+++

These are the name of concepts building KizunaShelf.

| Term | Meaning |
| --- | --- |
| **entity** | One thing in your library — one Markdown file in a type's folder. |
| **entity type** (or just **type**) | A kind of entity your schema declares (`anime`, `games`, …), mapped to one folder. |
| **vault** | The folder that holds everything: entities, daily notes, assets, and the schema. Same word as Obsidian's. |
| **library** | The whole indexed collection KizunaShelf reads from the vault what you browse. |
| **schema** (or **vault config**) | `KizunaShelf/config.yaml` — the declaration of types, fields, and roles. |
| **field** | One frontmatter key an entity carries, declared in the schema with a `fieldType`. |
| **role** | The semantic job of a field beyond its type: `titleRole`, `dateRole`, `enumRole`. Meaning always comes from roles, never from field names. |
| **frontmatter** | The YAML block at the top of an entity's Markdown file. |
| **body** | Everything below the frontmatter: notes, body sections, episode lists. |
| **relation** | A `[[wikilink]]` connecting two entities, read in both directions. |
| **item** | One entry *inside* a checklist, list, or queue: an episode, a track, an issue, a list-page line, an import-queue row. |
| **daily note** | A dated Markdown note (Obsidian-style); mentions and log lines live here. |
| **provider** | An external metadata source (Bangumi, TMDB, …) from the [provider catalog](@/reference/providers.md). |
| **match** | Connecting an entity to a provider result and applying mapped metadata. |
| **Quick Capture** | The search-a-provider-and-create-in-one-action flow. |
| **Log activity** | The one-line write-to-today's-daily-note action, the *Log* button on an entity. |
| **activity** | The dated record of what happened: episode check-offs, completion dates, daily-note mentions, logs. |
| **smart list** | A saved `.base` file of criteria whose items are computed, partly compatible with Obsidian Bases. |
| **home section** | One configured row of the Home page. |
