---
title: "Quickstart"
description: "Create your first vault on desktop, iOS, or self-hosted web."
sidebar_position: 1
---

A **vault** is the folder where KizunaShelf keeps your library. You can create an empty one or use a folder you already keep in Obsidian.

## iOS

1. Install KizunaShelf from [TestFlight](https://testflight.apple.com/join/hE7k3sWd).
2. Create a vault under **On My iPhone** or **On My iPad**, or open an existing folder from Files.
3. For a new vault, choose your preferred language and the types you want to collect. Start with **Anime**, **Games**, or **Music Albums**, for example.
4. Finish setup. Your library opens with suggested smart lists on Home.

You can add types and change their fields later in **More → Vault Schema**. An existing KizunaShelf vault opens directly, without repeating setup.

## Desktop

Desktop releases are coming soon. The desktop app lets you open or create a folder, then use the same language and type picker. Schema settings are under **Settings**.

## Self-hosted web

Run the published Docker image with setup and editing enabled:

```bash
mkdir -p vault
docker run -p 127.0.0.1:8787:8787 \
  -v "$PWD/vault:/vault" \
  -e KIZUNASHELF_CONTENT_WRITABLE=true \
  -e KIZUNASHELF_SETTINGS_WRITABLE=true \
  ghcr.io/mudkipme/kizunashelf:latest
```

Open `http://localhost:8787/` and complete setup. Your library is stored in the local `vault` folder; to use an existing vault, replace `$PWD/vault` with its absolute path. See [Self-hosting](./self-hosting.md) for read-only mode, configuration, and remote access.

## Add your first entity

An **entity** is one thing in your library, such as *Steins;Gate 0 (Anime)*.

1. Open **Quick Capture** and choose **Anime**.
2. Search for *Steins;Gate 0*, then select the matching result.
3. Add it to your library. Open it to record your status, rating, or notes.

See [Adding an entity](../features/adding.mdx) for manual entry and search requirements. To bring in an existing collection instead, use [Batch import](../features/import.mdx).

Your Home lists update as you add and edit entities. You can [pin your own smart lists or recreate suggestions](../features/home.mdx) at any time.
