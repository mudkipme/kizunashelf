# Local Cover / Asset Download

Status: complete (Phases 1-4)

## Goal

Download remote cover/image URLs into the vault so the files are truly yours
(files-over-apps), instead of depending on third-party CDNs that rot. Covers a
single-entity download, a batch download with an in-memory task manager, reliable
error handling (keep the original URL on failure and retry next time), and serving
the locally stored assets to both the web and desktop runtimes.

## How covers work today (baseline)

- An `image` / `imageList` field (e.g. `cover_url`) holds a **remote URL** in
  entity frontmatter.
- `library/mod.rs` lifts the first such field into `EntitySummary.image`
  (`first_field_string_for_types`, `FieldType::Image | FieldType::ImageList`).
- The frontend renders it raw: `<img src={entity.image}>`
  (`entity-cover.tsx`, `entity-grid-item.tsx`). There is **no static route for
  vault files** today — only `ServeDir` for the web build (`router.rs`).
- Writes go through `mutations.rs`: revision check -> `backup_file` ->
  path-validated write -> cache invalidate. Path safety is `ensure_path_inside_root`.
- Desktop has **no HTTP listener**; `apiFetch` routes through a Tauri `api_request`
  command whose response `body` is a **String** (`client.ts`) — binary-unsafe.

## Key design decisions

1. **Asset location** — new config `assetRoot` (default `Assets`), relative to
   `vaultRoot`. Files are grouped by entity, mirroring the entity's vault-relative
   path minus `.md`:
   - `image` (single): `<assetRoot>/<entity-rel-dir>/<basename>/<field>.<ext>`
   - `imageList` (multiple): `<assetRoot>/<entity-rel-dir>/<basename>/<field>/<key>.<ext>`

2. **Frontmatter write-back** — on success, replace the field value (the remote
   URL) with the **vault-relative local path** (plain relative path; resolvable by
   Obsidian Bases). On **failure/timeout the frontmatter is left untouched** so the
   remote URL persists and the user can retry. For `imageList`, the value stays a
   YAML array and each element is replaced independently — a partial array of local
   paths + remaining remote URLs is legal and re-runnable.

3. **`imageList` path format** — keep a YAML array, one file per element in a
   per-field subdirectory. Filenames are a **short hash of the source URL**, not the
   array index, so re-runs are idempotent (same URL -> same file -> skip) and
   order-independent. Original array order is preserved on write-back.

4. **Serving local assets** — distinguish by URL scheme at render time:
   - Web: new `GET /api/assets/{*path}` route streaming files from inside
     `assetRoot` (path-validated), with content-type + cache headers.
   - Desktop: the String-body command bridge can't carry binary, so the desktop
     app registers a custom async URI scheme (`kizasset://`) that forwards to the
     in-process `/api/assets/{path}` route and returns the raw bytes. This reuses
     the serve route's path validation and keeps binary data binary. (Chosen over
     Tauri's `convertFileSrc`/asset-protocol scope, which would require static
     scope config + runtime scope extension + absolute paths on the client.)
   - Frontend `resolveAssetSrc(value)`: `http(s):` / `data:` / `blob:` -> return
     as-is (back-compat); otherwise -> `/api/assets/<path>` on web, or
     `kizasset://localhost/<path>` (Windows/Android: `http://kizasset.localhost/...`)
     in the Tauri runtime.

5. **Reliability** — atomic download (`.tmp` -> rename), per-download timeout,
   max size cap (~20 MB), content-type sniffing (reject non-images / HTML error
   pages), bounded concurrency, never mutate frontmatter on failure. Reuse
   `backup_file` before rewriting the `.md`. A single shared `reqwest::Client`
   lives in `AppState` (also fixes the per-request-client inefficiency).

6. **Capabilities** — `asset_download_enabled` (= content writes enabled) gates the
   UI; the serve route is always available.

## Rename / delete handling

The frontmatter stores an **explicit relative path**, so correctness never depends
on the asset folder name matching the entity name.

- **Inside-app rename** (`rename_to` in `update_entity`): move the asset dir from
  `<oldBasename>/` to `<newBasename>/` and prefix-rewrite any `image`/`imageList`
  frontmatter values that point inside the old dir. **Best-effort and non-fatal**:
  if the move fails or the target dir already exists, leave the old paths (they
  still resolve). Reuses `ensure_path_inside_root`.
- **Inside-app delete**: trash the asset dir alongside the `.md` (mirrors the
  existing trash bucket). (Phase 3.)
- **External rename/delete**: nothing breaks — the explicit path still resolves.
  Folder-name drift is cosmetic. A dangling path (file actually missing) is surfaced
  by the cleanup queue (follow-up), with a re-download action.

## Collision rule (cross-entity, from out-of-app filename swaps)

Each live entity maps to a unique asset dir, so two **current** entities never
compute the same path. The only real collision is a **stale occupant**: a file
written when a different entity lived at that path, still referenced by that other
entity after an out-of-app rename/swap. Overwriting it would corrupt the other
entity's cover.

