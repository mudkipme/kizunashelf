# AGENTS.md — KizunaShelf

Guidance for coding agents working in this repo. Read this before implementing a
feature. KizunaShelf is small but unusually layered: **one Rust core serves three
runtimes**, and the data model is **entirely schema-driven**. Both facts change
how you should write code here.

## The two things to remember

1. **The schema is the source of truth for *meaning*. Never guess what a field
   means from its name.** See [Schema-driven, no guessing](#schema-driven-no-guessing).
2. **One Axum router serves web, desktop, and iOS. One Rust-generated OpenAPI
   spec drives both client generators.** If you change the API, the contract and
   *both* generated clients must be regenerated — and the iOS spec lives in
   another repo that CI does not guard. See [The contract](#the-contract-one-source-two-clients).

## What this app is

A schema-driven catalog over an Obsidian-style Markdown vault. Markdown files
(frontmatter + body) are the source of truth; the app reads them into a typed
library and offers browsing, detail pages, calendar/planning, relations,
analytics, cleanup queues, and external-metadata matching. **Files over apps**:
if you stop using KizunaShelf, the Markdown, frontmatter, links, and notes are
still plain files. Content writes (edit frontmatter/body, create/delete entities,
download assets) are gated and can be disabled (read-only mode).

## Layout

```
crates/
  kizunashelf/            the core: Axum router + library indexing + all domain logic
    src/
      api/                router, handlers, mutations, assets, external/*, state, error
      library/            vault indexing, Markdown parse, revision (content hash)
      calendar/  relations  dates  daily_notes  analytics(api/)  secrets  types
      vfs/                Vfs trait + NativeVfs + InMemoryVfs (the storage seam)
      bin/api.rs          the self-hosted web server binary (kizunashelf-api)
      bin/schema.rs       emits the OpenAPI spec (kizunashelf-schema)
  kizunashelf-ffi/        UniFFI wrapper of the router for in-process hosts (iOS)
apps/
  web/                    React 19 + Vite + TanStack Query; consumes the generated client
  desktop/src-tauri/      Tauri shell; api_request IPC drives the same router via oneshot
packages/
  api-contract/           orval-generated TS client + Zod validators (from the OpenAPI spec)
scripts/build-ios.sh      builds KizunaFFI.xcframework + the collapsed iOS openapi.json
docs/                     config.md (the schema reference), asset-download.md, intro.*
```

The native iOS app lives in a **separate repo at `../kizunashelf-ios`** and embeds
this core in-process. Its internals (UniFFI bridge, security-scoped vault FS,
Keychain) are documented in `../kizunashelf-ios/AGENTS.md` — read that for iOS
work; this file covers the core and the cross-cutting seams.

## Schema-driven, no guessing

A vault's `.kizunashelf/config.yaml` (the **vault config**, synced inside the
vault) declares the types, their fields, and each field's **role**. The engine
derives *everything* — titles, dates, covers, relations, external refs, progress,
ratings — from that schema. There is no fixed idea of what a "movie" or a "rating"
is.

**Rules:**

- **Field names are user-defined and arbitrary.** A cover might be `cover_url`,
  `poster`, or `画像`. A title field might be `title`, `name_jp`, or anything.
  **Never branch on a field name to infer semantics.** Code keys off the schema:
  `FieldType` (`title`, `date`, `season`, `image`, `imageList`, `relation`,
  `externalRef`, `enum`, `enumList`, `progress`, `id`, `text`, …) and the role
  enums (`TitleRole`, `DateRole`, `SeasonLanguage`, …). If you need "the cover,"
  ask the schema for the first `Image`/`ImageList` field — don't look for a field
  called `cover`.
- **Meaning flows one direction: schema → behavior.** New semantics belong in the
  schema model (`types.rs`) and the config docs (`docs/config.md`), then in the
  derivation logic — not in special-cased field-name checks scattered through
  handlers or the UI.
- **The web client (`apps/web`) is the behavioral reference.** When iOS or any
  other surface needs to render/derive a field, match what the web client does
  (the iOS Swift helpers even cite the web functions they mirror). Don't invent a
  second interpretation.
- **Preserve unknown values.** Hand-edited frontmatter and config values the UI
  doesn't recognize must be kept, not silently dropped (e.g. unknown title
  languages stay as selectable options; provider mappings aren't stripped when
  the provider catalog is unavailable).

App-level settings (which vault, write mode) are **not** in the vault config —
they are sourced per runtime (see below).

## The core serves three runtimes

