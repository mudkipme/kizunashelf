---
title: "Introduction"
description: "What KizunaShelf is, and where to start."
sidebar_position: 0
---

> A shelf for everything you love — and it stays yours.

KizunaShelf is a personal library for everything you watch, play, read, listen to, and care about. It feels familiar as a media tracker, but its shape is yours: you define the kinds of things in your library, the fields they carry, and how they relate.

Underneath, every entry is still a Markdown file in a folder you own. KizunaShelf helps you explore those files as a living shelf without turning them into data only one app can understand. Markdown is the source of truth, readable by Obsidian, SilverBullet, Zed, VS Code, any text editor, or no app at all. KizunaShelf can leave; your library does not.

## Start here

- **New to KizunaShelf?** Follow the [Quickstart](./start/quickstart.md) to create your first vault, then read [Schema-driven by design](./concepts/schema-driven.md) — the one concept everything else builds on.
- **What can it do?** [Features](./features/index.md) explains every feature, one page each — from Quick Capture to the iOS widgets.
- **"How should I log …?"** The [Cookbook](./cookbook/index.md) has a recipe per medium: anime, movies, books, games, music.
- **Coming from Obsidian?** See [Using KizunaShelf with Obsidian](./guides/obsidian.md) and [Syncing your vault](./guides/syncing.md).
- **Looking something up?** The [Reference](./reference/index.md) documents every schema option, field role, and environment variable.

## One library, three apps

One Rust core interprets the schema and Markdown everywhere. Web and desktop share the same interface; iOS presents the same library through a native app.

| App | How it fits |
| --- | --- |
| **Self-hosted web** | Serves one vault per instance through a browser. |
| **Desktop** | Opens, creates, and switches between local vaults. |
| **iOS** | Full native mobile app with widgets, notifications, Spotlight and Shortcuts integration. |

The vault schema lives at `KizunaShelf/config.yaml`, so it travels with the Markdown files and stays consistent across devices.
