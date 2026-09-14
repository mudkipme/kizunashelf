---
title: "Types, fields & your schema"
description: "Choose the details each collection keeps, without changing how you write your notes."
sidebar_position: 1
---

A **type** is a collection such as Anime or Games. Its **fields** are the details you keep about each entity: title, cover, status, dates, or anything else you find useful. The **schema** is the set of instructions that tells KizunaShelf how to display and edit those details.

The built-in presets give you a starting point. You can use them as they are, change them, or create a type for a different collection — Pokémon cards or BanG Dream! concert goods, for example.

## Names are yours, roles carry meaning

Suppose your Markdown files call their cover field `poster`. In the schema, set that field's type to **Image**, and KizunaShelf uses it as a cover. You do not have to rename it to `cover`.

Some fields also have a **role**. A date can mean a release date or a completion date; a status can mean planned, in progress, or completed. Those choices determine where an entity appears in the calendar, activity feed, and suggested lists.

Changing a field's name in Settings does not rename that property in your existing notes. If you change it, update the notes too. Values the app does not recognize remain in the files.

## Where the schema lives

The schema is saved in `KizunaShelf/config.yaml` inside your vault. It travels with the rest of your library when you [sync the folder](../guides/syncing.md).

You usually edit it through [Settings](../features/schema.mdx). For a concrete example, see [Inside an entity file](./anatomy-of-an-entity.md); to make a new collection, follow [Defining your own type](../guides/defining-a-type.md).
