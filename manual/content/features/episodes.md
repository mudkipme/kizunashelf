+++
title = "Managing episodes & tracks"
description = "A plain Markdown checklist tracks exactly which episodes, tracks, or issues you've finished — and can sync from a provider."
weight = 5
+++

Episodes, album tracks, and comic issues are **items** inside one entity, living in its body as an ordinary Markdown list under a declared heading:

```markdown
## Episodes
### Season 1
- [x] 1 · Pilot
- [ ] 12.5 · Recap (special)
### Season 2
- [ ] 1 · New Dawn
```

The type declares this with a `bodySections` entry of `kind: episodes`. With `tracking: checklist`, items are task-list checkboxes that record *exactly which* are done — handling skips, specials, and `12.5`-style numbering that a plain progress counter can't. The item number lives in the item text, seasons or discs are sub-headings, and the engine derives the watched/total roll-up shown in the library and on the detail page.

{{ screenshot(caption="The episodes panel: tick items on the detail page, or edit the same list in Obsidian.") }}

Because it's plain Markdown, the list is editable everywhere: tick an item on the detail page or check the box in Obsidian — both are the same edit. Checking an item stamps a ✅ completion date and airing dates carry a 📅 prefix, following the [Obsidian Tasks emoji format](https://publish.obsidian.md/tasks/Reference/Task+Formats/Tasks+Emoji+Format), so the Tasks plugin reads your episode list natively. Each check-off becomes dated [activity](@/features/calendar.md).

## Syncing from a provider

When the entity is [matched](@/features/editing.md#matching-external-metadata) to a provider that can supply a list, the episodes panel offers **Sync**: it pulls the provider's list into a checkable preview and merges the ticked items in — new items are added, existing ones get title updates, and **your watched ticks and hand-added items are always kept**.

Which providers can supply a list is the *Episode/track sync* column of the [provider reference](@/reference/providers.md): series providers supply episodes, music providers supply tracks (grouped by disc), and Comic Vine supplies a volume's issues.

When more than one linked provider can supply a list, the dialog lets you choose which to sync from.

## One entity or many?

Multiple seasons can live as sub-headings in **one** entity, or as **separate** entities linked by [relations](@/features/relations.md) — the engine mirrors whatever the files contain and never merges or splits them. Pair the checklist with a `progress`/`totalProgress` field pair only if you also want a plain numeric counter; the [Anime cookbook recipe](@/cookbook/log-anime.md) discusses the trade-offs.
