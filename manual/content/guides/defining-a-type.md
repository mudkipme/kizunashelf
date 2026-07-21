+++
title = "Defining your own type"
description = "From a folder of Markdown to a fully-wired entity type: fields, roles, titles, relations, external refs."
weight = 1
+++

<!-- TODO: the full walkthrough. Outline:
     1. When to define a type by hand vs starting from a preset.
     2. The type entry — id, label, icon, path, filename config, externalPriority.
     3. Choosing fields and roles — walk through each fieldType (id, title, image,
        imageList, enum, enumList, progress, totalProgress, rating, bool, season,
        date, externalRef, relation, text, textList) with a one-line "use this when".
     4. Titles and languages — the titles map, title language, bare `zh` keys.
     5. Date roles and what lands on the calendar.
     6. Relations — pointing at another type, wikilinks in frontmatter and body.
     7. External refs — wiring a provider so matching and Quick Capture work.
     8. Editing safely — the Form vs raw YAML editor, validation, what happens to
        existing files when the schema changes (nothing is rewritten until you edit). -->

*This guide is being written. The [Schema reference](@/reference/config.md) already documents every option, and the Settings editor validates as you go — a safe way to experiment.*
