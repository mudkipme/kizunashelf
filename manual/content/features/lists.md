+++
title = "Lists & smart lists"
description = "Hand-curated list pages and criteria-driven smart lists — stored in the vault, readable by Obsidian."
weight = 8
+++

Two kinds of lists, both living inside the vault under `KizunaShelf/Lists/` so they sync with everything else.

## Lists

A **list** is a hand-curated page of entities — a ranking, a "favorites of 2026", a lending record. Items are ordinary `[[wikilinks]]`, ordered by you (drag to reorder in the app, or edit the file), with room for a description and notes. Because items are wikilinks, list membership shows up on each entity as an incoming [relation](@/features/relations.md), and renames repoint list items automatically.

{{ screenshot(caption="A curated list: drag to reorder, or edit the Markdown.") }}

## Smart lists

A **smart list** is computed: you define criteria, and its items are whatever currently matches. Rules cover comparisons (`eq`/`ne`/`gt`/`gte`/`lt`/`lte` — dates, numbers, and relative windows like "in the last 30 days"), text (`contains`, `startsWith`, `endsWith`), presence (`isEmpty`, negatable), tags (`hasTag`), links (`linksTo`), and folders (`inFolder`), combined with `all`/`any`/`none` conjunctions and one level of subgroups.

Like everything else, a rule's meaning is value-driven: a comparison is a date comparison because the right-hand side is a date — never because of the field's name.

{{ screenshot(caption="The smart-list rule builder.") }}

## The Obsidian bridge

Smart lists are stored as **`.base` files using a subset of the Obsidian Bases format** — open the same file in Obsidian and it shows the exact same items. That's the files-over-apps promise applied to queries: your saved views aren't locked in either.

The same criteria model powers [Home page sections](@/features/home.md), so a home section you like can become a smart list and vice versa.
