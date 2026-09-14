---
title: "Using KizunaShelf with Obsidian"
description: "One vault, two apps: wikilinks, daily notes, and what KizunaShelf will and won't touch."
sidebar_position: 2
---

KizunaShelf and Obsidian can open the same vault. Your entities remain ordinary notes with properties, headings, checklists, and links.

## Open an existing vault

Choose your Obsidian vault folder in KizunaShelf. If it has no KizunaShelf schema yet, set one up to match your existing folders and properties. Built-in presets are a starting point; they do not automatically recognize every note in an existing vault.

Use the [schema editor](../features/schema.mdx) to choose which folders contain each type and what their fields mean. Notes outside those collections can stay where they are.

## Use the same notes in both apps

- Write `[[wikilinks]]` to [connect entities](../features/relations.mdx).
- Use the same folder and date format for [daily notes](../features/log.mdx#configuring-daily-notes).
- Edit [episode checklists](../features/episodes.mdx) in either app.
- Open compatible [smart lists](../features/lists.mdx#the-obsidian-bridge) as Obsidian Bases.

Refresh KizunaShelf after editing in Obsidian. Unrecognized fields and values remain in your files.

## Keep both apps in sync

KizunaShelf does not run Obsidian's sync service or plugins. If you use one of them, open Obsidian and let it finish syncing before switching devices. See [Syncing your vault](./syncing.md) for the required file types and conflict advice.
