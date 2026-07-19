+++
title = "Schema-Driven by design"
description = "Field names are yours; meaning comes from the schema. The one concept everything else builds on."
weight = 1
+++

To help you get started quickly, KizunaShelf includes built-in presets for common media types like movies, anime, TV shows, video games, podcasts, and music albums.

Under the hood, however, these are not rigid, hardcoded rules, and they are simply ready-to-use schemas. KizunaShelf is built on the belief that **any personal collection or interest, no matter how niche, can be cleanly modeled and organized through a schema you control.**

## Markdown first

KizunaShelf treats Markdown files as the durable source of truth. The configuration file does not create a database schema; it describes how to read and edit Markdown files that already exist in an Obsidian-style vault.

Presets give you a zero-friction start, but you can customize every field of a preset or create brand-new entity types from scratch, whether that’s vintage tea, retro synth hardware, or local restaurants from a niche food manga.

The model consists of four layers:

1. `vaultRoot` points to the vault directory on disk.
2. `taxonomyRoot` points to the collection root inside the vault.
3. `types` maps subfolders under `taxonomyRoot` to entity types.
4. `fields` maps frontmatter keys to semantic roles such as title, cover, date, external reference, or relation.

For example, with the following configuration:

```yaml
taxonomyRoot: Taxonomy
types:
- id: anime
  path: Anime
```

KizunaShelf reads Markdown files from `<vault>/Taxonomy/Anime/*.md`, and each file becomes one entity.

## Names are yours, roles carry meaning

Your cover field can be named `cover_url`, `poster`, or `画像`. KizunaShelf never guesses meaning from a field's literal name. Instead, every field in the schema declares a **field type** (`title`, `image`, `date`, `enum`, `progress`, `rating`, `relation`, `externalRef`, etc.), and the app derives all features—titles, covers, calendars, progress bars, and relation graphs—from those declared roles.

This design has two practical consequences:

- **Renaming a field is safe** as long as the schema entry is updated alongside it; nothing in the app relies on the literal field name.
- **Hand-edited values are preserved.** Frontmatter the app doesn't recognize is kept intact rather than silently dropped. Your vault stays yours, even when edited outside KizunaShelf.

## Where the schema lives

The schema is stored in a single file inside the vault:

```text
<vaultRoot>/KizunaShelf/config.yaml
```

Because it lives *inside* the vault, it syncs with the vault automatically: every device pointing at the folder shares the exact same schema. It uses a **visible** folder on purpose, as most Obsidian sync methods skip hidden dot-folders. See [Syncing your vault](@/guides/syncing.md).

## Where to go from here

- [Anatomy of an entity](@/concepts/anatomy-of-an-entity.md): An annotated example file line by line.
- [Defining your own type](@/guides/defining-a-type.md): A step-by-step walkthrough.
- [Schema reference](@/reference/config.md): A complete reference of every available option.
