# KizunaShelf — Architecture & Contributor Guide

KizunaShelf is small but unusually layered. Two facts shape almost every decision in the codebase, and reading them first will save you from the most common mistakes:

> **Two invariants**
>
> 1. **The schema is the source of truth for *meaning*.** Field names are arbitrary and user-defined — never infer what a field *is* from what it's *called*. See [Schema-driven: never guess a field's meaning](#schema-driven-never-guess-a-fields-meaning).
> 2. **One Rust core serves three runtimes, behind one generated API contract.** A single Axum router backs web, desktop, and iOS, and one Rust-generated OpenAPI spec drives both client generators. Change the API and *both* generated clients must be regenerated — and the iOS spec lives in another repo. See [The API contract](#the-api-contract-one-source-two-clients).

**Contents**

- [What KizunaShelf is](#what-kizunashelf-is)
- [Repository layout](#repository-layout)
- [Schema-driven: never guess a field's meaning](#schema-driven-never-guess-a-fields-meaning)
- [One core, three runtimes](#one-core-three-runtimes)
- [The API contract: one source, two clients](#the-api-contract-one-source-two-clients)
- [Building the iOS app](#building-the-ios-app)
- [Development & commands](#development--commands)
- [Conventions & gotchas](#conventions--gotchas)
- [Further reading](#further-reading)

## What KizunaShelf is

A schema-driven catalog over an Obsidian-style Markdown vault. Markdown files (frontmatter + body) are the source of truth; the app reads them into a typed library and offers browsing, detail pages, calendar/planning, relations, analytics, cleanup queues, and external-metadata matching.

The guiding principle is **files over apps**: if you stop using KizunaShelf, the Markdown, frontmatter, links, and notes are still plain files you own. Content writes (editing frontmatter/body, creating/deleting entities, downloading assets) are gated and can be disabled entirely — read-only mode.

## Repository layout

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

The native iOS app lives in a **separate repo at `../kizunashelf-ios`** and embeds this core in-process. Its internals (UniFFI bridge, security-scoped vault FS, Keychain) are documented in `../kizunashelf-ios/ARCHITECTURE.md` — read that for iOS work. This document covers the core and the cross-cutting seams.

## Schema-driven: never guess a field's meaning

A vault's `.kizunashelf/config.yaml` (the **vault config**, synced inside the vault) declares the types, their fields, and each field's **role**. The engine derives *everything* — titles, dates, covers, relations, external refs, progress, ratings — from that schema. There is no built-in idea of what a "movie" or a "rating" is; those concepts exist only as schema configuration.

This is the single most important thing to internalize, because it inverts the usual instinct. The rules that follow all flow from it:

- **Field names are user-defined and arbitrary.** A cover might be `cover_url`, `poster`, or `画像`. A title field might be `title`, `name_jp`, or anything at all. **Never branch on a field name to infer semantics.** Code keys off the schema instead: the `FieldType` (`title`, `date`, `season`, `image`, `imageList`, `relation`, `externalRef`, `enum`, `enumList`, `progress`, `id`, `text`, …) and the role enums (`TitleRole`, `DateRole`, `SeasonLanguage`, …). If you need "the cover," ask the schema for the first `Image`/`ImageList` field — don't look for a field literally named `cover`.
- **Meaning flows one direction: schema → behavior.** New semantics belong in the schema model (`types.rs`) and the config docs (`docs/config.md`), then in the derivation logic — *not* in special-cased field-name checks scattered through handlers or the UI.
- **Preserve unknown values.** Hand-edited frontmatter and config values the UI doesn't recognize must be kept, not silently dropped — e.g. unknown title languages stay selectable, and provider mappings aren't stripped when the provider catalog is unavailable.

App-level settings (which vault, write mode) are deliberately **not** in the vault config; they're sourced per runtime, as described next.

## One core, three runtimes

A single `build_router` (`src/api/router.rs`) is driven three different ways. The router, the domain logic, and the data model are identical across all three — only how requests arrive and where config comes from differs:

- **Web** — the `kizunashelf-api` binary serves real HTTP via `axum::serve`. Config comes from environment variables only.
- **Desktop** — a Tauri `api_request` command calls the router via a `tower` oneshot (no network). Config comes from the in-app vault switcher; credentials live in the OS keychain.
- **iOS** — `kizunashelf-ffi` exposes a single `request(method, url, body)` tunnel; the Swift app drives the router in-process (no server, no network). Config comes from security-scoped bookmarks; credentials live in the Keychain.

Two seams make this portability possible. Both are traits injected into `AppState`:

- **`Vfs` trait** (`src/vfs/mod.rs`) — **all** vault I/O goes through it: `NativeVfs` (web/desktop), `InMemoryVfs` (tests), and a Swift-backed FS (iOS). **Never use `std::fs` or `tokio::fs` to touch the vault folder.** Every read/write/list/rename inside the vault (entities, assets, config, daily notes, `.trash`) goes through the `Vfs` with vault-relative paths. This is what keeps the core platform-agnostic: on iOS there is no real filesystem at those paths, only the Swift-backed VFS. Paths are lexically contained — `normalize_relative` rejects `..`, absolute paths, and drive prefixes without `canonicalize`. Direct `std::fs`/`tokio::fs` is allowed *only* for genuinely non-vault host paths owned by a runtime (e.g. the desktop vault list, the web token cache, or the persistent index cache). Anything that lists or reads *inside* the vault must use the `Vfs` — including directory autocomplete (`api/path_suggestions.rs` lists vault directories through the VFS with vault-relative paths, so it stays contained and works on iOS).
- **`SecretStore` trait** (`src/secrets.rs`) — provider credentials and the OAuth token cache. Env+file (web), keychain (desktop/iOS).

Per-runtime config sourcing is documented in `docs/config.md`. Don't add an app-config *file* in core; `AppConfig` is always passed inline.

## The API contract: one source, two clients

The OpenAPI spec is **code-first and generated** — never hand-written. `aide` + `schemars` build it from the actual route registrations and the `#[derive(JsonSchema)]` types in `src/contract.rs` (`kizunashelf::api::openapi()`). The `kizunashelf-schema` bin emits two forms:

- **Canonical spec** → `packages/api-contract/openapi/kizunashelf.openapi.json`, consumed by **orval** to generate the TS client + Zod validators for the web app. Regenerate with `pnpm contract:generate`. **CI guards this**: it regenerates and `git diff --exit-code`s `packages/api-contract` and `openapi`.
- **Collapsed spec** (`--collapse-nullable-refs`) → committed in the **iOS repo** at `../kizunashelf-ios/.../Sources/KizunaCore/openapi.json`, consumed by swift-openapi-generator. The flag rewrites schemars' `anyOf: [X, {type: null}]` (`Option<NamedType>`) to `X`, which swift-openapi-generator otherwise drops.

### Consuming the contract from the web app

The web app's API types and request/response shapes come **only** from the generated contract. Never hand-write a TypeScript type that mirrors a `contract.rs` shape, and never hand-roll a `fetch`/URL for an endpoint.

- **API calls** go through the generated client functions, invoked with the shared transport: `getEntities(params, { signal }, apiFetch)` (see `apps/web/src/api/*.ts`). This gives contract-typed params **and** runtime Zod validation. Don't build URLs with `apiFetch(\`/api/...\`)` by hand.
- **Types** are re-exported (or derived) from `@kizunashelf/api-contract` via `apps/web/src/types/api.ts`. Add a missing one to the barrel (`packages/api-contract/src/index.ts`) and to `types/api.ts` rather than declaring a local duplicate.
- **orval inlines nested objects** (gotcha): `SaveSettingsRequest["vault"]`, `SettingsConfigResponse["vault"]`, and the standalone `VaultConfig` schema are three *nominally distinct* types, so mixing them yields TS2719 "two different types with this name … unrelated." For shapes used on both the request and response side (the settings/schema editor), **derive** the granular types by indexed access into one generated type so they unify structurally — e.g. `type VaultConfig = NonNullable<SaveSettingsRequest["vault"]>; type EntityTypeConfig = VaultConfig["types"][number];` — rather than importing the standalone named generated `VaultConfig`/`EntityTypeConfig`. The generated (`input`) types are more nullable/optional than hand-written ones, so expect to guard with `?? []` / `?? undefined` at the use sites.

## Building the iOS app

The Swift app has **no hand-written `unsafe`/C** — two generated layers stack: UniFFI (Rust ⇄ Swift) and swift-openapi-generator (a typed client over the FFI tunnel). `scripts/build-ios.sh` (Mac-only) runs the whole pipeline:

```bash
./scripts/build-ios.sh   # builds KizunaFFI.xcframework, kizunashelf_ffi.swift, and the collapsed openapi.json
```

It compiles the core to a static lib for the iOS targets, runs uniffi-bindgen, runs `kizunashelf-schema --collapse-nullable-refs`, lipos the slices, and assembles the xcframework. Its outputs are **gitignored**, so a fresh iOS checkout won't compile until the script has been run once.

The end-to-end iOS feature flow is: edit the core here → run the script → build the UI in Xcode. Full detail (the FFI tunnel, the `VaultFileSystem` / `HostSecretStore` callback protocols, codegen gotchas) is in `../kizunashelf-ios/ARCHITECTURE.md`.

## Development & commands

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

Before pushing, match what CI runs (`.forgejo/workflows/ci.yml`):

```bash
cargo fmt                                          # CI does `cargo fmt --check`
cargo clippy -p kizunashelf --all-targets -- -D warnings
cargo test -p kizunashelf
pnpm lint && pnpm typecheck
pnpm contract:generate   # then ensure git diff is clean (CI fails otherwise)
```

## Conventions & gotchas

- **Match the surrounding style.** The core favors small total functions, `Option` over panics, and explicit `ApiError` constructors (`bad_request`, `conflict`, `forbidden`, `bad_gateway`, …). Don't `unwrap`/`panic` on the request path.
- **No direct filesystem access to the vault.** All vault file operations go through the `Vfs` trait — no `std::fs`/`tokio::fs` for vault content, or the iOS build breaks silently. Reach for `state.vault_vfs(...)`.
- **Writes go through the library, atomically.** Entity/asset writes use temp-file + rename (`write_entity_raw`, `write_asset_file`); mutations are revision-guarded (409 on a stale revision, re-checked against freshly read content). Keep both properties when touching write paths.
- **NFC/NFD normalization.** Apple filesystems hand back NFD-decomposed names; wikilink/relation matching normalizes to NFC at the chokepoints. Don't compare raw filename bytes.
- **Asset downloads** validate URLs against an SSRF guard (private/loopback/link-local blocked, `198.18.0.0/15` allowlisted, redirects re-validated per hop); the opt-in `KIZUNASHELF_ALLOW_PRIVATE_ASSET_HOSTS` relaxes it for trusted LAN hosts.

## Further reading

- `docs/config.md` — the full schema/config reference (types, fields, roles, per-runtime config, provider credentials). The authority for what the schema means.
- `docs/asset-download.md` — asset/cover download behavior.
- `../kizunashelf-ios/ARCHITECTURE.md` — the iOS app's architecture and the regenerate-bindings workflow.
- `README.md` — product overview and quick start.
