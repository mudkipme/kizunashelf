+++
title = "Calendar & Activity"
description = "What's coming up, what you just did, and what you should catch up on — a catalog with a memory."
weight = 7
+++

Dates aren't just display fields in KizunaShelf: every field with a [date role](@/reference/field-types.md#date-roles-daterole) feeds the calendar, the planning views, and the activity feed. Together they answer three questions.

## What's coming up?

Fields with `dateRole: planning` (release dates, airing seasons, publish dates) and `dateRole: event` (concerts and exhibitions you attend) land on the **calendar**. Exact dates get day-level entries; seasons and years still count for planning views. Episode items' 📅 airing dates appear too, so an airing show shows its next episode — not just its premiere.

{{ screenshot(caption="The calendar: releases, airing episodes, and your own plans on one surface.") }}

## What have I done recently?

The **activity feed** is the read-only merge of everything dated that happened around your library:

- episode items you checked off (their ✅ dates),
- `completed`/`started` dates you logged,
- daily-note mentions and [log](@/features/log.md) lines,

each linked back to its entity and its day. Years from now it won't just say *that* you loved something — it says **when** it became part of your life.

{{ screenshot(caption="The activity feed: a dated record of watching, playing, and reading.") }}

## What should I catch up on?

The planning views slice the same data forward:

- **Up next** — `planning` entities whose date has arrived, and `ongoing` things with unwatched items. `paused` and `dropped` entities are deliberately kept out (no nagging); see [canonical statuses](@/reference/field-types.md#canonical-statuses-statusvalues).
- **Catch up** — aired-but-unwatched episode items across the library, so a season you fell behind on surfaces episode-by-episode.
- **Just started** — entities with a recent `started` date, for picking back up what you began.

On iOS, the same "coming up" data drives the Home Screen **widget** and release **notifications** — see [iOS features](@/features/ios.md).
