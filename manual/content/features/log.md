+++
title = "Log activity & daily notes"
description = "One tap writes a line into today's daily note, in a format you define — and can flip the entity's status."
weight = 4
+++

**Log activity** is the smallest unit of remembering: the **Log** button writes one line into **today's daily note**, connecting the day to the entity. Months later, that line is what the [activity feed](@/features/calendar.md) and the entity's history are made of.

## What a log writes

The line's shape is schema-driven. A type opts into logging by declaring a `log` block, and the line template comes from `lineFormat`:

```yaml
types:
- id: anime
  log:
    lineFormat: "- {title} {note} #Anime"   # → "- [[星辰远航]] 12 #Anime"
```

- `{title}` always renders as a `[[wikilink]]` — that link is what makes the line show up as activity and as a backlink on the entity.
- `{note}` is whatever you type in the log dialog (an episode number, a thought, or nothing — empty tokens collapse cleanly).
- The type's hashtag is a literal in the template, never inferred from the type name.

{{ screenshot(caption="Log activity: one line into today's daily note.") }}

## Status flips

If the type's status field maps [canonical statuses](@/reference/field-types.md#canonical-statuses-statusvalues), a log can flip the status — logging "completed" writes the first option mapped to `completed` (e.g. `看完`). Flips are **monotonic** along planning → ongoing → completed: a log never demotes, a `paused` entity resumes, and a `dropped` entity is never auto-resumed.

Logging is deliberately **independent of the episode checklist**: checking an episode stamps that item's ✅ date but never writes a daily-note line, and logging never ticks episodes. The two meet only in the read-only activity feed.

## Configuring daily notes

Daily notes are ordinary Obsidian-style dated notes; KizunaShelf needs to know where they live:

```yaml
dailyNotes:
  paths:
  - Daily Notes
  dateFormat: YYYY-MM-DD          # Moment-style, same syntax as Obsidian's Daily Notes
  template: Templates/Daily Note.md   # optional; seeds a note that doesn't exist yet
  log:
    section: Log                  # the heading log lines are appended under
    lineFormat: "- {title} {note}"  # global default; per-type log blocks override
```

`dateFormat` describes the file path relative to the daily-notes folder and supports subfolders (`YYYY/MM/YYYY-MM-DD`). Beyond logging, KizunaShelf scans these notes for entity `[[wikilinks]]` — every mention ties that day to the entity, whether KizunaShelf wrote the line or you did. Full options: [Daily Notes](@/reference/home-tags-daily-notes.md#daily-notes).

On iOS, a log is one tap away via the entity page, Siri/Shortcuts, or the Action button — see [iOS features](@/features/ios.md).
