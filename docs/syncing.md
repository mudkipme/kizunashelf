# Syncing KizunaShelf

**KizunaShelf does not sync anything itself.** Following the *files over apps* principle, a vault is just a folder of Markdown, frontmatter, links, notes, and downloaded assets — plus the schema at `KizunaShelf/config.yaml`. To use a vault on more than one device, sync that folder with **whatever syncing method you already use**. KizunaShelf simply reads whatever files are present on disk; getting the same files onto each device is the sync tool's job, not KizunaShelf's.

Because the schema lives *inside* the vault (`<vaultRoot>/KizunaShelf/config.yaml`), it travels with the vault automatically — every device pointing at the synced folder shares the same schema. There is nothing KizunaShelf-specific to configure on the other end: open the synced folder as a vault and onboarding is skipped because the config is already there.

## What travels with the vault

The synchronized folder contains the durable library:

- entity and daily-note Markdown files
- `KizunaShelf/config.yaml`
- saved lists under `KizunaShelf/Lists/`
- downloaded assets under the configured `assetRoot`
- `.trash` entries created by KizunaShelf deletions, when the sync tool includes hidden directories

Device-specific state does **not** belong in the vault: the active-vault selection, language preference, provider credentials and derived tokens, reminder preferences, and disposable index caches remain local to each app installation. Downloaded assets can be the largest part of a vault, so account for their storage and transfer cost when choosing a sync provider.

## The config folder is visible on purpose

The vault config lives in a **visible** folder (`KizunaShelf/`, not a hidden dot-folder). Most Obsidian sync methods skip hidden files — including official Obsidian Sync, which has no hidden-file support at all — so a dot-folder config would silently fail to reach your other devices. A visible folder syncs with every method (Obsidian LiveSync, iCloud, Syncthing, …). The folder also holds other app-owned, sync-worthy artifacts — for example saved lists under `KizunaShelf/Lists/` — so they travel with the vault too.

> **Official Obsidian Sync users:** Markdown always syncs, but `.yaml` is a non-Markdown extension, so enable **Settings → Sync → Sync all other types** (per device) for the config to travel. You may also want **Settings → Files & links → Detect all file extensions** so Obsidian shows the file at all. Other sync tools (LiveSync, iCloud, Syncthing) need no extra setting once the file is out of the dot-folder.

## Syncing methods that live inside Obsidian

If your sync method runs *inside* Obsidian — official **Obsidian Sync** or community plugins like **Self-hosted LiveSync** — then syncing only happens while **Obsidian is open**. KizunaShelf can't trigger those plugins; it doesn't run Obsidian. So the workflow is:

- **Open Obsidian on the source device** and let it finish pushing changes before you expect them elsewhere.
- **Open Obsidian on the target device** to pull changes down before opening KizunaShelf there.

If you'd rather not depend on Obsidian being open, use a sync method that runs at the filesystem/OS level instead (iCloud Drive, Syncthing, Dropbox, git, …) — those sync the folder regardless of which apps are running.

## Avoiding sync conflicts

KizunaShelf uses revision guards so an edit made through the app will not silently overwrite an entity that changed after it was loaded. That protects one write on one device; it cannot control how a separate sync service resolves two devices editing the same file at the same time.

For the safest workflow, let the source device finish uploading before editing the same vault on another device, then let the destination finish downloading and rescan or reopen the vault. If a provider creates conflict copies, resolve them as ordinary Markdown/YAML files before continuing. Be especially careful with simultaneous edits to `KizunaShelf/config.yaml`, because every client derives the library's meaning from that one shared schema.

## iOS

On iOS, KizunaShelf can create a managed vault under On My iPhone or open an external folder from Files through a security-scoped bookmark. An external File Provider must expose the vault as a normal directory; it must support writes as well as reads if you want to edit content from KizunaShelf.

- **iCloud Drive is recommended for a cross-device Apple workflow.** Put the vault in iCloud Drive and open that folder from Files. iOS may download or evict cloud files according to system and storage conditions, so make sure the vault is locally available before relying on offline access.
- **Other File Provider apps are supported when they expose a real directory** — for example **ShellFish** (SFTP), **S3 Files**, and **Working Copy** (git). Coordination, background transfer, offline availability, and conflict behavior belong to that provider, not KizunaShelf.
- **On My iPhone is fully local.** A managed vault works offline and appears under On My iPhone → KizunaShelf, but it does not reach another device unless you copy or move it into a synchronized location.

## Versioning

None of the above keeps history — they sync the current state. If you want **version history** (the ability to roll back, or to review changes over time), use **git**. The vault is plain text, so it diffs and versions cleanly. Commit the vault repo and sync it like any other (`git push`/`pull`, or a client such as Working Copy on iOS).
