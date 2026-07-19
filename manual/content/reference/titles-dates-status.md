+++
title = "Titles, dates & status"
description = "Title resolution and the language preference, date roles and seasons, and the canonical status model."
weight = 3
+++

## Title Design

Titles are intentionally more structured than ordinary text fields.

KizunaShelf exposes:

| Entity property | Purpose |
| --- | --- |
| `entity.title` | Language-agnostic fallback title, used in search and wherever no viewer language applies. |
| `entity.titles` | Title map keyed by language, used for language switching, subtitle display, and search. |

There is no `defaultTitle` flag. The **displayed** title is chosen by the viewer's language (see [Language preference](#language-preference) below). Each surface resolves a title as:

1. The viewer language's title — `entity.titles[language]`
2. Otherwise `entity.title` (the language-agnostic fallback below)

The core computes `entity.title` as the fallback, in this order:

1. The `titleRole: original` title field, when present
2. Otherwise the first configured `fieldType: title` field
3. Otherwise the first value in `entity.titles` (e.g. a `filename.titleLanguage` basename)
4. Otherwise the filename basename

So the effective resolution is **selected language → original → other titles**.

### Language preference

Each client holds **one per-device language preference** (not in the vault config — it's a viewer choice, so it lives in web `localStorage` / iOS `UserDefaults`, sourced per runtime). Everything language-sensitive derives from that single value:

- **UI language** — the app chrome is translated into English, Japanese, Simplified Chinese, and Traditional Chinese. A preference outside that set falls back to the **English UI** (titles still follow the preference). Which languages a client's UI ships is a per-client fact, not something the core reports — `GET /api/languages` returns the selectable options (`userLanguages`: `code`, endonym `label`, `titleLanguage`), and each client layers "is my UI translated into this" on top.
- **Title/content language** — the preference's bare primary subtag (`zh-Hans` → `zh`) keys `entity.titles[language]`. **Title languages and the schema's `titleLanguage` are always bare ISO codes** — script subtags never enter `titles` maps, `titleLanguage` config, or the dedup index.
- **Provider request language** — the raw preference (which *may* carry a script subtag) is sent to external providers on search / quick-add / episode fetch, so a provider that distinguishes Simplified vs Traditional (TMDB, Steam) can localize; the core normalizes the subtag per provider (TheTVDB collapses to one Chinese bucket, TMDB maps `zh-Hant` → `zh-TW`, and the Apple providers map every preference to its iTunes storefront — `ja` → `jp`, `zh-Hans` → `cn`, `zh-Hant` → `tw` — since the storefront picks both result relevance and metadata language).

**Simplified vs Traditional Chinese.** The picker offers `zh-Hans` (简体中文) and `zh-Hant` (繁體中文) as distinct UI languages, but both map to the single title language `zh`. A vault therefore has **one `zh` title bucket**, not two: whichever script a provider returned (or the user typed) is what's stored, and every Chinese viewer sees that stored script. This is a deliberate simplicity trade-off — keeping two Chinese title fields per type would burden every user to avoid an occasional script mismatch. (A future Simplified↔Traditional fold at search/dedup time could soften it; it is out of scope today.)

Server responses (`ApiError` messages, etc.) are **not** localized — clients surface them in English. Only the schema-derived, data-driven labels (type/field names, headings) and the client UI strings are translated.

**Type presets are the deliberate exception.** The built-in presets *seed* the schema — type labels, folder paths, field display names, status values, home shelf titles, daily-note hashtags — and that text becomes user data the user reads forever, so it must arrive in their language. The preset registry ships every string in `en`/`ja`/`zh-Hans`/`zh-Hant`, and the picker offers **one language choice** (a user-language preference code, defaulting to the app's preference) that drives both derivations: its bare primary subtag is stamped as the title/filename/season language, and it selects the seeded text. A language the presets aren't written in still stamps its titles — `ko` gets Korean titles with English labels (the picker says so explicitly). Script subtags affect text glyphs only: a `zh-Hant` choice seeds 繁體 labels while the stamped `titleLanguage` and every stored key remain bare `zh`. The choice also picks the **provider wiring** where sources are language-bound: Bangumi (Chinese `name_cn`, Japanese-original `name`, Chinese summaries) leads the search priority and feeds titles for Chinese vaults, feeds Japanese originals for Japanese vaults, and drops to last (covers and links only) everywhere else, where TMDB/TheTVDB/MAL localize per request instead. `GET /api/type-presets` takes the same `language` to localize the picker metadata.

All configured title fields are title data:

```yaml
fields:
- field: title
  fieldType: title
  titleLanguage: zh
- field: title_en
  fieldType: title
  titleLanguage: en
- field: title_original
  fieldType: title
  displayName: Title (Original)
  titleRole: original
```

Title keys are chosen like this:

| Field config | `entity.titles` key |
| --- | --- |
| `titleLanguage: en` | `en` |
| no `titleLanguage` | raw field key, for example `title_original` |
| `filename.titleLanguage: zh` | `zh` |

This means a `titleRole: original` field is still a title even if it is not language-specific. It appears in `entity.titles` under its field key and the UI can label it using `displayName`.

## Dates and Calendar Design

Dates are not just display fields. They drive:

- date badges in entity cards
- the entity detail Dates panel
- calendar views
- planning and completion views
- timeline analytics

Use `dateRole` to tell KizunaShelf what kind of date a field represents:

```yaml
- field: release_date
  fieldType: date
  displayName: Release date
  dateRole: planning

- field: complete_date
  fieldType: date
  displayName: Completed date
  dateRole: completed
```

The roles — `planning`, `started`, `completed`, and `event` (a date you *attend* rather than a release you consume; whether it reads as upcoming or attended is derived from the entity's [status](#status)) — are enumerated with their exact meanings in [Date roles](@/reference/field-types.md#date-roles-daterole).

`fieldType: season` can also use `dateRole`. It is useful when a collection uses seasons instead of exact dates.

```yaml
- field: season
  fieldType: season
  displayName: Season
  dateRole: planning
  seasonLanguage: zh
```

`seasonLanguage` selects the season label language — `zh`, `ja`, or `en` (see [Season languages](@/reference/field-types.md#season-languages-seasonlanguage)).

Exact dates such as `2025-04-20` are normalized for calendar links. Broader values such as seasons and years are still useful for planning/timeline views.

## Status

An `enum` field can be marked as the type's **lifecycle status** with `enumRole: status`. Like `dateRole`, this is a *role* — the meaning comes from the role, never the field name, so the field and its options can be named anything (`状态`, `state`, `進捗`…). A type may declare at most one status field.

On its own, `enumRole: status` just tells KizunaShelf "this is the status field" (used for a badge and a filter facet); every option you define stays selectable and nothing else changes. To let the engine *reason* about status — flip it when you log, keep planned items from nagging after you finish, hide a dropped show's remaining episodes — add a `statusValues` mapping from each **canonical** status to the option strings that mean it:

```yaml
- field: 状态
  fieldType: enum
  enumRole: status
  enumOptions: [想看, 在看, 搁置, 看完, 抛弃]
  statusValues:
    planning:  [想看]
    ongoing:   [在看]
    paused:    [搁置]
    completed: [看完, 刷过]   # both mean completed; 看完 is written when a log sets "completed"
    dropped:   [抛弃]
```

The five canonicals and their exact meanings are enumerated in [Canonical statuses](@/reference/field-types.md#canonical-statuses-statusvalues) — in short: `planning` (intend to), `ongoing`, `paused` (deferred, a log resumes it), `completed`, and `dropped` (abandoned, a log never auto-resumes it; past records still appear in the activity feed).

Notes:

- **The first option listed for a canonical is the write target** — what a log flip writes when it sets that status (e.g. logging a "completed" event writes `看完`, the first `completed` option).
- A canonical you don't map (or omitting `statusValues` entirely) simply leaves that canonical unmapped — no behavior fires for it.
- A frontmatter value that isn't in any list is preserved and still shown; it just has no canonical meaning. Hand-edited and legacy values are never dropped.
