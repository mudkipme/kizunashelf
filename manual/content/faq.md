---
title: "FAQ & troubleshooting"
sidebar_position: 7
---

<!-- TODO: grow this from real support questions. Seed list: -->

## Why isn't my cover showing?

KizunaShelf never guesses from field names — a field called `cover` means nothing by itself. Check that the type's schema declares an `image` (or `imageList`) field for the frontmatter key you're using.

## I edited frontmatter in Obsidian and KizunaShelf shows something odd

KizunaShelf re-reads files on rescan and preserves values it doesn't recognize. If a value disappeared from the *UI*, it usually means the schema doesn't declare that field — the data is still in the file.

## Why won't an asset download from my NAS / LAN host?

Asset downloads are validated against an SSRF guard: private, loopback, and link-local addresses are blocked by default. Set `KIZUNASHELF_ALLOW_PRIVATE_ASSET_HOSTS` to relax this for trusted LAN hosts (self-hosted web).

## I got a "conflict" error when saving

Mutations are revision-guarded: if the file changed on disk (another device, another editor) after you loaded it, the save is rejected instead of silently overwriting. Reload the entity and re-apply your edit.

## My config didn't reach my other device

If you use official Obsidian Sync: `.yaml` is a non-Markdown extension, so enable **Settings → Sync → Sync all other types** on each device. See [Syncing your vault](./guides/syncing.md).

<!-- TODO more candidates:
     - "Why are my Chinese titles stored as `zh`, not `zh-Hans`?" (the bare-zh invariant, user-visible angle)
     - Filenames look different after syncing from a Mac/iPhone (NFC/NFD)
     - Writes are disabled on my Docker deployment (non-loopback default)
     - Provider search returns nothing (credentials not configured) -->
