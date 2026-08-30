---
title: "Syncing Your Vault"
description: "KizunaShelf does not sync files directly. Because a vault is simply a folder, you can sync it using any tool you already use."
sidebar_position: 3
---

**KizunaShelf does not sync anything itself.** Following the *files over apps* principle, a vault is simply a folder containing Markdown files, frontmatter, links, notes, downloaded assets, and a schema located at `KizunaShelf/config.yaml`. To access a vault across multiple devices, sync that folder using your preferred sync tool. KizunaShelf simply reads whatever files are present on disk.

Because the schema lives *inside* the vault (`<vaultRoot>/KizunaShelf/config.yaml`), it travels with the vault automatically. Every device pointing at the synced folder shares the exact same schema. No KizunaShelf-specific configuration is needed on additional devices: simply open the synced folder as a vault, and onboarding is skipped automatically because the configuration file is already present.

## What Travels with the Vault

The synchronized folder contains your durable library:

- Entity and daily-note Markdown files
- `KizunaShelf/config.yaml`
- Lists and smart lists under `KizunaShelf/Lists/`
- Downloaded assets under the configured `assetRoot`

Device-specific state does **not** belong in the vault: active vault selection, language preferences, provider credentials, reminder settings, and disposable index caches remain local to each app installation. Downloaded assets often make up the largest portion of a vault, so keep storage and bandwidth limits in mind when choosing a sync provider.

## The Config Folder Is Visible on Purpose

The vault configuration lives in a **visible** folder (`KizunaShelf/`). Most sync tools, including official Obsidian Sync, ignore hidden files (dot-folders), meaning a hidden config folder would silently fail to reach your other devices. A visible folder syncs reliably with any method (Obsidian LiveSync, iCloud, Syncthing, etc.). This folder also stores other app-owned, syncable artifacts, such as saved lists under `KizunaShelf/Lists/`.

> **Official Obsidian Sync users:** Markdown files sync by default, but `.yaml` is a non-Markdown file type. Enable **Settings → Sync → Sync all other types** on each device so the config file travels correctly.

## Syncing Methods That Live Inside Obsidian

If your sync method runs *inside* Obsidian (such as official **Obsidian Sync** or community plugins like **Self-Hosted LiveSync**), syncing occurs only while **Obsidian is open**. KizunaShelf cannot trigger these plugins directly because it runs independently of Obsidian. Recommended workflow:

- **Open Obsidian on the source device** and allow it to finish uploading changes before accessing the vault elsewhere.
- **Open Obsidian on the target device** to pull down recent changes before opening KizunaShelf.

If you self-host KizunaShelf on a server, you can run [Obsidian Headless](https://obsidian.md/help/headless) or the [Self-Hosted LiveSync CLI](https://github.com/vrtmrz/obsidian-livesync/tree/main/src/apps/cli) on that same server to sync without the desktop app.

## Avoiding Sync Conflicts

KizunaShelf uses revision guards to prevent edits made in the app from silently overwriting an entity that changed after it was loaded. While this protects single-file writes on a single device, it cannot control how an external sync service handles simultaneous edits to the same file across multiple devices.

For the safest workflow, allow the source device to finish uploading before editing the same vault on another device. Then, allow the destination device to finish downloading before rescanning or reopening the vault. If a sync provider creates conflict files, resolve them as standard Markdown or YAML files. Be especially careful with simultaneous edits to `KizunaShelf/config.yaml`, as every client relies on that single shared schema.

## iOS

On iOS, KizunaShelf can create a managed vault under **On My iPhone** (or **On My iPad**) or open an external folder from the Files app. An external File Provider must expose the vault as a standard directory and support both read and write permissions if you plan to edit content in KizunaShelf.

iCloud Drive is recommended for Apple cross-device workflows. Place the vault in iCloud Drive and select that folder from Files. Note that iOS may offload or evict cloud files depending on device storage conditions, so ensure the vault remains downloaded locally if you need offline access.

Several other File Provider apps are supported, including **ShellFish** (SFTP), **S3 Files**, and **Working Copy** (Git). File synchronization, background transfers, offline availability, and conflict resolution are managed entirely by that provider and its connected server.

Alternatively, you can sync your vault within Obsidian on iOS and open that same local vault folder in KizunaShelf.

## Versioning

If you want version history, consider using Git. Because the vault consists of plain-text files, it diffs and versions cleanly. You can commit the vault repository and sync it like any other Git project (via `git push`/`pull` or an iOS client like Working Copy).
