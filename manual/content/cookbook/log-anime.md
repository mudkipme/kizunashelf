---
title: "Logging anime"
description: "Seasons, episode check-ins, airing calendar, and Japanese/English titles."
sidebar_position: 1
---

<!-- TODO: full recipe. Outline (write from the Anime preset in presets.rs so docs and preset agree):
     1. The type definition — YAML from the resolved Anime preset, annotated.
     2. A sample entity — frontmatter for one show; call out multi-language titles
        (titles map, bare `zh` keys), season field, status enum, progress.
     3. What the UI derives — airing dates on the calendar, episode check-ins as
        dated activity, progress against episode counts from the provider.
     4. External metadata — recommended providers (Bangumi, TMDB, TheTVDB, AniList…),
        credentials needed, how matching fills covers + episode lists.
     5. Best practices — one entry per season vs per franchise, relating adaptations
        to source novels/manga, using Quick Capture for airing-season additions. -->

*This recipe is being written. Start from the **Anime** type preset during onboarding (or Settings → add a built-in type) — it wires titles, cover, season, status, progress, and provider mappings for you.*
