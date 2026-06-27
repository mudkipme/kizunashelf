# Syncing KizunaShelf

**KizunaShelf does not sync anything itself.** Following the *files over apps* principle, a vault is just a folder of Markdown, frontmatter, links, notes, and downloaded assets — plus the schema at `KizunaShelf/config.yaml`. To use a vault on more than one device, sync that folder with **whatever syncing method you already use**. KizunaShelf simply reads whatever files are present on disk; getting the same files onto each device is the sync tool's job, not KizunaShelf's.

Because the schema lives *inside* the vault (`<vaultRoot>/KizunaShelf/config.yaml`), it travels with the vault automatically — every device pointing at the synced folder shares the same schema. There is nothing KizunaShelf-specific to configure on the other end: open the synced folder as a vault and onboarding is skipped because the config is already there.

## The config folder is visible on purpose

The vault config lives in a **visible** folder (`KizunaShelf/`, not a hidden dot-folder). Most Obsidian sync methods skip hidden files — including official Obsidian Sync, which has no hidden-file support at all — so a dot-folder config would silently fail to reach your other devices. A visible folder syncs with every method (Obsidian LiveSync, iCloud, Syncthing, …). The folder also holds other app-owned, sync-worthy artifacts — for example saved lists under `KizunaShelf/Lists/` — so they travel with the vault too.

> **Official Obsidian Sync users:** Markdown always syncs, but `.yaml` is a non-Markdown extension, so enable **Settings → Sync → Sync all other types** (per device) for the config to travel. You may also want **Settings → Files & links → Detect all file extensions** so Obsidian shows the file at all. Other sync tools (LiveSync, iCloud, Syncthing) need no extra setting once the file is out of the dot-folder.

## Syncing methods that live inside Obsidian

If your sync method runs *inside* Obsidian — official **Obsidian Sync** or community plugins like **Self-hosted LiveSync** — then syncing only happens while **Obsidian is open**. KizunaShelf can't trigger those plugins; it doesn't run Obsidian. So the workflow is:

- **Open Obsidian on the source device** and let it finish pushing changes before you expect them elsewhere.
- **Open Obsidian on the target device** to pull changes down before opening KizunaShelf there.

If you'd rather not depend on Obsidian being open, use a sync method that runs at the filesystem/OS level instead (iCloud Drive, Syncthing, Dropbox, git, …) — those sync the folder regardless of which apps are running.

## iOS

On iOS, KizunaShelf opens a vault folder from the Files app via a security-scoped bookmark, so any Files-provider that exposes the vault as a **readable directory** works.

- **iCloud Drive is recommended.** It's built into Files, keeps the folder available offline, and syncs in the background without any app open. Put the vault in iCloud Drive and open it from Files.
- **Other file-provider apps that expose a readable directory are technically supported** — for example **ShellFish** (SFTP), **S3 Files**, and **Working Copy** (git). Anything that mounts the vault as a normal Files directory KizunaShelf can read will work, though background/offline behavior depends on that provider.

## Versioning

None of the above keeps history — they sync the current state. If you want **version history** (the ability to roll back, or to review changes over time), use **git**. The vault is plain text, so it diffs and versions cleanly. Commit the vault repo and sync it like any other (`git push`/`pull`, or a client such as Working Copy on iOS).
