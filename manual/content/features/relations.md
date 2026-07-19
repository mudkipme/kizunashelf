+++
title = "Relations"
description = "Ordinary wikilinks read in both directions — the network around each entity, with no separate graph store."
weight = 6
+++

A game belongs to a franchise; an anime adapts a novel; a character connects to her voice actor. In KizunaShelf those relations are ordinary `[[wikilinks]]` written in your files — there is no proprietary graph database to export from or lose.

## Where relations come from

- **Frontmatter relation fields** — a schema field with `fieldType: relation` holds one wikilink or a list of them; `relationType` restricts matching to a target type. These are the *typed* relations shown in the detail page's panels.
- **Body links** — any `[[wikilink]]` in an entity's body that points at a known entity is indexed as a `body` relation.
- **Daily-note mentions** — wikilinks in daily notes become `daily-note` relations, tying days to entities.

The index stores **both directions**: the detail page shows outgoing relations (what this entity points at) and incoming ones (what points at it) separately.

{{ screenshot(caption="The relations panel: typed relations, backlinks, and mentions around one entity.") }}

## Renames repoint links

Renaming an entity from KizunaShelf rewrites inbound `[[wikilinks]]` across type folders, daily notes, and list pages, so the network survives renames. (Matching normalizes Unicode NFC/NFD, so *Pokémon* and *ポケモン* filenames are safe across macOS/iOS and other platforms.)

Relations also power [library filtering](@/features/browse.md#filtering) ("everything linked to this franchise") and the relation columns in [smart lists](@/features/lists.md).
