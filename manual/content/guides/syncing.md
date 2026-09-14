---
title: "Syncing your vault"
description: "KizunaShelf does not sync files directly. Because a vault is simply a folder, you can sync it using any tool you already use."
sidebar_position: 3
---

KizunaShelf reads a folder; it does not provide its own sync service. Use your existing sync tool to keep that folder available on other devices, then open it in KizunaShelf.

## What travels with the vault

Include the whole library:

- Entity files and daily notes.
- `KizunaShelf/config.yaml`, which holds the schema.
- Lists under `KizunaShelf/Lists/`.
- Downloaded covers and other assets.

A device that receives the schema opens the library without repeating setup. Language preferences, provider credentials, and reminder settings stay on each device.

## Obsidian Sync {#the-config-folder-is-visible-on-purpose}

Enable **Sync all other types** on each device so `.yaml` settings and `.base` smart lists travel with your Markdown. Include images if you want local covers on every device.

## Syncing methods that live inside Obsidian

Obsidian Sync and sync plugins run through Obsidian. Let it upload changes on the first device, then download them on the second before opening KizunaShelf there.

For a self-hosted server, see your sync tool's headless or command-line instructions. KizunaShelf does not start or manage that process.

## Avoiding sync conflicts

Let syncing finish before editing the same file on another device. If a file changes after you opened it, KizunaShelf may ask you to reload before saving. Your sync service can still create conflicts if two devices edit offline.

Resolve any conflict copies in a text editor. Take particular care with `KizunaShelf/config.yaml`, since it describes the whole library. Keep backups as well as synced copies.

## iOS

Create a local vault under **On My iPhone** or **On My iPad**, or open a folder from Files, such as one in iCloud Drive. The folder must allow writing if you want to edit it in KizunaShelf.

Cloud files may be removed from local storage to save space. Download the vault before going offline, and check your file provider's settings for keeping files on the device.

You can also open the local vault that Obsidian uses on iOS, after letting Obsidian finish syncing it.

## Versioning

Git can keep a history of your Markdown and settings files. If you already use it for your vault, continue committing and syncing that repository as usual. Covers are binary files and can make the repository much larger.