One `build_router` (`src/api/router.rs`) is driven three ways:

- **Web** — `kizunashelf-api` binary, real HTTP via `axum::serve`. Config from env
  vars only.
- **Desktop** — Tauri `api_request` command calls the router via `tower` oneshot.
  Config from the in-app vault switcher; credentials in the OS keychain.
- **iOS** — `kizunashelf-ffi` exposes a single `request(method, url, body)` tunnel;
  the Swift app drives the router in-process (no server, no network). Config from
  security-scoped bookmarks; credentials in the Keychain.

Two seams make this work, both injected into `AppState`:

- **`Vfs` trait** (`src/vfs/mod.rs`) — **all** vault I/O. `NativeVfs` (web/desktop),
  `InMemoryVfs` (tests), Swift-backed FS (iOS). **Never use `std::fs` or
  `tokio::fs` to touch the vault folder** — every read/write/list/rename inside the
  vault (entities, assets, config, daily notes, `.trash`) goes through the `Vfs`,
  with vault-relative paths. This is what keeps the core platform-agnostic: on iOS
  there is no real filesystem at those paths, only the Swift-backed VFS. Paths are
  lexically contained (`normalize_relative` rejects `..`/absolute/drive-prefix
  without `canonicalize`). Direct `std::fs`/`tokio::fs` is allowed *only* for
  genuinely non-vault host paths (e.g. the desktop/web config file). Anything that
  lists or reads *inside* the vault must use the `Vfs` — including directory
  autocomplete (`api/path_suggestions.rs` lists vault directories through the VFS
  with vault-relative paths, so it is contained to the vault and works on iOS).
- **`SecretStore` trait** (`src/secrets.rs`) — provider credentials + OAuth token
  cache. Env+file (web), keychain (desktop/iOS).

Per-runtime config sourcing is documented in `docs/config.md`. Don't add an
app-config *file*; `AppConfig` is always passed inline.

## The contract: one source, two clients

The OpenAPI spec is **code-first and generated** — never hand-written. `aide` +
`schemars` build it from the actual route registrations and the `#[derive(JsonSchema)]`
types in `src/contract.rs` (`kizunashelf::api::openapi()`). The `kizunashelf-schema`
bin emits it:

- **Canonical spec** → `packages/api-contract/openapi/kizunashelf.openapi.json`,
  consumed by **orval** to generate the TS client + Zod validators for the web app.
  Regenerate with `pnpm contract:generate`. **CI guards this**: it regenerates and
  `git diff --exit-code`s `packages/api-contract` and `openapi`.
- **Collapsed spec** (`--collapse-nullable-refs`) → committed in the **iOS repo**
  at `../kizunashelf-ios/.../Sources/KizunaCore/openapi.json`, consumed by
  swift-openapi-generator. The flag rewrites schemars' `anyOf: [X, {type: null}]`
  (`Option<NamedType>`) to `X`, which swift-openapi-generator otherwise drops.

> **⚠️ The one drift trap.** When you change the API (a route, a `contract.rs`
> type, an optional/required field), the web contract is CI-guarded but **the iOS
> spec is not** — it's in another repo and is only refreshed by running
> `./scripts/build-ios.sh` on a Mac. Forgetting this is the usual cause of "field
> exists in Rust but missing/optional in Swift." After any API change, treat
> regenerating the iOS spec as part of the change, even though CI won't fail.

### Web: always consume the contract, never hand-write its types

The web app's API types and request/response shapes come **only** from the
generated contract — never hand-write a TypeScript type that mirrors a
`contract.rs` shape, and never hand-roll a `fetch`/URL for an endpoint.

- **API calls** go through the generated client functions, invoked with the
  shared transport: `getEntities(params, { signal }, apiFetch)` (see
  `apps/web/src/api/*.ts`). This gives contract-typed params **and** runtime Zod
  validation. Don't build URLs with `apiFetch(\`/api/...\`)` by hand.
- **Types** are re-exported (or derived) from `@kizunashelf/api-contract` via
  `apps/web/src/types/api.ts`. Add a missing one to the barrel
  (`packages/api-contract/src/index.ts`) and to `types/api.ts` rather than
  declaring a local duplicate. There is intentionally **no** hand-written
  `types/config.ts` (it was removed); the schema-editor config types live in
  `types/api.ts`.
