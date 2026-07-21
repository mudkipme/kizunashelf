+++
title = "Batch import"
description = "Bring an existing library from another service — with a full review before anything is written."
weight = 10
+++

Batch import brings the years you've already logged elsewhere onto your shelf. It is a three-step, **review-before-write** job: nothing touches your vault until you approve the plan.

1. **Fetch & plan** — pull the source (a public profile's username or an exported CSV) and resolve every item against your schema.
2. **Review** — see what will be *created*, what is *already on the shelf* (skipped), and what *needs a second look*; where a source bucket maps to more than one of your types, pick the target.
3. **Commit** — create the approved entities. Re-running the same import is safe: anything already created is skipped.

{{ screenshot(caption="The import review: every decision visible before a single file is written.") }}

## Sources

| Source | Input | Resolves via | Credential needed |
| --- | --- | --- | --- |
| Bangumi | username | `bangumi` | — |
| MyAnimeList | username | `myanimelist` | MyAnimeList client id |
| AniList | username | `myanimelist` (MAL ids) | — |
| Kitsu | username | `myanimelist` (MAL mapping) | — |
| Trakt | username (slug) | `tmdb` | Trakt client id |
| Steam | SteamID64 | `steam` | Steam Web API key |
| IMDb | CSV export | `tmdb` | TMDB API key |
| Goodreads | CSV export | `openlibrary` (ISBN) | — |
| Yamtrack | CSV export | `myanimelist`, `tmdb` | — |

Each source resolves its items to one of the built-in [providers](@/reference/providers.md), so **a type must declare an `externalRef` field for that provider** to receive them — e.g. an `externalRef: tmdb` field to import from Trakt or IMDb. A source missing a required credential is shown but disabled, with the reason.

## What comes across

Your data maps through **schema roles, not field names**: the source's status translates to a [canonical status](@/reference/field-types.md#canonical-statuses-statusvalues) and lands in your status field via its `statusValues`; scores go to the first `rating` field (normalized to 0–10); started/finished dates go to the `started`/`completed` date-role fields; notes become a `## Notes` body section; and watched progress ticks the first *N* episode items. A role you haven't wired is simply skipped, and per-import toggles control which of these run.

Import does **not** download covers — imported image fields keep their remote URLs. Fetch local copies afterwards with the [batch cover downloader](@/features/covers.md).

Where it lives: the import wizard on web/desktop, and **Settings → Data** on iOS.
