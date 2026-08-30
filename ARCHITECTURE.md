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
- [Internationalization](#internationalization)
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
      bin/docs.rs         emits the manual's generated reference pages (kizunashelf-docs)
  kizunashelf-ffi/        UniFFI wrapper of the router for in-process hosts (iOS)
apps/
  web/                    React 19 + Vite + TanStack Query; consumes the generated client
  desktop/src-tauri/      Tauri shell; api_request IPC drives the same router via oneshot
packages/
  api-contract/           orval-generated TS client + Zod validators (from the OpenAPI spec)
scripts/build-ios.sh      builds KizunaFFI.xcframework + the collapsed iOS openapi.json
manual/                   the user manual + landing page (Docusaurus site); content/reference/ is the schema reference (config.md is the entry page)
```

The native iOS app lives in a **separate repo at `../kizunashelf-ios`** and embeds this core in-process. Its internals (UniFFI bridge, managed/security-scoped vault FS, Keychain, background downloads, widgets, reminders, Spotlight, sharing, and App Intents) are documented in `../kizunashelf-ios/ARCHITECTURE.md` — read that for iOS work. This document covers the core and the cross-cutting seams.

## Schema-driven: never guess a field's meaning

A vault's `KizunaShelf/config.yaml` (the **vault config**, synced inside the vault) declares the types, their fields, and each field's **role**. The engine derives *everything* — titles, dates, covers, relations, external refs, progress, ratings — from that schema. There is no built-in idea of what a "movie" or a "rating" is; those concepts exist only as schema configuration.

This is the single most important thing to internalize, because it inverts the usual instinct. The rules that follow all flow from it:

- **Field names are user-defined and arbitrary.** A cover might be `cover_url`, `poster`, or `画像`. A title field might be `title`, `name_jp`, or anything at all. **Never branch on a field name to infer semantics.** Code keys off the schema instead: the `FieldType` (`title`, `date`, `season`, `image`, `imageList`, `relation`, `externalRef`, `enum`, `enumList`, `progress`, `id`, `text`, …) and the role enums (`TitleRole`, `DateRole`, `SeasonLanguage`, …). If you need "the cover," ask the schema for the first `Image`/`ImageList` field — don't look for a field literally named `cover`.
- **Meaning flows one direction: schema → behavior.** New semantics belong in the schema model (`types.rs`) and the config docs (`manual/content/reference/`), then in the derivation logic — *not* in special-cased field-name checks scattered through handlers or the UI.
- **Preserve unknown values.** Hand-edited frontmatter and config values the UI doesn't recognize must be kept, not silently dropped — e.g. unknown title languages stay selectable, and provider mappings aren't stripped when the provider catalog is unavailable.

App-level settings (which vault, write mode) are deliberately **not** in the vault config; they're sourced per runtime, as described next.

## One core, three runtimes

A single `build_router` (`src/api/router.rs`) is driven three different ways. The router, the domain logic, and the data model are identical across all three — only how requests arrive and where config comes from differs:

- **Web** — the `kizunashelf-api` binary serves real HTTP via `axum::serve`. Config comes from environment variables only.
- **Desktop** — a Tauri `api_request` command calls the router via a `tower` oneshot (no HTTP server or network hop). Config comes from the in-app vault switcher; credentials live in the OS keychain.
- **iOS** — `kizunashelf-ffi` exposes a single `request(method, url, body)` tunnel; the Swift app drives the router in-process (no HTTP server or network hop between SwiftUI and the core). Managed vault paths or security-scoped bookmarks select the vault; credentials live in the Keychain. External metadata, remote covers, and cloud/File Provider synchronization can still use the network.

The shared core owns portable domain behavior. A native host still owns behavior that only its platform can provide: vault selection, credential storage, lifecycle/background execution, and OS integrations. In particular, the iOS app owns its widgets, reminders, Spotlight index, Share extension, App Intents, and background `URLSession` transfers; those features call into the core instead of duplicating its schema or mutation logic.

Two seams make this portability possible. Both are traits injected into `AppState`:

- **`Vfs` trait** (`src/vfs/mod.rs`) — **all** vault I/O goes through it: `NativeVfs` (web/desktop), `InMemoryVfs` (tests), and a Swift-backed FS (iOS). **Never use `std::fs` or `tokio::fs` to touch the vault folder.** Every read/write/list/rename inside the vault (entities, assets, config, daily notes, `.trash`) goes through the `Vfs` with vault-relative paths. This is what keeps the core platform-agnostic: on iOS there is no real filesystem at those paths, only the Swift-backed VFS. Paths are lexically contained — `normalize_relative` rejects `..`, absolute paths, and drive prefixes without `canonicalize`. Direct `std::fs`/`tokio::fs` is allowed *only* for genuinely non-vault host paths owned by a runtime (e.g. the desktop vault list, the web token cache, or the persistent index cache). Anything that lists or reads *inside* the vault must use the `Vfs` — including directory autocomplete (`api/path_suggestions.rs` lists vault directories through the VFS with vault-relative paths, so it stays contained and works on iOS).
- **`SecretStore` trait** (`src/secrets.rs`) — provider credentials and the OAuth token cache. Env+file (web), keychain (desktop/iOS).

Per-runtime config sourcing is documented in `manual/content/reference/config.md`. Don't add an app-config *file* in core; `AppConfig` is always passed inline.

**Logging follows the same shape**: the core only ever *emits* `tracing` events and never installs a subscriber, so each host decides where they go (the web binary writes to stdout filtered by `RUST_LOG`; the desktop shell writes to stderr; iOS can forward to `OSLog`). With no subscriber installed — tests, an embedding host that wants silence — every event compiles to a no-op. Two chokepoints carry almost all of it, so instrumenting a new handler is usually unnecessary: `trace_requests` (`api/router.rs`) opens the per-request span everything else nests under and logs failures axum rejected before a handler ran, and `ApiError::into_response` logs every failure the app itself decides on, with the message the client is about to see. Prefer adding a field to those over sprinkling events through handlers, and keep `println!`/`eprintln!` out of the core — they bypass the host's filtering and, on iOS, go nowhere.

## The API contract: one source, two clients

The OpenAPI spec is **code-first and generated** — never hand-written. `aide` + `schemars` build it from the actual route registrations and the `#[derive(JsonSchema)]` types in `src/contract.rs` (`kizunashelf::api::openapi()`). The `kizunashelf-schema` bin emits two forms:

- **Canonical spec** → `packages/api-contract/openapi/kizunashelf.openapi.json`, consumed by **orval** to generate the TS client + Zod validators for the web app. Regenerate with `pnpm contract:generate`. **CI guards this**: it regenerates and `git diff --exit-code`s `packages/api-contract` and `openapi`.
- **Collapsed spec** (`--collapse-nullable-refs`) → committed in the **iOS repo** at `../kizunashelf-ios/.../Sources/KizunaCore/openapi.json`, consumed by swift-openapi-generator. The flag rewrites schemars' `anyOf: [X, {type: null}]` (`Option<NamedType>`) to `X`, which swift-openapi-generator otherwise drops.

### Consuming the contract from the web app

The web app's API types and request/response shapes come **only** from the generated contract. Never hand-write a TypeScript type that mirrors a `contract.rs` shape, and never hand-roll a `fetch`/URL for an endpoint.

- **API calls** go through the generated client functions, invoked with the shared transport: `getEntities(params, { signal }, apiFetch)` (see `apps/web/src/api/*.ts`). This gives contract-typed params **and** runtime Zod validation. Don't build URLs with `apiFetch(\`/api/...\`)` by hand.
- **Types** are re-exported (or derived) from `@kizunashelf/api-contract` via `apps/web/src/types/api.ts`. Add a missing one to the barrel (`packages/api-contract/src/index.ts`) and to `types/api.ts` rather than declaring a local duplicate.
- **orval inlines nested objects** (gotcha): `SaveSettingsRequest["vault"]`, `SettingsConfigResponse["vault"]`, and the standalone `VaultConfig` schema are three *nominally distinct* types, so mixing them yields TS2719 "two different types with this name … unrelated." For shapes used on both the request and response side (the settings/schema editor), **derive** the granular types by indexed access into one generated type so they unify structurally — e.g. `type VaultConfig = NonNullable<SaveSettingsRequest["vault"]>; type EntityTypeConfig = VaultConfig["types"][number];` — rather than importing the standalone named generated `VaultConfig`/`EntityTypeConfig`. The generated (`input`) types are more nullable/optional than hand-written ones, so expect to guard with `?? []` / `?? undefined` at the use sites.

## Internationalization

The UI is localized (web + desktop share the web bundle; iOS ships its own catalogs) into English, Japanese, Simplified and Traditional Chinese, driven by the **single existing language preference** — there is no separate UI-language picker. The preference value space is the content-language list (`languages.rs`) with the Chinese entry split into `zh-Hans`/`zh-Hant`; UI locale, title language, and per-provider request language all *derive* from that one value.

> **The i18n invariant**
>
> Script subtags (`-Hans`/`-Hant`) live in exactly two places: the **UI locale** and **outbound requests to providers that distinguish them**. Everywhere data is stored, keyed, or matched — frontmatter `titles`, schema `titleLanguage`, candidate/dedup keys, `titles[lang]` lookup — Chinese is always bare **`zh`**. `primary_language()` (core) and the primary-subtag derivation (`useTitleLanguage`, web) are the chokepoints that strip the subtag before anything touches stored data. This mirrors invariant #1: the same reason a `zh-Hant` script tag must never reach frontmatter is why field names must never leak into logic — meaning lives in one place, not scattered.

- **The core owns the derivation policy.** `languages.rs::user_languages()` is the picker's option list (`{ code, label (endonym), titleLanguage }`, Chinese split into two), served on `GET /api/languages` and consumed by both clients instead of hardcoding the mapping. `primary_language(code)` maps a preference to its bare title language (`zh-Hans` → `zh`, `en-US` → `en`); providers normalize the full preference internally (`thetvdb_language` strips the subtag, `tmdb.rs` has an explicit `zh-CN`/`zh-TW`/`ja-JP` locale map, `bangumi.rs` keys off `starts_with("zh")`). The viewer language rides on `ProviderSearchConfig.language` (per-request context), so the provider `search` signatures stay untouched.
- **Which UI languages a client is *translated* into is NOT in the contract.** It's a per-client build fact — web derives it from its shipped `UI_LOCALES` (`lib/i18n.ts`: `en`, `ja`, `zh-Hans`, `zh-Hant`), iOS from its shipped String Catalog set. `user_languages()` lists *every* preference option; a client shows a "UI in English" hint for the ones it hasn't translated. Don't add a `uiSupported` flag to the contract.
- **Web i18n is Lingui** (`@lingui/react` + macros). Source strings are inline English prose wrapped in place with `<Trans>` / `` t`…` `` — no invented keys; `pnpm i18n:extract` (`lingui extract --clean`) regenerates the catalogs at `apps/web/src/locales/{en,ja,zh-Hans,zh-Hant}/messages.po` mechanically. **CI guards this exactly like the API contract**: it re-extracts and `git diff --exit-code`s `apps/web/src/locales`, so run `pnpm i18n:extract` and commit after adding or changing any UI string. The macro transform runs as a separate Vite Babel pass (`@rolldown/plugin-babel`, since oxc-based `@vitejs/plugin-react` has no Babel hook); `en` is bundled, the others lazy-load via an explicit loader map in `lib/i18n.ts`.
- **The preference stays per-device** (web localStorage `kizunashelf.language.v2` / iOS UserDefaults), never in the vault config. A first-run default sniffs `navigator.languages` / system locale for the Chinese script. Formatting and collation route through the derived UI locale (`lib/locale.ts`); entity-title elements carry a `lang` attribute from the title's own map key for correct Han glyph selection.

Intentionally **not** localized: server `ApiError` messages (they surface verbatim in toasts, English in v1) and schema/config-driven labels (type/field/enum/provider names — those are user data, not app copy). Hans↔Hant folding in search/dedup is deferred.

## Building the iOS app

The Swift app has **no hand-written `unsafe`/C** — two generated layers stack: UniFFI (Rust ⇄ Swift) and swift-openapi-generator (a typed client over the FFI tunnel). `scripts/build-ios.sh` (Mac-only) runs the whole pipeline:

```bash
./scripts/build-ios.sh   # builds KizunaFFI.xcframework, kizunashelf_ffi.swift, and the collapsed openapi.json
```

It compiles the core to a static lib for the iOS targets, runs uniffi-bindgen, runs `kizunashelf-schema --collapse-nullable-refs`, lipos the slices, and assembles the xcframework. The xcframework and generated UniFFI Swift glue are **gitignored**, so a fresh iOS checkout won't compile until the script has been run once. The collapsed `openapi.json` is generated by the same script but committed so Xcode's OpenAPI plugin has a versioned input.

The end-to-end iOS feature flow is: edit the core here → run the script → build the UI in Xcode. Full detail (the FFI tunnel, the `VaultFileSystem` / `HostSecretStore` callback protocols, codegen gotchas) is in `../kizunashelf-ios/ARCHITECTURE.md`.

## Development & commands

```bash
pnpm install
KIZUNASHELF_VAULT_ROOT=/path/to/vault pnpm dev   # Rust API + Vite (proxies /api → :8787)
pnpm dev:api          # core API only         pnpm dev:web        # web only
pnpm dev:desktop      # Tauri desktop
pnpm docs:dev         # the manual + landing page (Docusaurus)

pnpm test             # cargo test -p kizunashelf  (the test suite; uses InMemoryVfs)
pnpm typecheck        # tsc across packages + cargo check -p kizunashelf
pnpm lint             # oxlint (web)
pnpm contract:generate  # regenerate the OpenAPI spec + TS client (run after API changes)
pnpm i18n:extract       # re-extract the web UI-string catalogs (run after UI string changes)
pnpm docs:generate      # regenerate the manual's reference pages from the Rust source (run after schema/provider/preset changes)
pnpm docs:build         # build the manual (link/anchor check); also part of `pnpm build`
pnpm build
```

Before pushing, match what CI runs (`.forgejo/workflows/ci.yml`):

```bash
cargo fmt                                          # CI does `cargo fmt --check`
cargo clippy -p kizunashelf --all-targets -- -D warnings
cargo test -p kizunashelf
pnpm lint && pnpm typecheck
pnpm contract:generate   # then ensure git diff is clean (CI fails otherwise)
pnpm i18n:extract        # same deal — CI diffs apps/web/src/locales after extract
pnpm docs:generate       # same deal — CI diffs manual/content/reference after generating
pnpm build               # includes the manual's build, which fails on a broken doc link/anchor
```

## Conventions & gotchas

- **Match the surrounding style.** The core favors small total functions, `Option` over panics, and explicit `ApiError` constructors (`bad_request`, `conflict`, `forbidden`, `bad_gateway`, …). Don't `unwrap`/`panic` on the request path.
- **No direct filesystem access to the vault.** All vault file operations go through the `Vfs` trait — no `std::fs`/`tokio::fs` for vault content, or the iOS build breaks silently. Reach for `state.vault_vfs(...)`.
- **Writes go through the library, atomically.** Entity/asset writes use temp-file + rename (`write_entity_raw`, `write_asset_file`); mutations are revision-guarded (409 on a stale revision, re-checked against freshly read content). Keep both properties when touching write paths.
- **NFC/NFD normalization.** Apple filesystems hand back NFD-decomposed names; wikilink/relation matching normalizes to NFC at the chokepoints. Don't compare raw filename bytes.
- **Asset downloads** validate URLs against an SSRF guard (private/loopback/link-local blocked, `198.18.0.0/15` allowlisted, redirects re-validated per hop); the opt-in `KIZUNASHELF_ALLOW_PRIVATE_ASSET_HOSTS` relaxes it for trusted LAN hosts.

## Further reading

- `manual/content/reference/` — the full schema/config reference (types, fields, roles, per-runtime config, provider credentials), split into per-topic pages with `config.md` as the entry/overview page. The authority for what the schema means. It lives in the user manual (`manual/`, a Docusaurus site — see `manual/README.md`).
- `../kizunashelf-ios/ARCHITECTURE.md` — the iOS app's architecture and the regenerate-bindings workflow.
- `README.md` — product overview and quick start.
