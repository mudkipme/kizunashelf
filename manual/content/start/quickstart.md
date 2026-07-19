+++
title = "Quickstart"
description = "Create your first vault on desktop, iOS, or self-hosted web."
weight = 1
+++

A **vault** is simply a folder of Markdown files accompanied by a schema file at `KizunaShelf/config.yaml`. This guide walks you through setting up a working library on your platform of choice.

## Desktop

1. Download and launch KizunaShelf Desktop (coming soon).
2. Onboarding opens a native vault chooser: select an existing folder (an Obsidian vault works as-is) or create a new, empty vault.
3. Select your preferred language, then choose from the built-in type presets (movies, TV, anime, manga, games, books, music, etc.). Each preset comes fully configured with titles, covers, dates, statuses, and external-provider mappings.
4. Click **Create Vault**. KizunaShelf writes `KizunaShelf/config.yaml` to your vault and opens your library.

Everything configured by a preset is standard schema data that you can edit at any time in **Settings**.

## iOS

1. Install KizunaShelf from [TestFlight](https://testflight.apple.com/join/hE7k3sWd).
2. Create a vault under **On My iPhone** (or **On My iPad**), or select an existing folder using the **Files** app.
3. If the selected folder does not contain a schema yet, the type-preset picker will launch automatically.

## Self-hosted web

The web app serves **one vault per instance** and is configured entirely through environment variables:

```bash
docker run -p 8787:8787 -v /path/to/vault:/vault ghcr.io/mudkipme/kizunashelf:latest # coming soon
```

The published image binds beyond loopback, so writes default to off — add `-e KIZUNASHELF_CONTENT_WRITABLE=true -e KIZUNASHELF_SETTINGS_WRITABLE=true` to let onboarding write the schema and the app edit your library.

Open the app in your browser. If the vault does not contain `KizunaShelf/config.yaml` yet, onboarding will launch the preset picker. See [Self-hosting](@/start/self-hosting.md) for the complete deployment guide, including write modes and setting up authentication.

## Next steps

- Read [Schema-driven by design](@/concepts/schema-driven.md): explaining how the system works under the hood.
- Add your first entries with **Quick Capture**: search an external provider and create an entity in a single action.
- Browse the [Cookbook](@/cookbook/_index.md) for recipes tailored to the media types you track most.