- **orval inlines nested objects** (gotcha): `SaveSettingsRequest["vault"]`,
  `SettingsConfigResponse["vault"]`, and the standalone `VaultConfig` schema are
  three *nominally distinct* types, so mixing them yields TS2719 "two different
  types with this name … unrelated." For shapes used on both the request and
  response side (the settings/schema editor), **derive** the granular types by
  indexed access into one generated type so they unify structurally — e.g.
  `type VaultConfig = NonNullable<SaveSettingsRequest["vault"]>;
  type EntityTypeConfig = VaultConfig["types"][number];` — rather than importing
  the standalone named generated `VaultConfig`/`EntityTypeConfig`. The generated
  (`input`) types are more nullable/optional than hand-written ones, so expect to
  guard with `?? []` / `?? undefined` at the use sites.

## How the iOS app is built

The Swift app has **no hand-written `unsafe`/C** — two generated layers stack:
UniFFI (Rust ⇄ Swift) and swift-openapi-generator (typed client over the FFI
tunnel). `scripts/build-ios.sh` (Mac-only) does the whole pipeline:

```bash
./scripts/build-ios.sh   # builds KizunaFFI.xcframework, kizunashelf_ffi.swift, and the collapsed openapi.json
```

It compiles the core to a static lib for the iOS targets, runs uniffi-bindgen,
runs `kizunashelf-schema --collapse-nullable-refs`, lipos, and assembles the
xcframework. Its outputs are **gitignored**, so a fresh iOS checkout won't compile
until it's run once. End-to-end iOS feature flow: edit the core here → run the
script → build the UI in Xcode. Full detail (the FFI tunnel, the `VaultFileSystem`
/ `HostSecretStore` callback protocols, codegen gotchas) is in
`../kizunashelf-ios/AGENTS.md`.

## Commands

```bash
pnpm install
KIZUNASHELF_VAULT_ROOT=/path/to/vault pnpm dev   # Rust API + Vite (proxies /api → :8787)
pnpm dev:api          # core API only         pnpm dev:web        # web only
pnpm dev:desktop      # Tauri desktop

pnpm test             # cargo test -p kizunashelf  (the test suite; uses InMemoryVfs)
pnpm typecheck        # tsc across packages + cargo check -p kizunashelf
pnpm lint             # oxlint (web)
pnpm contract:generate  # regenerate the OpenAPI spec + TS client (run after API changes)
pnpm build
```

Match what CI runs before pushing (`.forgejo/workflows/ci.yml`):

```bash
cargo fmt                                          # CI does `cargo fmt --check`
cargo clippy -p kizunashelf --all-targets -- -D warnings
cargo test -p kizunashelf
pnpm lint && pnpm typecheck
pnpm contract:generate   # then ensure git diff is clean (CI fails otherwise)
```

There is no iOS job in CI.

## Conventions & gotchas

- **Match surrounding style.** The core favors small total functions, `Option`
  over panics, and explicit `ApiError` constructors (`bad_request`, `conflict`,
  `forbidden`, `bad_gateway`, …). Don't `unwrap`/`panic` on the request path.
- **No direct filesystem access to the vault.** All vault file operations go
  through the `Vfs` trait (above) — no `std::fs`/`tokio::fs` for vault content, or
  the iOS build breaks silently. Reach for `state.vault_vfs(...)`.
- **Writes go through the library, atomically.** Entity/asset writes use temp-file
  + rename (`write_entity_raw`, `write_asset_file`); mutations are revision-guarded
  (409 on stale revision, re-checked against freshly read content). Keep both
  properties when touching write paths.
- **NFC/NFD.** Apple filesystems hand back NFD-decomposed names; wikilink/relation
  matching normalizes to NFC at the chokepoints. Don't compare raw filename bytes.
- **External providers** (`src/api/external/*`) never leak upstream error detail or
  credentials to clients (logged server-side only); token fetches are single-flighted.
- **Asset downloads** validate URLs against an SSRF guard (private/loopback/link-local
  blocked, `198.18.0.0/15` allowlisted, redirects re-validated per hop); the opt-in
  `KIZUNASHELF_ALLOW_PRIVATE_ASSET_HOSTS` relaxes it for trusted LAN hosts.
- **The library cache** is TTL'd and invalidated on writes; analytics is memoized on
  `library.generated_at`. After a write that should be visible, `invalidate_cache`.

## Reference

- `docs/config.md` — the full schema/config reference (types, fields, roles,
  per-runtime config, provider credentials). The authority for what the schema means.
- `docs/asset-download.md` — asset/cover download behavior.
- `../kizunashelf-ios/AGENTS.md` — the iOS app's architecture and the regenerate-bindings workflow.
- `README.md` — product overview and quick start.
