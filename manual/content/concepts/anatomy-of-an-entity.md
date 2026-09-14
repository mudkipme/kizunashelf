---
title: "Inside an entity file"
description: "A short Markdown example and what each part looks like in the app."
sidebar_position: 2
---

Each entity is a Markdown file. Here is a shortened example for `Taxonomy/Anime/Steins;Gate 0 (Anime).md`, using the [example schema](../reference/config.md#complete-example):

```markdown
---
title_original: シュタインズ・ゲート ゼロ
title_zh: 命运石之门0
status: Completed
season: Spring 2018
complete_date: 2023-07-08
franchise: "[[Steins;Gate]]"
---

## Episodes

- [x] 1 ✅ 2023-07-01
- [x] 2 ✅ 2023-07-02

## Notes

Rewatching this after the game changes how I see the story.
```

The viewing dates and personal notes in this manual are illustrative.

## The filename

The filename gives this example its English title and its link target: `[[Steins;Gate 0 (Anime)]]`. Adding `(Anime)` distinguishes it from a game with the same title. Use different filenames for different entities so links stay unambiguous.

## The frontmatter

The block between `---` lines holds the entity's details:

| Detail | What you see in KizunaShelf |
| --- | --- |
| `title_original` and `title_zh` | Alternative titles; your language preference chooses which one is displayed. |
| `status` | A status badge and a way to filter your library. |
| `season` | The airing season, available in filters and upcoming views. |
| `complete_date` | The day you finished, shown in your activity history. |
| `franchise` | A link to the *Steins;Gate* franchise entity. |

These names work because the example schema defines them. Your vault can use different names for the same details; see [Types, fields & your schema](./schema-driven.md).

## The body

Below the frontmatter, write ordinary Markdown. The example's **Episodes** section becomes a [checklist](../features/episodes.mdx). **Notes** is your own writing. Adding a provider summary later does not replace those personal notes.

A [cover](../features/covers.mdx) or [external match](../features/editing.mdx#matching-external-metadata) adds more frontmatter fields. You can edit the file in KizunaShelf or a text editor.

## Daily notes

A mention such as `- [[Steins;Gate 0 (Anime)]] — finished my rewatch` in a dated daily note connects that day to the entity. The [Log activity](../features/log.mdx) action writes such a line for you.
