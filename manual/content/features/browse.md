+++
title = "Browsing the library"
description = "Covers, filters, facets, search with relevance ranking — the shelf view over your Markdown."
weight = 3
+++

The Library page is the shelf itself: every entity of a type, with covers, titles in your language, and status badges — all derived live from the files.

{{ screenshot(caption="The library grid with filters and search.") }}

## Filtering

Filters are built from your schema, not a fixed list:

- **Status** — the options of the type's `enumRole: status` field become a facet.
- **Tags** — the built-in, vault-wide tags field is filterable everywhere.
- **Relations** — filter by linked entities (e.g. every anime in one franchise), with autocomplete suggestions.
- Enum fields and other declared facets follow the same pattern: declare a field, get a filter.

## Search

The header search covers the whole library with an autosuggest dropdown, and matches across **all** of an entity's titles — original, English, Chinese, whatever the `titles` map holds — not just the displayed one. Results rank by relevance (title matches over body mentions); list views can also sort by `title`, any date/season field (`date:<field>`), relation count, or path.

## Titles follow your language

What each card shows as *the* title is the viewer's language preference resolved against the entity's titles map (`titles[language]`, falling back to the original title). Two people browsing the same vault in different languages see the same shelf with different titles — nothing in the files changes. See [Title Design](@/reference/titles-dates-status.md#title-design).

Detail pages then show the network around an entity ([Relations](@/features/relations.md)), its dated history ([Calendar & Activity](@/features/calendar.md)), and the editable frontmatter ([Editing](@/features/editing.md)).
