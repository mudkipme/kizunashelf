---
title: "Anatomy of an Entity"
description: "An annotated Markdown file showing how schema roles drive each part of the UI."
sidebar_position: 2
---

An entity is represented by a single Markdown file. This page breaks down a sample file line by line to show how each schema role assigns meaning to the data and how that meaning drives the UI.

The file below lives in a vault using the schema from the [Complete Example](../reference/config.md#complete-example) in the reference: an `anime` type stored under `Taxonomy/Anime/`, with English as the vault's primary title language.

## The file

`Taxonomy/Anime/Steins;Gate 0 (Anime).md`:

```markdown
---
title_original: シュタインズ・ゲート ゼロ
title_zh: 命运石之门0
cover_url: Assets/Anime/Steins;Gate 0 (Anime)/cover_url.jpg
status: Watched
season: Spring 2018
complete_date: 2023-07-08
bgm_url: https://bgm.tv/subject/129807
mal_url: https://myanimelist.net/anime/30484
tmdb_url: https://www.themoviedb.org/tv/78102
thetvdb_url: https://thetvdb.com/dereferrer/series/339268
franchise:
- "[[Steins;Gate]]"
tags:
- sci-fi
- time-traveling
---

## Summary

Eccentric Rintaro falls into a depression after failing to save Kurisu, but then a neuroscientist offers him the chance to interact with an AI copy.

## Episodes

### Specials

- [ ] 1 · Open the Missing Link - Divide By Zero 📅 2015-12-03
- [ ] 2 · Valentine's of Crystal Polymorphism: Bittersweet Intermedio 📅 2018-12-21

### Season 1

- [x] 1 · Missing Link of the Annihilator: Absolute Zero 📅 2018-04-12 ✅ 2023-06-30
- [x] 2 · Epigraph of the Closed Curve: Closed Epigraph 📅 2018-04-19 ✅ 2023-06-30
- [x] 3 · Protocol of the Two-sided Gospel: X-Day Protocol 📅 2018-04-26 ✅ 2023-06-30
- [x] 4 · Solitude of the Mournful Flow: A Stray Sheep 📅 2018-05-03 ✅ 2023-07-03
- ……

## Notes

The beta world line is cruel.
```

Nothing in this file uses KizunaShelf-specific syntax. It is standard Markdown with YAML frontmatter, wikilinks, headings, and a task list. Below is how the schema turns each piece into app behavior.

## The filename

`Steins;Gate 0 (Anime).md` — the file basename serves three purposes:

* It is the **wikilink target**: other notes link to this entity using `[[Steins;Gate 0 (Anime)]]`. When you rename the entity in KizunaShelf, inbound wikilinks in type folders, daily notes, and list pages are automatically updated. We recommend avoiding duplicate filenames within a vault; for example, `(Anime)` was added here to prevent ambiguity with the game.
* Because the type declares `filename: { titleLanguage: en }`, the basename is also registered as the **English title**.
* It forms part of the default entity key indexed by the app.

## The Frontmatter, line by line

### The title lines

```yaml
title_original: シュタインズ・ゲート ゼロ   # fieldType: title, titleRole: original
title_zh: 命运石之门0                    # fieldType: title, titleLanguage: zh
```

These fields build the entity's title map. **The field names themselves carry no functional meaning**: `title_original` could be named `original_name` or anything else; only the `titleLanguage` and `titleRole` attributes in the schema matter. What each viewer sees depends on their language preference:

* When the language preference is Chinese, the main title is **命运石之门0**.
* When the language preference is English, the main title is **Steins;Gate 0 (Anime)** (derived from the filename).
* When the language preference is neither English nor Chinese, it falls back to the `titleRole: original` field: **シュタインズ・ゲート ゼロ**.

The resolution order is always *language preference → original → other titles*, and every title remains fully searchable and is displayed in the entity detail page. See [Title Design](../reference/titles-dates-status.md#title-design).

### `cover_url: Assets/Anime/Steins;Gate 0 (Anime)/cover_url.jpg`

Schema: `fieldType: image`. This field supplies the cover image for the library grid, detail page, and Home smart lists. The app identifies this field because the schema classifies it as an image, not because of its key name. When external matching downloads a cover, it writes to this field, and the file is saved under the vault's configured `assetRoot`.

### `status: Watched`

Schema: `fieldType: enum` with `enumRole: status` and a `statusValues` mapping (e.g., `completed: [Watched]`). This enables three layers of opt-in behavior:

1. As a standard `enum`, it renders as a visual badge and acts as a library filter facet.
2. `enumRole: status` marks it as *the lifecycle status* field for this type.
3. `statusValues` assigns canonical meaning to values: `Watched` maps to `completed`, allowing this entry to appear in the recent activity list. Unmapped values, such as a manually entered `Finished`, are preserved and displayed, but carry no canonical meaning until explicitly mapped.

See [Status](../reference/titles-dates-status.md#status).

### `season: Spring 2018`

Schema: `fieldType: season` with `dateRole: planning` and `seasonLanguage: en`. Season strings are parsed as broadcast windows to drive activity lists and `sort: date:season` ordering.

### `complete_date: 2023-07-08`

Schema: `fieldType: date` with `dateRole: completed`. This date appears in calendars, activity histories, and smart lists, including those pinned to Home (e.g., "Finished in 2023").

### The external references

```yaml
bgm_url: https://bgm.tv/subject/129807
mal_url: https://myanimelist.net/anime/30484
tmdb_url: https://www.themoviedb.org/tv/78102
thetvdb_url: https://thetvdb.com/dereferrer/series/339268
```

Schema: `fieldType: externalRef` with corresponding provider mappings. These lines link the entity to external metadata. They enable "already in library" detection for Quick Capture and batch import, allow metadata refreshes via re-matching, and power the episode sync action that pulls episode lists from Bangumi, MyAnimeList, TMDB, or TheTVDB into the checklist below. They are stored as standard URLs, making them easily readable and clickable in any text editor.

### `franchise: ["[[Steins;Gate]]"]`

Schema: `fieldType: relation` with `relationType: franchise`. This uses standard wikilinks and is indexed in **both directions**: this entity's detail page shows *Steins;Gate* as an outgoing relation, while the *Steins;Gate* entity lists *Steins;Gate 0 (Anime)* as an incoming relation. The `relationType` restricts matching exclusively to the `franchise` type.

### `tags: ["sci-fi", "time-traveling"]`

Tags are vault-wide, shared across all entity types, and filterable everywhere — but only when the vault opts in by setting `tags.field` in the config (this vault sets it to `tags`). Without that, a `tags` key is just an ordinary frontmatter field. See [Tags](../reference/home-tags-daily-notes.md#tags).

## The body

### `## Summary`

If the type declares a `bodySections` entry with `kind: external` for the heading `Summary`, external matching populates this section from the mapped provider field (e.g., TMDB's `summary`). It replaces only this section, leaving the rest of the body untouched.

### `## Episodes`

A `bodySections` entry with `kind: episodes` and `tracking: checklist` turns this standard Markdown task list into an interactive episode tracker. Checked items mark watched episodes, schedule dates are indicated by 📅, and season subheadings group the list. Ticking an episode on the detail page updates this list and appends a completion date (✅). These dates feed into "Recent" and "Up Next" activity lists.

### `## Notes`

Because this heading is not declared in the schema, KizunaShelf leaves it completely untouched. Undeclared frontmatter behaves the same way: it is preserved and displayed as extra properties without being overwritten or removed. Your files are never trimmed to fit only what the schema defines.

The one thing the app *does* offer here is interaction: any `- [ ]` task list you write in the body is clickable on the detail page, and ticking one stamps a ✅ completion date on that line and nothing else. See [Ticking tasks in your notes](../features/editing.mdx#ticking-tasks-in-your-notes).

## Daily notes

Referencing an entity's wikilink in daily notes—such as adding `- [[Steins;Gate 0 (Anime)]] ep 20` in `Daily Notes/2023-07-06.md` (manually or via the Log action), connects that date to the entity and displays the entry in its activity history.
