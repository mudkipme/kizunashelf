---
title: "Using KizunaShelf with Obsidian"
description: "One vault, two apps: wikilinks, daily notes, and what KizunaShelf will and won't touch."
sidebar_position: 2
---

<!-- TODO: full guide. Outline:
     1. The shared-vault model — KizunaShelf opens an Obsidian vault as-is; entities
        are ordinary notes; nothing is locked or converted.
     2. Wikilinks are relations — [[links]] in frontmatter and body are read in both
        directions; renames from KizunaShelf repoint inbound wikilinks in type
        folders, daily notes, and list pages.
     3. Daily notes — sharing the daily-notes folder and date format with Obsidian's
        core plugin; mentions connect a day to an entity; the Log action appends a line
        in your configured format.
     4. What KizunaShelf touches — entity files (atomic temp-file + rename writes),
        KizunaShelf/config.yaml, KizunaShelf/Lists/, the assetRoot, .trash on delete.
        Everything else in the vault is ignored.
     5. Why the config folder is visible (KizunaShelf/, not a dot-folder) and the
        Obsidian Sync "Sync all other types" setting.
     6. Coexisting with plugins — Dataview/Bases-style queries over the same
        frontmatter; smart lists as .base files (planned).
     7. Editing frontmatter in Obsidian — KizunaShelf re-reads on rescan; unknown
        fields and values are preserved. -->

*This guide is being written. The short version: point KizunaShelf at your existing Obsidian vault — entities are ordinary notes, relations are ordinary `[[wikilinks]]`, and KizunaShelf never touches files outside the folders your schema declares.*