Rule — **never overwrite a file another entity references.** Build a set of asset
paths referenced by *other* entities from the in-memory `Library` (once per single
download; once at batch-job start). For each intended path `P`:

1. `P` is this entity's own current value, or an unreferenced file inside its own
   dir -> **overwrite** (normal idempotent re-download; reclaim orphans).
2. `P` is referenced by a *different* entity -> **do not overwrite**; write a
   disambiguated name `<field>-<shorthash(entityId)>.<ext>` and point this entity's
   frontmatter there.
3. `P` exists but nobody references it (true orphan in our dir) -> overwrite/reclaim.

`imageList` is already URL-hashed; "this exact URL already downloaded and ours" ->
**skip (idempotent)**. Surface conflict resolution in the per-field result
(`conflictResolved` / a note) so it is visible, not silent.

## API surface

- `GET /api/assets/{*path}` — serve a vault asset (path-validated inside
  `assetRoot`). `403` on escape.
- `POST /api/entities/{id}/assets/download` — single entity. Body: revision +
  optional field list. Downloads remote `image`/`imageList` values, rewrites
  frontmatter on success, leaves URLs on failure. Returns the updated entity +
  per-field results. Partial success is normal.
- `POST /api/assets/download-jobs` — batch. Selector: per-type or whole library
  (`{ entityType?, onlyRemote: true }`). Returns a job id; runs in a background
  `tokio::task` with bounded concurrency.
- `GET /api/assets/download-jobs/{id}` — job status for polling.
- `GET /api/assets/download-jobs` — list jobs.
- (optional) `POST /api/assets/download-jobs/{id}/cancel`.

Job state is **in-memory only** (`Arc<Mutex<HashMap<JobId, JobState>>>` in
`AppState`). After a restart the user simply re-runs; already-downloaded images are
skipped, so resume is free.

## Contract / types

New structs in `contract.rs`: `AssetDownloadRequest`, `AssetDownloadResult` (per
field/image: `downloaded | skipped | failed`, new path or error, `conflictResolved`),
`AssetDownloadResponse` (updated `Entity` + results), and the job types. Add
`asset_root` (and vault root for desktop resolution) to `ConfigResponse`, and
`asset_download_enabled` to `CapabilitiesResponse`. Regenerate with
`pnpm contract:generate`.

## Frontend

- `lib/asset-src.ts`: `resolveAssetSrc(value)` (scheme detection + runtime branch),
  used by `entity-cover.tsx`, `entity-grid-item.tsx`, `entity-detail.tsx`, and the
  external-match preview. Add `onError` fallback to the existing placeholder.
- `api/assets.ts`: thin wrappers over generated functions.
- Entity detail/edit: "Download cover" button when `image` is remote and
  `asset_download_enabled`; spinner -> optimistic swap on success, toast on failure
  (URL kept). Targeted query invalidation.
- External-match apply: optional "Download cover locally" checkbox.
- Batch UI on the review/cleanup page: per-type + whole-library actions, progress
  panel polling job status.

## Tests

- In-test mock image server (small axum/hyper, or `wiremock` dev-dep — the suite is
  currently network-free). Cover: success writes file under `assetRoot` +
  frontmatter rewritten to local path; failure keeps URL; non-image rejected; size
  cap; serve-route path escape -> 403; idempotent skip; cross-entity collision ->
  disambiguated name; batch job reaches `completed` with mixed counts; read-only ->
  403.

## Phased rollout

1. **Phase 1 (this work):** config `assetRoot` + download core + serve route +
   single-entity endpoint + capabilities/config contract + frontend
   `resolveAssetSrc` + "Download cover" button + tests.
2. **Phase 2 (done):** desktop `kizasset://` custom URI scheme forwarding to the
   serve route; `resolveAssetSrc` desktop branch. Note: the desktop crate
   (`kizunashelf-desktop`) requires GTK/WebKit dev libraries to compile, which are
   not present in all dev environments — verify with `pnpm dev:desktop`.
3. **Phase 3 (done):** in-memory batch job manager (`/api/asset-jobs`,
   per-type + whole-library, bounded concurrency, cancel) with a review-page
   download panel that polls job status; inside-app rename asset-move +
   frontmatter rewrite; delete trashes the asset dir; `broken-asset` cleanup
   queue flagging local covers whose file is missing.
4. **Phase 4 (done):** external-match "download cover locally" opt-in. The
   shared `useExternalMatch` hook owns the `downloadAfterApply` flag and a
   `maybeDownloadCover(entity)` method; `ExternalMatchDialog` renders one
   checkbox (shown only when the selection sets a remote image and asset
   download is enabled); each consumer (view / edit / create) calls
   `maybeDownloadCover` at its own persist point. The download localizes the
   cover that the applied match wrote.
