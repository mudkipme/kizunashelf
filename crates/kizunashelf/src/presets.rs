//! Built-in **type presets** — the "add a built-in type" registry that powers
//! onboarding and the vault-settings type picker.
//!
//! This replaces the old whole-vault starter templates entirely. A preset is one
//! fully-wired
//! [`EntityTypeConfig`] — external refs, field/body mappings, status roles,
//! progress — plus a little picker metadata. The registry here is the single
//! source of truth for every frontend (web onboarding, the web/desktop/iOS type
//! picker); each reaches it through `GET /api/type-presets` and
//! `POST /api/type-presets/resolve`.
//!
//! Two things are deliberately *not* baked into a preset's config, because they
//! depend on the user's choices at add-time and are applied by
//! [`resolve_presets`]:
//!
//! - **Language.** Presets are language-neutral, and the request's single
//!   `language` choice drives two derivations at resolve time. Its bare primary
//!   subtag is stamped onto title fields, `filename`, and `seasonLanguage`. And
//!   it picks the language of all seeded *text* — type labels, folder names,
//!   field display names, status values, shelf titles, descriptions — which
//!   ships in the registry in every language the presets are written in
//!   ([`L`]). A language the presets aren't written in still stamps its titles
//!   but gets English text (`ko`: Korean titles, English labels). Presets are
//!   the deliberate exception to "server strings are English": they seed user
//!   data a user will read in their vault forever, so the text must arrive in
//!   their language. Stored language *keys* are unaffected — a `zh-Hant`
//!   choice stamps bare `zh` and picks Traditional glyphs for the text.
//! - **Relations.** A preset carries its relation fields, but a relation only
//!   *survives* when its target type is present — see the module's resolve rules.
//!
//! External-field wiring is **literal**: each preset spells out its
//! `(provider, provider field)` pairs at the declaration — there is no shared
//! role table to cross-reference, and deliberately no cleverness; what you read
//! is what gets seeded. A registry-integrity test validates every referenced
//! provider id *and* mapped field id against the provider catalog that backs
//! `/api/external/providers`, so a preset can never silently drift from the
//! providers it references.

use crate::api::external::provider_catalog_items;
use crate::contract::{
    ResolveTypePresetsRequest, ResolveTypePresetsResponse, SmartCompareOp, SmartFilterConjunction,
    SmartFilterGroup, SmartFilterRule, SmartFilterRuleKind, SmartFilterSubgroup,
    TypePresetBackfill, TypePresetCategory, TypePresetCategoryInfo, TypePresetCollision,
    TypePresetProvider, TypePresetSummary, TypePresetsResponse,
};
use crate::languages::primary_language;
use crate::types::{
    BodySection, BodySectionKind, CanonicalStatus, DateRole, EntityTypeConfig, EnumRole,
    EpisodeTracking, ExternalFieldMapping, FieldConfig, FieldType, FilenameConfig,
    HomeSectionConfig, SeasonLanguage, SortDirection, StatusValues, TitleRole, TypeLogConfig,
};
use std::collections::HashSet;

// --- Public API ----------------------------------------------------------------

/// The full preset catalog for the picker: per-preset metadata plus the category
/// list in display order, with all display text in the language `language`
/// prefers (English fallback). The concrete configs are produced by
/// [`resolve_presets`].
pub fn type_presets_response(language: Option<&str>) -> TypePresetsResponse {
    let ctx = BuildCtx::new(language);
    let presets = built_presets(&ctx);
    TypePresetsResponse {
        presets: presets.iter().map(|preset| preset.summary(&ctx)).collect(),
        categories: category_infos(ctx.locale),
    }
}

/// Materialize the selected presets into concrete types, merged against the
/// caller's current schema. Pure and stateless — onboarding passes an empty
/// `current_types`, the settings editor passes its in-progress types, and both
/// get the same relation wiring, back-fill proposals, and collision handling.
pub fn resolve_presets(request: &ResolveTypePresetsRequest) -> ResolveTypePresetsResponse {
    let ctx = BuildCtx::new(request.language.as_deref());
    let all = built_presets(&ctx);

    // The presets the user picked, in request order, de-duplicated, unknown ids
    // ignored. Keep the original preset id alongside the built config.
    let mut seen_ids = HashSet::new();
    let selected: Vec<&Preset> = request
        .preset_ids
        .iter()
        .filter(|id| seen_ids.insert(id.as_str()))
        .filter_map(|id| all.iter().find(|preset| preset.config.id == *id))
        .collect();

    // Names already taken by the current schema (case-insensitive for paths).
    let mut taken_ids: HashSet<String> =
        request.current_types.iter().map(|t| t.id.clone()).collect();
    let mut taken_paths: HashSet<String> = request
        .current_types
        .iter()
        .map(|t| t.path.to_lowercase())
        .collect();

    // 1. Assign a (possibly suffixed) id/path to each selected preset, recording
    //    the original→assigned id map so co-selected relations can be repointed.
    let mut collisions = Vec::new();
    let mut assigned_id_of: Vec<(String, String)> = Vec::new(); // (original id, assigned id)
    let mut plan: Vec<(&Preset, String, String)> = Vec::new(); // (preset, id, path)
    for preset in &selected {
        let original_id = preset.config.id.clone();
        let original_path = preset.config.path.clone();
        let id = unique_key(&original_id, &taken_ids, |base, n| format!("{base}-{n}"));
        let path = unique_key(&original_path, &taken_paths, |base, n| {
            format!("{base} {n}")
        });
        taken_ids.insert(id.clone());
        taken_paths.insert(path.to_lowercase());
        if id != original_id || path != original_path {
            collisions.push(TypePresetCollision {
                preset_id: original_id.clone(),
                requested_id: original_id.clone(),
                assigned_id: id.clone(),
                requested_path: original_path.clone(),
                assigned_path: path.clone(),
            });
        }
        assigned_id_of.push((original_id, id.clone()));
        plan.push((preset, id, path));
    }
    let assigned = |original: &str| -> Option<&str> {
        assigned_id_of
            .iter()
            .find(|(orig, _)| orig == original)
            .map(|(_, id)| id.as_str())
    };
    // A relation target resolves if it points at an existing type, or at a preset
    // being added in this same batch (then repoint to that preset's assigned id).
    let existing_ids: HashSet<&str> = request
        .current_types
        .iter()
        .map(|t| t.id.as_str())
        .collect();

    // 2. Build the new types: set the assigned id/path, stamp language, and keep
    //    only relation fields whose target is present (existing or co-selected).
    let mut types = Vec::new();
    let mut home_sections = Vec::new();
    for (preset, id, path) in &plan {
        let mut config = preset.config.clone();
        config.id = id.clone();
        config.path = path.clone();
        config.fields.retain_mut(|field| {
            if field.field_type != FieldType::Relation {
                return true;
            }
            let Some(target) = field.relation_type.as_deref() else {
                return true;
            };
            if existing_ids.contains(target) {
                // Tie-break: an *existing* type with the target id wins over a
                // co-selected preset (which would be suffixed past it) — linking
                // into what the vault already has is what "add anime to a vault
                // with franchises" means. The co-selected copy is still added,
                // under its suffixed id, just not linked from here.
                return true;
            }
            if let Some(assigned_id) = assigned(target) {
                field.relation_type = Some(assigned_id.to_string()); // co-selected
                return true;
            }
            false // target absent → drop the relation field
        });
        // One default shelf per type, so a multi-type vault's home doesn't
        // start out bloated: the "in progress" shelf when the type's status
        // role maps an ongoing value, else the chronological "recent" shelf.
        if let Some(section) = ongoing_home_section_for(&config, preset.ongoing_shelf)
            .or_else(|| recent_home_section_for(&config, ctx.locale))
        {
            home_sections.push(section);
        }
        types.push(config);
    }

    // 3. Back-fills: an *existing* type whose id matches a preset gains a link to a
    //    newly added type when its origin preset declares that relation and the
    //    live copy is missing the field. Recompute from the registry each time, so
    //    no provenance needs to be stored. Two accepted trade-offs of that: a type
    //    that was *suffixed* at add time (`anime-2`) no longer matches its origin
    //    preset and never receives proposals, and "already has the field" is
    //    checked by field name alone.
    let mut backfills = Vec::new();
    for current in &request.current_types {
        let Some(origin) = all.iter().find(|preset| preset.config.id == current.id) else {
            continue;
        };
        for field in &origin.config.fields {
            if field.field_type != FieldType::Relation {
                continue;
            }
            let Some(target) = field.relation_type.as_deref() else {
                continue;
            };
            // Only targets added in *this* batch, repointed to their assigned id.
            let Some(assigned_id) = assigned(target) else {
                continue;
            };
            if current.fields.iter().any(|f| f.field == field.field) {
                continue; // the live type already has this relation field
            }
            let mut backfill_field = field.clone();
            backfill_field.relation_type = Some(assigned_id.to_string());
            let target_preset = all.iter().find(|preset| preset.config.id == target);
            backfills.push(TypePresetBackfill {
                type_id: current.id.clone(),
                type_label: current.label.clone(),
                preset_id: target.to_string(),
                preset_label: target_preset
                    .map(|preset| preset.config.label.clone())
                    .unwrap_or_else(|| target.to_string()),
                field: backfill_field,
            });
        }
    }

    ResolveTypePresetsResponse {
        types,
        backfills,
        home_sections,
        collisions,
    }
}

// --- Seed text localization ------------------------------------------------------
//
// Presets seed *user data* — labels, folder names, status values a user reads in
// their vault forever — so unlike ordinary server strings the text must arrive in
// the user's language. The registry carries every string inline in the four
// languages the app ships (`en`/`ja`/`zh-Hans`/`zh-Hant`); translations live at
// each declaration, never in an id-keyed side table that could drift.

/// The languages the seed text is written in. Note this is about *text*, not
/// language keys: picking [`SeedLocale::ZhHant`] chooses Traditional glyphs for
/// labels while `titleLanguage` and every stored key stay bare `zh`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SeedLocale {
    En,
    Ja,
    ZhHans,
    ZhHant,
}

impl SeedLocale {
    /// From the user's raw language preference (`ja`, `zh-Hant`, `en-US`).
    /// Unlike title languages the script subtag matters here — it picks the
    /// glyph set of the seeded text — so this must see the raw preference, not
    /// the bare primary subtag. Preferences the seeds aren't written in fall
    /// back to English; absent means English (old clients that don't send a
    /// preference keep their previous behavior).
    fn from_preference(code: Option<&str>) -> Self {
        let Some(code) = code else {
            return SeedLocale::En;
        };
        match primary_language(code).as_str() {
            "ja" => SeedLocale::Ja,
            "zh" => {
                let lower = code.to_ascii_lowercase();
                let traditional = lower
                    .split(['-', '_'])
                    .any(|part| matches!(part, "hant" | "tw" | "hk" | "mo"));
                if traditional {
                    SeedLocale::ZhHant
                } else {
                    SeedLocale::ZhHans
                }
            }
            _ => SeedLocale::En,
        }
    }
}

/// One piece of seed text in every supported language.
#[derive(Clone, Copy)]
struct L {
    en: &'static str,
    ja: &'static str,
    hans: &'static str,
    hant: &'static str,
}

/// Shorthand constructor so registry entries read as one line per string:
/// `l("Anime", "アニメ", "动画", "動畫")`.
const fn l(en: &'static str, ja: &'static str, hans: &'static str, hant: &'static str) -> L {
    L { en, ja, hans, hant }
}

impl L {
    fn get(self, locale: SeedLocale) -> &'static str {
        match locale {
            SeedLocale::En => self.en,
            SeedLocale::Ja => self.ja,
            SeedLocale::ZhHans => self.hans,
            SeedLocale::ZhHant => self.hant,
        }
    }
}

// --- The registry --------------------------------------------------------------

struct Preset {
    category: TypePresetCategory,
    /// Picker-card description, already in the build's text language.
    description: String,
    /// `"{label}"` template for the default in-progress shelf title, already in
    /// the build's text language. `None` when the status vocabulary maps no
    /// ongoing value (events, people).
    ongoing_shelf: Option<&'static str>,
    config: EntityTypeConfig,
}

impl Preset {
    fn summary(&self, ctx: &BuildCtx) -> TypePresetSummary {
        // Picker chips come straight from the config's provider priority — the
        // single list that wires the type — so the chips can't disagree with it.
        let providers = self
            .config
            .external_priority
            .iter()
            .filter_map(|id| {
                ctx.catalog
                    .iter()
                    .find(|item| item.id == *id)
                    .map(|item| TypePresetProvider {
                        id: item.id.clone(),
                        label: item.label.clone(),
                    })
            })
            .collect();
        let mut relation_targets = Vec::new();
        for field in &self.config.fields {
            if field.field_type == FieldType::Relation {
                if let Some(target) = &field.relation_type {
                    if !relation_targets.contains(target) {
                        relation_targets.push(target.clone());
                    }
                }
            }
        }
        TypePresetSummary {
            id: self.config.id.clone(),
            category: self.category,
            icon: self.config.icon.clone().unwrap_or_default(),
            label: self.config.label.clone(),
            description: self.description.clone(),
            providers,
            relation_targets,
        }
    }
}

fn category_infos(locale: SeedLocale) -> Vec<TypePresetCategoryInfo> {
    use TypePresetCategory::*;
    [
        (Watch, l("Watch", "観る", "看", "看")),
        (Play, l("Play", "遊ぶ", "玩", "玩")),
        (Read, l("Read", "読む", "读", "讀")),
        (Listen, l("Listen", "聴く", "听", "聽")),
        (
            People,
            l(
                "People & connections",
                "人・つながり",
                "人物与关联",
                "人物與關聯",
            ),
        ),
        (Life, l("Life", "ライフ", "生活", "生活")),
    ]
    .into_iter()
    .map(|(id, label)| TypePresetCategoryInfo {
        id,
        label: label.get(locale).to_string(),
    })
    .collect()
}

/// Build every preset for the given context (title language + text language).
/// The order here is the picker's within-category order. Each entry carries its
/// own category and translations — everything about a preset lives at its
/// declaration, never in an id-keyed side table.
fn built_presets(ctx: &BuildCtx) -> Vec<Preset> {
    vec![
        // --- Watch ---
        preset(
            ctx,
            TypeSpec {
                id: "anime",
                category: TypePresetCategory::Watch,
                label: l("Anime", "アニメ", "动画", "動畫"),
                icon: "📺",
                path: l("Anime", "アニメ", "动画", "動畫"),
                description: l(
                    "Track what you're watching and where you left off.",
                    "見ているアニメと視聴の進み具合を記録。",
                    "记录在看的动画和观看进度。",
                    "記錄在看的動畫與觀看進度。",
                ),
                providers: &["bangumi", "myanimelist", "tmdb", "thetvdb"],
                title_sources: &[
                    ("bangumi", "name_cn"),
                    ("myanimelist", "title"),
                    ("tmdb", "title"),
                    ("thetvdb", "name"),
                ],
                original_title: Some(&[
                    ("bangumi", "name"),
                    ("myanimelist", "title"),
                    ("tmdb", "original_title"),
                    ("thetvdb", "name"),
                ]),
                cover_sources: &[
                    ("bangumi", "cover_url"),
                    ("myanimelist", "cover_url"),
                    ("tmdb", "cover_url"),
                    ("thetvdb", "cover_url"),
                ],
                summary_sources: &[
                    ("bangumi", "summary"),
                    ("myanimelist", "synopsis"),
                    ("tmdb", "overview"),
                    ("thetvdb", "overview"),
                ],
                // Air dates coerce to a season; MAL has an explicit season label.
                date_sources: &[
                    ("bangumi", "date"),
                    ("myanimelist", "season"),
                    ("tmdb", "release_date"),
                    ("thetvdb", "first_air_time"),
                ],
                total_sources: &[
                    ("bangumi", "eps"),
                    ("myanimelist", "episodes"),
                    ("tmdb", "episode_count"),
                ],
                statuses: Some(&WATCH_STATUS),
                name_based: false,
                primary_date: PrimaryDate::Season,
                completed_date: true,
                list: Some(&EPISODES_LIST),
                extras: &[],
                relations: &[FRANCHISE_REL],
                log_hashtag: Some(l("Anime", "アニメ", "动画", "動畫")),
            },
        ),
        preset(
            ctx,
            TypeSpec {
                id: "drama",
                category: TypePresetCategory::Watch,
                label: l("TV & Drama", "ドラマ", "剧集", "影集"),
                icon: "🎭",
                path: l("Drama", "ドラマ", "剧集", "影集"),
                description: l(
                    "TV series and dramas, episode by episode.",
                    "ドラマやテレビシリーズを1話ずつ。",
                    "电视剧与剧集，一集一集地记录。",
                    "電視劇與影集，一集一集地記錄。",
                ),
                providers: &["tmdb", "thetvdb"],
                title_sources: &[("tmdb", "title"), ("thetvdb", "name")],
                original_title: Some(&[("tmdb", "original_title"), ("thetvdb", "name")]),
                cover_sources: &[("tmdb", "cover_url"), ("thetvdb", "cover_url")],
                summary_sources: &[("tmdb", "overview"), ("thetvdb", "overview")],
                date_sources: &[("tmdb", "release_date"), ("thetvdb", "first_air_time")],
                total_sources: &[("tmdb", "episode_count")],
                statuses: Some(&WATCH_STATUS),
                name_based: false,
                primary_date: PrimaryDate::Season,
                completed_date: true,
                list: Some(&EPISODES_LIST),
                extras: &[],
                relations: &[FRANCHISE_REL],
                log_hashtag: Some(l("Drama", "ドラマ", "剧集", "影集")),
            },
        ),
        preset(
            ctx,
            TypeSpec {
                id: "movie",
                category: TypePresetCategory::Watch,
                label: l("Movies", "映画", "电影", "電影"),
                icon: "🎬",
                path: l("Movie", "映画", "电影", "電影"),
                description: l(
                    "Films you've seen or want to see.",
                    "見た映画も、見たい映画も。",
                    "看过的电影，和想看的电影。",
                    "看過的電影，和想看的電影。",
                ),
                providers: &["tmdb", "bangumi", "thetvdb"],
                title_sources: &[
                    ("tmdb", "title"),
                    ("bangumi", "name_cn"),
                    ("thetvdb", "name"),
                ],
                original_title: Some(&[
                    ("tmdb", "original_title"),
                    ("bangumi", "name"),
                    ("thetvdb", "name"),
                ]),
                cover_sources: &[
                    ("tmdb", "cover_url"),
                    ("bangumi", "cover_url"),
                    ("thetvdb", "cover_url"),
                ],
                summary_sources: &[
                    ("tmdb", "overview"),
                    ("bangumi", "summary"),
                    ("thetvdb", "overview"),
                ],
                date_sources: &[
                    ("tmdb", "release_date"),
                    ("bangumi", "date"),
                    ("thetvdb", "first_air_time"),
                ],
                total_sources: &[],
                statuses: Some(&WATCH_STATUS),
                name_based: false,
                primary_date: PrimaryDate::ReleaseDate,
                completed_date: true,
                list: None,
                extras: &[],
                relations: &[FRANCHISE_REL],
                log_hashtag: Some(l("Movie", "映画", "电影", "電影")),
            },
        ),
        // --- Play ---
        preset(
            ctx,
            TypeSpec {
                id: "games",
                category: TypePresetCategory::Play,
                label: l("Games", "ゲーム", "游戏", "遊戲"),
                icon: "🎮",
                path: l("Games", "ゲーム", "游戏", "遊戲"),
                description: l(
                    "Your backlog, what you're playing, and what you've beaten.",
                    "積みゲーも、プレイ中も、クリア済みも。",
                    "愿望单、在玩和已通关的游戏。",
                    "願望清單、遊玩中和已全破的遊戲。",
                ),
                providers: &["igdb", "steam", "bangumi"],
                title_sources: &[("igdb", "name"), ("steam", "name"), ("bangumi", "name_cn")],
                original_title: None,
                cover_sources: &[
                    ("igdb", "cover_url"),
                    ("steam", "cover_url"),
                    ("bangumi", "cover_url"),
                ],
                summary_sources: &[
                    ("igdb", "summary"),
                    ("steam", "description"),
                    ("bangumi", "summary"),
                ],
                date_sources: &[
                    ("igdb", "first_release_date"),
                    ("steam", "release_date"),
                    ("bangumi", "date"),
                ],
                total_sources: &[],
                statuses: Some(&PLAY_STATUS),
                name_based: false,
                primary_date: PrimaryDate::ReleaseDate,
                completed_date: true,
                list: None,
                extras: &[
                    Extra::Platform(&[("igdb", "platforms"), ("steam", "platform")]),
                    Extra::Genre(&[
                        ("igdb", "genres"),
                        ("steam", "genres"),
                        ("bangumi", "genre"),
                    ]),
                ],
                relations: &[FRANCHISE_REL],
                log_hashtag: Some(l("Game", "ゲーム", "游戏", "遊戲")),
            },
        ),
        preset(
            ctx,
            TypeSpec {
                id: "board",
                category: TypePresetCategory::Play,
                label: l("Board Games", "ボードゲーム", "桌游", "桌遊"),
                icon: "🎲",
                path: l("Board Games", "ボードゲーム", "桌游", "桌遊"),
                description: l(
                    "Board games and tabletop, with player count and playtime.",
                    "ボードゲーム・テーブルゲームを人数やプレイ時間とともに。",
                    "桌游与桌面游戏，带玩家人数和时长。",
                    "桌遊與桌上遊戲，帶玩家人數和時長。",
                ),
                providers: &["bgg"],
                title_sources: &[("bgg", "name")],
                original_title: None,
                cover_sources: &[("bgg", "cover_url")],
                summary_sources: &[("bgg", "description")],
                // BGG exposes only a year, which the release-date field can't hold.
                date_sources: &[],
                total_sources: &[],
                statuses: Some(&PLAY_STATUS),
                name_based: false,
                primary_date: PrimaryDate::ReleaseDate,
                completed_date: false,
                list: None,
                extras: &[Extra::Players, Extra::Playtime],
                relations: &[FRANCHISE_REL],
                log_hashtag: Some(l("BoardGame", "ボードゲーム", "桌游", "桌遊")),
            },
        ),
        // --- Read ---
        preset(
            ctx,
            TypeSpec {
                id: "books",
                category: TypePresetCategory::Read,
                label: l("Books", "本", "书籍", "書籍"),
                icon: "📚",
                path: l("Books", "本", "书籍", "書籍"),
                description: l(
                    "Books you're reading, with authors and ISBNs.",
                    "読んでいる本を、著者やISBNとともに。",
                    "在读的书，带作者和 ISBN。",
                    "在讀的書，帶作者和 ISBN。",
                ),
                providers: &["googlebooks", "openlibrary", "hardcover", "bangumi"],
                title_sources: &[
                    ("googlebooks", "title"),
                    ("openlibrary", "title"),
                    ("hardcover", "title"),
                    ("bangumi", "name_cn"),
                ],
                original_title: Some(&[
                    ("googlebooks", "title"),
                    ("openlibrary", "title"),
                    ("hardcover", "title"),
                    ("bangumi", "name"),
                ]),
                cover_sources: &[
                    ("googlebooks", "cover_url"),
                    ("openlibrary", "cover_url"),
                    ("hardcover", "cover_url"),
                    ("bangumi", "cover_url"),
                ],
                summary_sources: &[
                    ("googlebooks", "description"),
                    ("openlibrary", "description"),
                    ("hardcover", "synopsis"),
                    ("bangumi", "summary"),
                ],
                date_sources: &[
                    ("googlebooks", "published_date"),
                    ("openlibrary", "published_date"),
                    ("hardcover", "publish_date"),
                    ("bangumi", "date"),
                ],
                total_sources: &[],
                statuses: Some(&READ_STATUS),
                name_based: false,
                primary_date: PrimaryDate::ReleaseDate,
                completed_date: true,
                list: None,
                extras: &[
                    Extra::Author(&[
                        ("googlebooks", "authors"),
                        ("openlibrary", "authors"),
                        ("hardcover", "authors"),
                        ("bangumi", "author"),
                    ]),
                    Extra::Isbn(&[
                        ("googlebooks", "isbn"),
                        ("openlibrary", "isbn"),
                        ("hardcover", "isbn"),
                        ("bangumi", "isbn"),
                    ]),
                ],
                relations: &[FRANCHISE_REL],
                log_hashtag: Some(l("Book", "読書", "读书", "讀書")),
            },
        ),
        preset(
            ctx,
            TypeSpec {
                id: "manga",
                category: TypePresetCategory::Read,
                label: l("Manga & Comics", "マンガ", "漫画", "漫畫"),
                icon: "📖",
                path: l("Manga", "マンガ", "漫画", "漫畫"),
                description: l(
                    "Manga and comics, tracked by chapter.",
                    "マンガ・コミックを話数で管理。",
                    "漫画，按话数记录。",
                    "漫畫，按話數記錄。",
                ),
                providers: &["bangumi", "mangaupdates", "myanimelist", "comicvine"],
                title_sources: &[
                    ("bangumi", "name_cn"),
                    ("mangaupdates", "title"),
                    ("myanimelist", "title"),
                    ("comicvine", "title"),
                ],
                original_title: Some(&[
                    ("bangumi", "name"),
                    ("mangaupdates", "title"),
                    ("myanimelist", "title"),
                    ("comicvine", "title"),
                ]),
                cover_sources: &[
                    ("bangumi", "cover_url"),
                    ("mangaupdates", "cover_url"),
                    ("myanimelist", "cover_url"),
                    ("comicvine", "cover_url"),
                ],
                summary_sources: &[
                    ("bangumi", "summary"),
                    ("mangaupdates", "synopsis"),
                    ("myanimelist", "synopsis"),
                    ("comicvine", "description"),
                ],
                date_sources: &[("bangumi", "date"), ("myanimelist", "start_date")],
                // Bangumi counts manga chapters in its `eps` field.
                total_sources: &[
                    ("bangumi", "eps"),
                    ("mangaupdates", "latest_chapter"),
                    ("myanimelist", "chapters"),
                ],
                statuses: Some(&READ_STATUS),
                name_based: false,
                primary_date: PrimaryDate::ReleaseDate,
                completed_date: true,
                list: Some(&CHAPTERS_LIST),
                extras: &[Extra::Author(&[
                    ("bangumi", "author"),
                    ("mangaupdates", "authors"),
                ])],
                relations: &[FRANCHISE_REL],
                log_hashtag: Some(l("Manga", "マンガ", "漫画", "漫畫")),
            },
        ),
        // --- Listen ---
        preset(
            ctx,
            TypeSpec {
                id: "music",
                category: TypePresetCategory::Listen,
                label: l("Music Albums", "音楽アルバム", "音乐专辑", "音樂專輯"),
                icon: "💿",
                path: l("Music", "音楽", "音乐", "音樂"),
                description: l(
                    "Albums and CDs — the music you own and love.",
                    "持っているアルバムやCD、大切な音楽のコレクション。",
                    "专辑与 CD——你拥有和喜爱的音乐。",
                    "專輯與 CD——你擁有和喜愛的音樂。",
                ),
                providers: &["musicbrainz", "applemusic", "discogs", "bangumi"],
                title_sources: &[
                    ("musicbrainz", "title"),
                    ("applemusic", "title"),
                    ("discogs", "title"),
                    ("bangumi", "name_cn"),
                ],
                original_title: None,
                cover_sources: &[
                    ("musicbrainz", "cover_url"),
                    ("applemusic", "cover_url"),
                    ("discogs", "cover_url"),
                    ("bangumi", "cover_url"),
                ],
                summary_sources: &[("bangumi", "summary")],
                date_sources: &[
                    ("musicbrainz", "release_date"),
                    ("applemusic", "release_date"),
                    ("bangumi", "date"),
                ],
                total_sources: &[],
                statuses: Some(&LISTEN_STATUS),
                name_based: false,
                primary_date: PrimaryDate::ReleaseDate,
                completed_date: false,
                list: Some(&TRACKS_LIST),
                extras: &[Extra::OwnedFormats],
                relations: &[ARTIST_REL, FRANCHISE_REL],
                log_hashtag: Some(l("Music", "音楽", "音乐", "音樂")),
            },
        ),
        preset(
            ctx,
            TypeSpec {
                id: "podcast",
                category: TypePresetCategory::Listen,
                label: l("Podcasts", "ポッドキャスト", "播客", "Podcast"),
                icon: "🎙️",
                path: l("Podcasts", "ポッドキャスト", "播客", "Podcast"),
                description: l(
                    "Podcasts you follow.",
                    "フォロー中のポッドキャスト。",
                    "关注的播客。",
                    "追蹤中的 Podcast。",
                ),
                providers: &["applepodcast"],
                title_sources: &[("applepodcast", "title")],
                original_title: None,
                cover_sources: &[("applepodcast", "cover_url")],
                summary_sources: &[],
                date_sources: &[],
                total_sources: &[],
                statuses: Some(&LISTEN_STATUS),
                name_based: false,
                primary_date: PrimaryDate::None,
                completed_date: false,
                list: Some(&PODCAST_EPISODES_LIST),
                extras: &[],
                relations: &[],
                log_hashtag: Some(l("Podcast", "ポッドキャスト", "播客", "Podcast")),
            },
        ),
        // --- People & connections ---
        preset(
            ctx,
            TypeSpec {
                id: "artist",
                category: TypePresetCategory::People,
                label: l(
                    "Artists & Creators",
                    "アーティスト・クリエイター",
                    "艺术家与创作者",
                    "藝術家與創作者",
                ),
                icon: "🎤",
                path: l("Artist", "アーティスト", "艺术家", "藝術家"),
                description: l(
                    "Artists, authors, studios — the people behind your library.",
                    "アーティスト・作家・スタジオなど、ライブラリの作り手。",
                    "艺术家、作者、工作室——藏品背后的创作者。",
                    "藝術家、作者、工作室——收藏背後的創作者。",
                ),
                providers: &["bangumi", "musicbrainz"],
                title_sources: &[("bangumi", "name_cn"), ("musicbrainz", "title")],
                original_title: None,
                cover_sources: &[("bangumi", "cover_url"), ("musicbrainz", "cover_url")],
                summary_sources: &[("bangumi", "summary")],
                date_sources: &[],
                total_sources: &[],
                statuses: None,
                name_based: true,
                primary_date: PrimaryDate::None,
                completed_date: false,
                list: None,
                extras: &[Extra::Birthday(&[("bangumi", "birthday")])],
                relations: &[FRANCHISE_REL, GROUPS_REL],
                log_hashtag: None,
            },
        ),
        preset(
            ctx,
            TypeSpec {
                id: "franchise",
                category: TypePresetCategory::People,
                label: l("Franchises", "シリーズ", "系列", "系列"),
                icon: "🧩",
                path: l("Franchise", "シリーズ", "系列", "系列"),
                description: l(
                    "Group related entries — a series, saga, or shared universe.",
                    "シリーズや同じ世界観の作品をひとまとめに。",
                    "把相关条目归为系列或同一世界观。",
                    "把相關條目歸為系列或同一世界觀。",
                ),
                providers: &[],
                title_sources: &[],
                original_title: None,
                cover_sources: &[],
                summary_sources: &[],
                date_sources: &[],
                total_sources: &[],
                statuses: None,
                name_based: true,
                primary_date: PrimaryDate::None,
                completed_date: false,
                list: None,
                extras: &[],
                relations: &[],
                log_hashtag: None,
            },
        ),
        preset(
            ctx,
            TypeSpec {
                id: "character",
                category: TypePresetCategory::People,
                label: l("Characters", "キャラクター", "角色", "角色"),
                icon: "👤",
                path: l("Characters", "キャラクター", "角色", "角色"),
                description: l(
                    "Characters, with who voices or portrays them.",
                    "キャラクターと、演じる声優・キャスト。",
                    "角色，以及配音或饰演的人。",
                    "角色，以及配音或飾演的人。",
                ),
                providers: &["bangumi"],
                title_sources: &[("bangumi", "name_cn")],
                original_title: None,
                cover_sources: &[("bangumi", "cover_url")],
                summary_sources: &[("bangumi", "summary")],
                date_sources: &[],
                total_sources: &[],
                statuses: None,
                name_based: true,
                primary_date: PrimaryDate::None,
                completed_date: false,
                list: None,
                extras: &[Extra::Birthday(&[("bangumi", "birthday")])],
                relations: &[FRANCHISE_REL, VOICE_BY_REL],
                log_hashtag: None,
            },
        ),
        // --- Life ---
        preset(
            ctx,
            TypeSpec {
                id: "event",
                category: TypePresetCategory::Life,
                label: l("Events", "イベント", "活动", "活動"),
                icon: "🎫",
                path: l("Event", "イベント", "活动", "活動"),
                description: l(
                    "Concerts, exhibitions, and events you attend.",
                    "ライブ・展示・イベントの参加記録。",
                    "参加的演出、展览与活动。",
                    "參加的演出、展覽與活動。",
                ),
                providers: &[],
                title_sources: &[],
                original_title: None,
                cover_sources: &[],
                summary_sources: &[],
                date_sources: &[],
                total_sources: &[],
                statuses: Some(&EVENT_STATUS),
                name_based: false,
                primary_date: PrimaryDate::EventDate,
                completed_date: false,
                list: None,
                extras: &[Extra::Location],
                relations: &[ARTIST_REL, FRANCHISE_REL],
                log_hashtag: Some(l("Event", "イベント", "活动", "活動")),
            },
        ),
    ]
}

/// A relation field a preset seeds: field name (never localized), display label,
/// target preset id.
struct RelationSpec {
    field: &'static str,
    label: L,
    target: &'static str,
}

const FRANCHISE_REL: RelationSpec = RelationSpec {
    field: "franchise",
    label: l("Franchise", "シリーズ", "系列", "系列"),
    target: "franchise",
};
const ARTIST_REL: RelationSpec = RelationSpec {
    field: "artist",
    label: l("Artist", "アーティスト", "艺术家", "藝術家"),
    target: "artist",
};
const GROUPS_REL: RelationSpec = RelationSpec {
    field: "groups",
    label: l("Groups", "グループ", "团体", "團體"),
    target: "artist",
};
const VOICE_BY_REL: RelationSpec = RelationSpec {
    field: "voice_by",
    label: l("Voice by", "CV", "配音", "配音"),
    target: "artist",
};

/// A status vocabulary: the enum options (with their canonical-status mapping)
/// plus the title template for the default in-progress home shelf.
struct StatusVocab {
    /// `(canonical, label)` — the first label for a canonical is the write
    /// target for a log flip.
    options: &'static [(CanonicalStatus, L)],
    /// `"{label}"` template for the "in progress" shelf title. Defined *with*
    /// the vocabulary (rather than composed from the ongoing label at runtime)
    /// because word order and particles differ per language — English prepends a
    /// verb, Japanese needs a relative clause, Chinese inserts 的. Baking the
    /// verb here duplicates the ongoing label on purpose: the two are one
    /// vocabulary, declared side by side so they can't drift. `None` when
    /// nothing maps ongoing (events) — no such shelf then.
    ongoing_shelf: Option<L>,
}

const WATCH_STATUS: StatusVocab = StatusVocab {
    options: &[
        (
            CanonicalStatus::Planning,
            l("Wishlist", "見たい", "想看", "想看"),
        ),
        (
            CanonicalStatus::Ongoing,
            l("Watching", "見てる", "在看", "在看"),
        ),
        (
            CanonicalStatus::Completed,
            l("Watched", "見た", "看过", "看過"),
        ),
        (
            CanonicalStatus::Paused,
            l("Paused", "一時中断", "搁置", "擱置"),
        ),
        (
            CanonicalStatus::Dropped,
            l("Dropped", "中止", "抛弃", "放棄"),
        ),
    ],
    ongoing_shelf: Some(l(
        "Watching {label}",
        "見てる{label}",
        "在看的{label}",
        "在看的{label}",
    )),
};
const PLAY_STATUS: StatusVocab = StatusVocab {
    options: &[
        (
            CanonicalStatus::Planning,
            l("Backlog", "プレイしたい", "想玩", "想玩"),
        ),
        (
            CanonicalStatus::Ongoing,
            l("Playing", "プレイ中", "在玩", "在玩"),
        ),
        (
            CanonicalStatus::Completed,
            l("Completed", "プレイ済み", "玩过", "玩過"),
        ),
        (
            CanonicalStatus::Paused,
            l("Paused", "一時中断", "搁置", "擱置"),
        ),
        (
            CanonicalStatus::Dropped,
            l("Dropped", "中止", "抛弃", "放棄"),
        ),
    ],
    ongoing_shelf: Some(l(
        "Playing {label}",
        "プレイ中の{label}",
        "在玩的{label}",
        "在玩的{label}",
    )),
};
const READ_STATUS: StatusVocab = StatusVocab {
    options: &[
        (
            CanonicalStatus::Planning,
            l("Backlog", "読みたい", "想读", "想讀"),
        ),
        (
            CanonicalStatus::Ongoing,
            l("Reading", "読んでる", "在读", "在讀"),
        ),
        (
            CanonicalStatus::Completed,
            l("Finished", "読んだ", "读过", "讀過"),
        ),
        (
            CanonicalStatus::Paused,
            l("Paused", "一時中断", "搁置", "擱置"),
        ),
        (
            CanonicalStatus::Dropped,
            l("Dropped", "中止", "抛弃", "放棄"),
        ),
    ],
    ongoing_shelf: Some(l(
        "Reading {label}",
        "読んでる{label}",
        "在读的{label}",
        "在讀的{label}",
    )),
};
const LISTEN_STATUS: StatusVocab = StatusVocab {
    options: &[
        (
            CanonicalStatus::Planning,
            l("Wishlist", "聴きたい", "想听", "想聽"),
        ),
        (
            CanonicalStatus::Ongoing,
            l("Listening", "聴いてる", "在听", "在聽"),
        ),
        (
            CanonicalStatus::Completed,
            l("Listened", "聴いた", "听过", "聽過"),
        ),
        (
            CanonicalStatus::Paused,
            l("Paused", "一時中断", "搁置", "擱置"),
        ),
        (
            CanonicalStatus::Dropped,
            l("Dropped", "中止", "抛弃", "放棄"),
        ),
    ],
    ongoing_shelf: Some(l(
        "Listening to {label}",
        "聴いてる{label}",
        "在听的{label}",
        "在聽的{label}",
    )),
};
const EVENT_STATUS: StatusVocab = StatusVocab {
    options: &[
        (
            CanonicalStatus::Planning,
            l("Planned", "参加予定", "想去", "想去"),
        ),
        (
            CanonicalStatus::Completed,
            l("Attended", "参加済み", "去过", "去過"),
        ),
    ],
    ongoing_shelf: None,
};

/// The `"Recent {label}"` shelf title, for types with a date but no ongoing
/// status.
const RECENT_SHELF: L = l(
    "Recent {label}",
    "最近の{label}",
    "最近的{label}",
    "最近的{label}",
);

const EPISODES_LIST: ListSpec = ListSpec {
    heading: l("Episodes", "エピソード", "剧集", "劇集"),
    tracking: EpisodeTracking::Checklist,
    progress: Some(ProgressSpec {
        total_field: "episodes",
        total_label: l("Episodes", "話数", "总集数", "總集數"),
    }),
};
const CHAPTERS_LIST: ListSpec = ListSpec {
    heading: l("Chapters", "チャプター", "章节", "章節"),
    tracking: EpisodeTracking::Checklist,
    progress: Some(ProgressSpec {
        total_field: "chapters",
        total_label: l("Chapters", "話数", "话数", "話數"),
    }),
};
// Podcasts are open-ended (no reliable total to count against), so they get the
// episode checklist without the scalar progress pair.
const PODCAST_EPISODES_LIST: ListSpec = ListSpec {
    heading: l("Episodes", "エピソード", "单集", "單集"),
    tracking: EpisodeTracking::Checklist,
    progress: None,
};
// Albums are owned, not progressed through — a track list, no checklist and no
// "x of y" fields.
const TRACKS_LIST: ListSpec = ListSpec {
    heading: l("Tracks", "トラック", "曲目", "曲目"),
    tracking: EpisodeTracking::None,
    progress: None,
};

// --- The builder ---------------------------------------------------------------

struct BuildCtx {
    catalog: Vec<crate::contract::ExternalProviderCatalogItem>,
    /// Normalized ISO 639-1 title language; `en` when unset/unknown.
    lang: String,
    /// The language the seeded *text* is written in.
    locale: SeedLocale,
}

impl BuildCtx {
    /// One language choice, two derivations: the *stamped* title language is
    /// the bare primary subtag (a `zh-Hans`/`zh-Hant` choice stamps `zh` —
    /// script subtags never enter config), while the *text* locale keeps the
    /// script, since it picks glyphs.
    fn new(language: Option<&str>) -> Self {
        let lang = language
            .map(primary_language)
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "en".to_string());
        Self {
            catalog: provider_catalog_items(),
            lang,
            locale: SeedLocale::from_preference(language),
        }
    }

    fn season_language(&self) -> SeasonLanguage {
        match self.lang.as_str() {
            "zh" => SeasonLanguage::Zh,
            "ja" => SeasonLanguage::Ja,
            _ => SeasonLanguage::En,
        }
    }

    fn text(&self, value: L) -> &'static str {
        value.get(self.locale)
    }
}

#[derive(Clone, Copy)]
enum PrimaryDate {
    None,
    Season,
    ReleaseDate,
    EventDate,
}

#[derive(Clone, Copy)]
enum Extra {
    Platform(Sources),
    Genre(Sources),
    Author(Sources),
    Isbn(Sources),
    Players,
    Playtime,
    OwnedFormats,
    Location,
    Birthday(Sources),
}

struct ListSpec {
    heading: L,
    tracking: EpisodeTracking,
    /// The scalar progress/total field pair. `None` for open-ended lists
    /// (podcast episodes) and ownership lists (album tracks), where an
    /// "x of y" fraction isn't meaningful.
    progress: Option<ProgressSpec>,
}

struct ProgressSpec {
    total_field: &'static str,
    total_label: L,
}

struct TypeSpec {
    id: &'static str,
    category: TypePresetCategory,
    label: L,
    icon: &'static str,
    /// Vault folder name — localized like the label; the folders are where the
    /// user lives, so they should be in the user's language too.
    path: L,
    description: L,
    /// Providers in priority order; the primary is the first. Every id must
    /// exist in the provider catalog (a registry test enforces it — unknown ids
    /// are NOT silently filtered).
    providers: &'static [&'static str],
    /// Sources for the primary title field.
    title_sources: Sources,
    /// A separate original-title field with these sources; `None` for types
    /// without one (and for `name_based` types, whose *primary* title already
    /// carries the original role).
    original_title: Option<Sources>,
    /// Sources for the cover image field.
    cover_sources: Sources,
    /// Sources for the Summary body section; empty → no such section.
    summary_sources: Sources,
    /// Sources for the primary date field. For [`PrimaryDate::Season`] these
    /// may be plain air dates — the core coerces a date into a season.
    date_sources: Sources,
    /// Sources for the list total-count field (when `list` has a progress pair).
    total_sources: Sources,
    /// `None` → no status field (people/hub types).
    statuses: Option<&'static StatusVocab>,
    /// People/hub types: the filename and primary title use the
    /// language-neutral *original* name instead of the chosen title language,
    /// and there is no rating field. Explicit — meaning is never inferred from
    /// the shape of the other fields.
    name_based: bool,
    primary_date: PrimaryDate,
    completed_date: bool,
    list: Option<&'static ListSpec>,
    extras: &'static [Extra],
    relations: &'static [RelationSpec],
    /// Per-type daily-note hashtag, e.g. `Anime` → `- {title} {note} #Anime`.
    /// Localized text: the entity link is the `[[wikilink]]`, the hashtag is
    /// human organization. `None` → the type is not loggable.
    log_hashtag: Option<L>,
}

fn preset(ctx: &BuildCtx, spec: TypeSpec) -> Preset {
    Preset {
        category: spec.category,
        description: ctx.text(spec.description).to_string(),
        ongoing_shelf: spec
            .statuses
            .and_then(|vocab| vocab.ongoing_shelf)
            .map(|template| template.get(ctx.locale)),
        config: build_type(ctx, &spec),
    }
}

fn build_type(ctx: &BuildCtx, spec: &TypeSpec) -> EntityTypeConfig {
    let mut fields = vec![field("id", FieldType::Id, "ID")];

    // Primary title, in the chosen language, mapped from every provider's title.
    let mut title = field("title", FieldType::Title, ctx.text(TITLE_NAME));
    if !spec.name_based {
        title.title_language = Some(ctx.lang.clone());
    } else {
        title.title_role = Some(TitleRole::Original);
    }
    title.external_fields = source_mappings(spec.title_sources);
    fields.push(title);

    if let Some(sources) = spec.original_title {
        let mut original = field(
            "title_original",
            FieldType::Title,
            ctx.text(l("Title (original)", "原題", "原名", "原名")),
        );
        original.title_role = Some(TitleRole::Original);
        original.external_fields = source_mappings(sources);
        fields.push(original);
    }

    let mut cover = field(
        "cover_url",
        FieldType::Image,
        ctx.text(l("Cover", "カバー", "封面", "封面")),
    );
    cover.external_fields = source_mappings(spec.cover_sources);
    fields.push(cover);

    if let Some(vocab) = spec.statuses {
        fields.push(status_field(vocab, ctx.locale));
    }

    if !spec.name_based {
        fields.push(field(
            "rating",
            FieldType::Rating,
            ctx.text(l("Rating", "評価", "评分", "評分")),
        ));
    }

    if let Some(list) = spec.list {
        if let Some(progress) = &list.progress {
            let mut progress_field = field(
                "progress",
                FieldType::Progress,
                ctx.text(l("Progress", "進捗", "进度", "進度")),
            );
            progress_field.total_progress_field = Some(progress.total_field.to_string());
            fields.push(progress_field);
            let mut total = field(
                progress.total_field,
                FieldType::TotalProgress,
                ctx.text(progress.total_label),
            );
            total.external_fields = source_mappings(spec.total_sources);
            fields.push(total);
        }
    }

    for extra in spec.extras {
        fields.push(extra_field(ctx, *extra));
    }

    match spec.primary_date {
        PrimaryDate::None => {}
        PrimaryDate::Season => {
            let mut season = field(
                "season",
                FieldType::Season,
                ctx.text(l("Season", "放送時期", "季度", "季度")),
            );
            season.date_role = Some(DateRole::Planning);
            season.season_language = Some(ctx.season_language());
            season.external_fields = source_mappings(spec.date_sources);
            fields.push(season);
        }
        PrimaryDate::ReleaseDate => {
            let mut release = field(
                "release_date",
                FieldType::Date,
                ctx.text(l("Release date", "リリース日", "发行日期", "發行日期")),
            );
            release.date_role = Some(DateRole::Planning);
            release.external_fields = source_mappings(spec.date_sources);
            fields.push(release);
        }
        PrimaryDate::EventDate => {
            let mut date = field(
                "date",
                FieldType::Date,
                ctx.text(l("Date", "開催日", "日期", "日期")),
            );
            date.date_role = Some(DateRole::Event);
            fields.push(date);
        }
    }

    if spec.completed_date {
        let mut completed = field(
            "complete_date",
            FieldType::Date,
            ctx.text(l("Completed date", "完了日", "完成日期", "完成日期")),
        );
        completed.date_role = Some(DateRole::Completed);
        fields.push(completed);
    }

    for provider in spec.providers {
        if let Some(item) = ctx.catalog.iter().find(|item| item.id == *provider) {
            let mut external = field(
                &format!("{provider}_url"),
                FieldType::ExternalRef,
                &item.label,
            );
            external.external_ref = Some((*provider).to_string());
            external.external_types = item.default_external_types.clone();
            fields.push(external);
        }
    }

    for relation_spec in spec.relations {
        let mut relation = field(
            relation_spec.field,
            FieldType::Relation,
            ctx.text(relation_spec.label),
        );
        relation.relation_type = Some(relation_spec.target.to_string());
        fields.push(relation);
    }

    let body_sections = build_body_sections(ctx, spec);

    EntityTypeConfig {
        id: spec.id.to_string(),
        label: ctx.text(spec.label).to_string(),
        icon: Some(spec.icon.to_string()),
        path: ctx.text(spec.path).to_string(),
        external_priority: spec.providers.iter().map(|p| (*p).to_string()).collect(),
        filename: Some(if spec.name_based {
            FilenameConfig {
                title_language: None,
                title_role: Some(TitleRole::Original),
            }
        } else {
            FilenameConfig {
                title_language: Some(ctx.lang.clone()),
                title_role: None,
            }
        }),
        body_sections,
        log: spec.log_hashtag.map(|tag| TypeLogConfig {
            section: None,
            line_format: Some(format!("- {{title}} {{note}} #{}", ctx.text(tag))),
        }),
        fields,
    }
}

const TITLE_NAME: L = l("Title", "タイトル", "标题", "標題");

fn build_body_sections(ctx: &BuildCtx, spec: &TypeSpec) -> Vec<BodySection> {
    let mut sections = Vec::new();
    if !spec.summary_sources.is_empty() {
        let external_fields = source_mappings(spec.summary_sources);
        {
            sections.push(BodySection {
                heading: ctx.text(l("Summary", "概要", "简介", "簡介")).to_string(),
                kind: BodySectionKind::External,
                external_fields,
                tracking: None,
            });
        }
    }
    if let Some(list) = spec.list {
        sections.push(BodySection {
            heading: ctx.text(list.heading).to_string(),
            kind: BodySectionKind::Episodes,
            external_fields: Vec::new(),
            tracking: Some(list.tracking),
        });
    }
    sections
}

fn extra_field(ctx: &BuildCtx, extra: Extra) -> FieldConfig {
    match extra {
        Extra::Platform(sources) => {
            let mut f = field(
                "platform",
                FieldType::EnumList,
                ctx.text(l("Platform", "プラットフォーム", "平台", "平台")),
            );
            // Platform names are proper nouns — the same in every language.
            f.enum_options = [
                "PC",
                "Nintendo Switch",
                "PS5",
                "Xbox Series X|S",
                "iOS",
                "Android",
            ]
            .iter()
            .map(|v| v.to_string())
            .collect();
            f.external_fields = source_mappings(sources);
            f
        }
        Extra::Genre(sources) => {
            let mut f = field(
                "genre",
                FieldType::EnumList,
                ctx.text(l("Genre", "ジャンル", "类型", "類型")),
            );
            f.enum_options = [
                l("RPG", "RPG", "角色扮演", "角色扮演"),
                l("Action", "アクション", "动作", "動作"),
                l("Adventure", "アドベンチャー", "冒险", "冒險"),
                l("Strategy", "ストラテジー", "策略", "策略"),
                l("Simulation", "シミュレーション", "模拟", "模擬"),
                l("Puzzle", "パズル", "解谜", "解謎"),
            ]
            .iter()
            .map(|v| ctx.text(*v).to_string())
            .collect();
            f.external_fields = source_mappings(sources);
            f
        }
        Extra::Author(sources) => {
            let mut f = field(
                "author",
                FieldType::TextList,
                ctx.text(l("Author", "著者", "作者", "作者")),
            );
            f.external_fields = source_mappings(sources);
            f
        }
        Extra::Isbn(sources) => {
            let mut f = field("isbn", FieldType::Text, "ISBN");
            f.external_fields = source_mappings(sources);
            f
        }
        Extra::Players => field(
            "players",
            FieldType::Text,
            ctx.text(l("Players", "プレイ人数", "玩家人数", "玩家人數")),
        ),
        Extra::Playtime => field(
            "playtime",
            FieldType::Text,
            ctx.text(l("Playtime", "プレイ時間", "游玩时长", "遊玩時長")),
        ),
        Extra::OwnedFormats => {
            let mut f = field(
                "owned",
                FieldType::EnumList,
                ctx.text(l("Owned", "所持形式", "收藏形式", "收藏形式")),
            );
            f.enum_options = [
                l("CD", "CD", "CD", "CD"),
                l("Vinyl", "レコード", "黑胶", "黑膠"),
                l("Digital", "デジタル", "数字", "數位"),
            ]
            .iter()
            .map(|v| ctx.text(*v).to_string())
            .collect();
            f
        }
        Extra::Location => field(
            "location",
            FieldType::Text,
            ctx.text(l("Location", "場所", "地点", "地點")),
        ),
        Extra::Birthday(sources) => {
            let mut f = field(
                "birthday",
                FieldType::Date,
                ctx.text(l("Birthday", "誕生日", "生日", "生日")),
            );
            f.external_fields = source_mappings(sources);
            f
        }
    }
}

fn status_field(vocab: &StatusVocab, locale: SeedLocale) -> FieldConfig {
    let mut f = field(
        "status",
        FieldType::Enum,
        l("Status", "ステータス", "状态", "狀態").get(locale),
    );
    f.enum_options = vocab
        .options
        .iter()
        .map(|(_, label)| label.get(locale).to_string())
        .collect();
    f.enum_role = Some(EnumRole::Status);
    let mut values = StatusValues::default();
    for (canonical, label) in vocab.options {
        let bucket = match canonical {
            CanonicalStatus::Planning => &mut values.planning,
            CanonicalStatus::Ongoing => &mut values.ongoing,
            CanonicalStatus::Paused => &mut values.paused,
            CanonicalStatus::Completed => &mut values.completed,
            CanonicalStatus::Dropped => &mut values.dropped,
        };
        bucket.push(label.get(locale).to_string());
    }
    f.status_values = Some(values);
    f
}

/// The default "in progress" home section ("Watching Anime", "Playing Games")
/// for a type whose status **role** maps at least one ongoing value. The
/// *criteria* — field name and matched values — derive entirely from the role's
/// `statusValues` mapping, never from field or option names. Only the shelf
/// *title* comes from `shelf_template` (the status vocabulary's per-language
/// pattern), because natural titles can't be composed from a label across
/// languages. Types with no ongoing status (events: planned/attended) get no
/// such shelf.
fn ongoing_home_section_for(
    config: &EntityTypeConfig,
    shelf_template: Option<&str>,
) -> Option<HomeSectionConfig> {
    let template = shelf_template?;
    let status = config
        .fields
        .iter()
        .find(|field| field.enum_role == Some(EnumRole::Status))?;
    let ongoing = status
        .status_values
        .as_ref()
        .map(|values| values.ongoing.as_slice())
        .unwrap_or_default();
    ongoing.first()?;
    let rule = |value: &String| SmartFilterRule {
        kind: SmartFilterRuleKind::Compare,
        field: Some(status.field.clone()),
        op: Some(SmartCompareOp::Eq),
        value: Some(value.clone()),
        ..Default::default()
    };
    // The canonical shapes the settings editor produces: one equality, or an
    // "any of" subgroup when several values mean ongoing.
    let criteria = if ongoing.len() == 1 {
        SmartFilterGroup {
            conjunction: SmartFilterConjunction::All,
            rules: ongoing.iter().map(rule).collect(),
            groups: Vec::new(),
        }
    } else {
        SmartFilterGroup {
            conjunction: SmartFilterConjunction::All,
            rules: Vec::new(),
            groups: vec![SmartFilterSubgroup {
                conjunction: SmartFilterConjunction::Any,
                rules: ongoing.iter().map(rule).collect(),
            }],
        }
    };
    // Newest release first when the type has a planning date (season, release
    // date); otherwise the most recently touched file leads.
    let sort = config
        .fields
        .iter()
        .find(|field| field.date_role == Some(DateRole::Planning))
        .map(|field| format!("date:{}", field.field))
        .unwrap_or_else(|| "recentlyUpdated".to_string());
    Some(HomeSectionConfig {
        id: format!("ongoing-{}", config.id),
        title: template.replace("{label}", &config.label),
        entity_type: config.id.clone(),
        criteria: Some(criteria),
        limit: Some(12),
        sort: Some(sort),
        direction: Some(SortDirection::Desc),
    })
}

/// A default "Recent {label}" home section for a type — but **only** when the type
/// has a release/completion date to sort by. A chronological shelf is meaningless
/// for types with no such date (people, franchises) or whose only date is an
/// attendance date (events), so those are left out of the default Home entirely
/// rather than getting a title-sorted "recent" shelf that isn't really recent.
fn recent_home_section_for(
    config: &EntityTypeConfig,
    locale: SeedLocale,
) -> Option<HomeSectionConfig> {
    let date_field = config
        .fields
        .iter()
        .find(|field| field.date_role == Some(DateRole::Planning))
        .or_else(|| {
            config
                .fields
                .iter()
                .find(|field| field.date_role == Some(DateRole::Completed))
        })?;
    Some(HomeSectionConfig {
        id: format!("recent-{}", config.id),
        title: RECENT_SHELF.get(locale).replace("{label}", &config.label),
        entity_type: config.id.clone(),
        criteria: None,
        limit: Some(12),
        sort: Some(format!("date:{}", date_field.field)),
        direction: Some(SortDirection::Desc),
    })
}

// --- Field construction --------------------------------------------------------

fn field(name: &str, field_type: FieldType, display_name: &str) -> FieldConfig {
    FieldConfig {
        field: name.to_string(),
        field_type,
        display_name: Some(display_name.to_string()),
        title_language: None,
        title_role: None,
        external_fields: Vec::new(),
        enum_options: Vec::new(),
        enum_role: None,
        status_values: None,
        total_progress_field: None,
        date_role: None,
        season_language: None,
        external_ref: None,
        external_types: Vec::new(),
        relation_type: None,
    }
}

/// The first non-colliding key: `base`, then `suffix(base, 2)`, `suffix(base, 3)`,
/// … Comparison is case-insensitive for ids as well as paths — two type ids
/// differing only in case would be indistinguishable in most UIs.
fn unique_key(base: &str, taken: &HashSet<String>, suffix: impl Fn(&str, u32) -> String) -> String {
    let taken_lower: HashSet<String> = taken.iter().map(|k| k.to_lowercase()).collect();
    if !taken_lower.contains(&base.to_lowercase()) {
        return base.to_string();
    }
    let mut n = 2;
    loop {
        let candidate = suffix(base, n);
        if !taken_lower.contains(&candidate.to_lowercase()) {
            return candidate;
        }
        n += 1;
    }
}

// --- Literal external wiring -----------------------------------------------------

/// `(provider id, provider field id)` pairs — the literal external wiring for
/// one seeded field, written in the preset's provider-priority order. There is
/// deliberately no shared role table and no cleverness here: every preset
/// spells its mappings out at its declaration, and the registry test validates
/// each pair against the provider catalog's field options. The running app is
/// strictly schema-driven — this literal seeding lives here and nowhere else.
type Sources = &'static [(&'static str, &'static str)];

fn source_mappings(sources: Sources) -> Vec<ExternalFieldMapping> {
    sources
        .iter()
        .map(|(source, field)| ExternalFieldMapping {
            source: (*source).to_string(),
            field: (*field).to_string(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolve(
        current: Vec<EntityTypeConfig>,
        ids: &[&str],
        language: Option<&str>,
    ) -> ResolveTypePresetsResponse {
        resolve_presets(&ResolveTypePresetsRequest {
            current_types: current,
            preset_ids: ids.iter().map(|s| s.to_string()).collect(),
            language: language.map(str::to_string),
        })
    }

    fn find_field<'a>(type_config: &'a EntityTypeConfig, name: &str) -> Option<&'a FieldConfig> {
        type_config.fields.iter().find(|f| f.field == name)
    }

    #[test]
    fn every_preset_appears_in_the_catalog() {
        let response = type_presets_response(None);
        let ids: Vec<_> = response.presets.iter().map(|p| p.id.as_str()).collect();
        assert!(ids.contains(&"anime"));
        assert!(ids.contains(&"franchise"));
        assert_eq!(response.presets.len(), 13);
        // Every preset's category is one the response advertises.
        let categories: HashSet<_> = response.categories.iter().map(|c| c.id).collect();
        for preset in &response.presets {
            assert!(categories.contains(&preset.category));
        }
    }

    #[test]
    fn every_referenced_provider_is_in_the_catalog() {
        // The registry must not drift from the provider catalog: every provider a
        // preset wires (priority list, field mappings, external refs) resolves,
        // and the wiring actually produced a title and a cover mapping. Nothing
        // is silently filtered, so a typo'd or removed provider fails here.
        let ctx = BuildCtx::new(None);
        let catalog_ids: HashSet<&str> = ctx.catalog.iter().map(|item| item.id.as_str()).collect();
        for preset in built_presets(&ctx) {
            let config = &preset.config;
            for provider in &config.external_priority {
                assert!(
                    catalog_ids.contains(provider.as_str()),
                    "{}: externalPriority provider {provider} not in the catalog",
                    config.id
                );
            }
            for field in &config.fields {
                for mapping in &field.external_fields {
                    // The mappings are literal per preset, so validate both
                    // halves of each pair: the provider exists, and the mapped
                    // field is one the provider actually declares mappable — a
                    // typo'd field id would otherwise seed a dead mapping.
                    let provider = ctx
                        .catalog
                        .iter()
                        .find(|item| item.id == mapping.source)
                        .unwrap_or_else(|| {
                            panic!(
                                "{}.{}: mapping source {} not in the catalog",
                                config.id, field.field, mapping.source
                            )
                        });
                    assert!(
                        provider
                            .fields
                            .iter()
                            .any(|option| option.field == mapping.field),
                        "{}.{}: {} declares no mappable field {:?}",
                        config.id,
                        field.field,
                        mapping.source,
                        mapping.field
                    );
                }
                if let Some(provider) = &field.external_ref {
                    assert!(
                        catalog_ids.contains(provider.as_str()),
                        "{}.{}: externalRef {provider} not in the catalog",
                        config.id,
                        field.field
                    );
                }
            }
            if !config.external_priority.is_empty() {
                let title = find_field(config, "title").expect("title field");
                assert!(
                    !title.external_fields.is_empty(),
                    "{}: no provider maps a title",
                    config.id
                );
                let cover = find_field(config, "cover_url").expect("cover field");
                assert!(
                    !cover.external_fields.is_empty(),
                    "{}: no provider maps a cover",
                    config.id
                );
            }
        }
    }

    #[test]
    fn every_media_preset_has_a_status_role() {
        // The bug the presets fix vs. the old templates: status-driven behavior
        // needs enumRole + statusValues on the status field.
        let ctx = BuildCtx::new(None);
        for preset in built_presets(&ctx) {
            let Some(status) = find_field(&preset.config, "status") else {
                continue; // people/hub types have no status
            };
            assert_eq!(
                status.enum_role,
                Some(EnumRole::Status),
                "{} status missing enumRole",
                preset.config.id
            );
            let values = status.status_values.as_ref().expect("status_values");
            assert!(
                !values.completed.is_empty(),
                "{} completed unmapped",
                preset.config.id
            );
        }
    }

    #[test]
    fn relation_dropped_when_target_absent() {
        // Anime alone: no franchise type, so its franchise relation is stripped.
        let result = resolve(vec![], &["anime"], None);
        let anime = &result.types[0];
        assert!(find_field(anime, "franchise").is_none());
    }

    #[test]
    fn relation_wired_when_co_selected() {
        // Anime + Franchise together: the franchise link survives.
        let result = resolve(vec![], &["anime", "franchise"], None);
        let anime = result.types.iter().find(|t| t.id == "anime").unwrap();
        let franchise = find_field(anime, "franchise").expect("franchise relation kept");
        assert_eq!(franchise.relation_type.as_deref(), Some("franchise"));
        // No back-fills — both sides are new.
        assert!(result.backfills.is_empty());
    }

    #[test]
    fn relation_to_existing_type_is_kept() {
        // Franchise already in the vault; add anime → its link resolves.
        let franchise = resolve(vec![], &["franchise"], None).types.remove(0);
        let result = resolve(vec![franchise], &["anime"], None);
        let anime = &result.types[0];
        assert!(find_field(anime, "franchise").is_some());
    }

    #[test]
    fn existing_target_wins_over_a_co_selected_preset() {
        // The vault already has a `franchise` type AND the user co-selects the
        // Franchise preset (which gets suffixed to `franchise-2`). The tie-break:
        // relations bind to the *existing* type; the co-selected copy is still
        // added under its suffixed id, just not linked.
        let existing_franchise = resolve(vec![], &["franchise"], None).types.remove(0);
        let result = resolve(vec![existing_franchise], &["anime", "franchise"], None);
        let anime = result.types.iter().find(|t| t.id == "anime").unwrap();
        let link = find_field(anime, "franchise").expect("franchise relation kept");
        assert_eq!(link.relation_type.as_deref(), Some("franchise"));
        assert!(result.types.iter().any(|t| t.id == "franchise-2"));
    }

    #[test]
    fn backfill_proposed_for_existing_type_pointing_at_new_type() {
        // Anime already added (with its franchise relation stripped). Adding
        // Franchise later proposes back-filling anime's franchise link.
        let anime = resolve(vec![], &["anime"], None).types.remove(0);
        assert!(find_field(&anime, "franchise").is_none());
        let result = resolve(vec![anime], &["franchise"], None);
        assert_eq!(result.backfills.len(), 1);
        let backfill = &result.backfills[0];
        assert_eq!(backfill.type_id, "anime");
        assert_eq!(backfill.preset_id, "franchise");
        assert_eq!(backfill.field.field, "franchise");
        assert_eq!(backfill.field.relation_type.as_deref(), Some("franchise"));
    }

    #[test]
    fn artist_self_relation_survives() {
        // Artist's `groups` relation targets `artist` itself — always satisfiable.
        let result = resolve(vec![], &["artist"], None);
        let artist = &result.types[0];
        let groups = find_field(artist, "groups").expect("groups kept");
        assert_eq!(groups.relation_type.as_deref(), Some("artist"));
    }

    #[test]
    fn id_and_path_collisions_are_suffixed() {
        let existing = EntityTypeConfig {
            id: "anime".to_string(),
            label: "My Anime".to_string(),
            icon: None,
            path: "Anime".to_string(),
            external_priority: Vec::new(),
            filename: None,
            body_sections: Vec::new(),
            log: None,
            fields: Vec::new(),
        };
        let result = resolve(vec![existing], &["anime"], None);
        assert_eq!(result.types[0].id, "anime-2");
        assert_eq!(result.types[0].path, "Anime 2");
        assert_eq!(result.collisions.len(), 1);
        assert_eq!(result.collisions[0].assigned_id, "anime-2");
    }

    #[test]
    fn one_language_choice_stamps_titles_and_localizes_text() {
        // The single `language` choice drives both derivations. For a
        // Traditional-Chinese user the seeded *text* (label, path, statuses,
        // field names, shelf title, log hashtag) is 繁體 — while every stamped
        // language *key* is the bare primary subtag per the i18n invariant.
        let result = resolve(vec![], &["anime"], Some("zh-Hant"));
        let anime = &result.types[0];
        assert_eq!(anime.label, "動畫");
        assert_eq!(anime.path, "動畫");
        let title = find_field(anime, "title").unwrap();
        assert_eq!(title.title_language.as_deref(), Some("zh")); // bare, no script
        assert_eq!(title.display_name.as_deref(), Some("標題"));
        let season = find_field(anime, "season").unwrap();
        assert_eq!(season.season_language, Some(SeasonLanguage::Zh));
        assert_eq!(
            anime.filename.as_ref().unwrap().title_language.as_deref(),
            Some("zh")
        );
        let status = find_field(anime, "status").unwrap();
        assert!(status.enum_options.contains(&"想看".to_string()));
        assert!(status.enum_options.contains(&"看過".to_string()));
        let values = status.status_values.as_ref().unwrap();
        assert_eq!(values.ongoing, vec!["在看".to_string()]);
        assert_eq!(result.home_sections[0].title, "在看的動畫");
        let log = anime.log.as_ref().unwrap();
        assert_eq!(log.line_format.as_deref(), Some("- {title} {note} #動畫"));

        // Same title language, Simplified script for the text.
        let hans = resolve(vec![], &["anime"], Some("zh-Hans"));
        assert_eq!(hans.types[0].label, "动画");
        let title = find_field(&hans.types[0], "title").unwrap();
        assert_eq!(title.title_language.as_deref(), Some("zh"));

        // Japanese.
        let ja = resolve(vec![], &["games"], Some("ja"));
        let games = &ja.types[0];
        assert_eq!(games.label, "ゲーム");
        let status = find_field(games, "status").unwrap();
        assert!(status.enum_options.contains(&"プレイ中".to_string()));
        assert_eq!(ja.home_sections[0].title, "プレイ中のゲーム");
    }

    #[test]
    fn unwritten_languages_keep_titles_but_fall_back_to_english_text() {
        // Korean isn't a language the presets are written in: the title
        // language is still honored (ko titles/filenames), only the text is
        // English. Absent language → English throughout.
        let ko = resolve(vec![], &["anime"], Some("ko"));
        assert_eq!(ko.types[0].label, "Anime");
        assert_eq!(ko.home_sections[0].title, "Watching Anime");
        let title = find_field(&ko.types[0], "title").unwrap();
        assert_eq!(title.title_language.as_deref(), Some("ko"));

        let none = resolve(vec![], &["anime"], None);
        assert_eq!(none.types[0].label, "Anime");
        let title = find_field(&none.types[0], "title").unwrap();
        assert_eq!(title.title_language.as_deref(), Some("en"));
    }

    #[test]
    fn summaries_localize_for_the_language_preference() {
        let response = type_presets_response(Some("ja"));
        let anime = response.presets.iter().find(|p| p.id == "anime").unwrap();
        assert_eq!(anime.label, "アニメ");
        assert!(!anime.description.is_empty());
        assert_ne!(
            anime.description,
            "Track what you're watching and where you left off."
        );
        let watch = response
            .categories
            .iter()
            .find(|c| c.id == TypePresetCategory::Watch)
            .unwrap();
        assert_eq!(watch.label, "観る");

        // Region subtags map to a script: zh-TW means Traditional.
        let hant = type_presets_response(Some("zh-TW"));
        let anime = hant.presets.iter().find(|p| p.id == "anime").unwrap();
        assert_eq!(anime.label, "動畫");
    }

    #[test]
    fn external_refs_and_mappings_wire_from_catalog() {
        let anime = resolve(vec![], &["anime"], None).types.remove(0);
        // Title maps from Bangumi's localized name.
        let title = find_field(&anime, "title").unwrap();
        assert!(title
            .external_fields
            .iter()
            .any(|m| m.source == "bangumi" && m.field == "name_cn"));
        // External-ref field carries the provider id and default types.
        let bgm = find_field(&anime, "bangumi_url").expect("bangumi_url");
        assert_eq!(bgm.external_ref.as_deref(), Some("bangumi"));
        assert_eq!(bgm.field_type, FieldType::ExternalRef);
        // Summary body section wired from providers.
        assert!(anime
            .body_sections
            .iter()
            .any(|s| s.heading == "Summary" && !s.external_fields.is_empty()));
    }

    #[test]
    fn season_and_total_episodes_are_mapped() {
        // The season field pulls from providers' air dates (coerced to a season)
        // and MAL's explicit season; the total-episodes field pulls the count.
        let anime = resolve(vec![], &["anime"], None).types.remove(0);
        let season = find_field(&anime, "season").expect("season field");
        assert!(
            season
                .external_fields
                .iter()
                .any(|m| m.source == "bangumi" && m.field == "date"),
            "season should coerce from Bangumi's air date"
        );
        assert!(season
            .external_fields
            .iter()
            .any(|m| m.source == "myanimelist" && m.field == "season"));

        let episodes = find_field(&anime, "episodes").expect("episodes field");
        assert!(episodes
            .external_fields
            .iter()
            .any(|m| m.source == "bangumi" && m.field == "eps"));
        assert!(episodes
            .external_fields
            .iter()
            .any(|m| m.source == "tmdb" && m.field == "episode_count"));

        // Manga's chapter count wires from MAL/MangaUpdates.
        let manga = resolve(vec![], &["manga"], None).types.remove(0);
        let chapters = find_field(&manga, "chapters").expect("chapters field");
        assert!(chapters
            .external_fields
            .iter()
            .any(|m| m.source == "myanimelist" && m.field == "chapters"));
        assert!(chapters
            .external_fields
            .iter()
            .any(|m| m.source == "mangaupdates" && m.field == "latest_chapter"));
    }

    #[test]
    fn open_ended_lists_seed_no_progress_pair() {
        // Podcasts are open-ended: an episode checklist, but no scalar
        // progress/total fields ("x of y" has no meaningful y).
        let podcast = resolve(vec![], &["podcast"], None).types.remove(0);
        assert!(find_field(&podcast, "progress").is_none());
        assert!(find_field(&podcast, "episodes").is_none());
        let list = podcast
            .body_sections
            .iter()
            .find(|s| s.kind == BodySectionKind::Episodes)
            .expect("episodes section");
        assert_eq!(list.tracking, Some(EpisodeTracking::Checklist));

        // Albums are owned, not progressed through: a plain track list.
        let music = resolve(vec![], &["music"], None).types.remove(0);
        assert!(find_field(&music, "progress").is_none());
        assert!(find_field(&music, "tracks").is_none());
        let list = music
            .body_sections
            .iter()
            .find(|s| s.kind == BodySectionKind::Episodes)
            .expect("tracks section");
        assert_eq!(list.tracking, Some(EpisodeTracking::None));
    }

    #[test]
    fn one_default_home_shelf_per_type_preferring_ongoing() {
        // Anime has an ongoing status ("Watching"), so its single default shelf
        // is the in-progress one — not a second "Recent" shelf on top. Franchise
        // has no date field or status; Event's only date is an attendance date
        // (role Event) and its statuses map no ongoing value — so neither gets
        // a shelf at all.
        let result = resolve(vec![], &["anime", "franchise", "event"], None);
        assert_eq!(result.home_sections.len(), 1);
        let ongoing = &result.home_sections[0];
        assert_eq!(ongoing.id, "ongoing-anime");
        assert_eq!(ongoing.entity_type, "anime");
        assert_eq!(ongoing.title, "Watching Anime");
        assert_eq!(ongoing.sort.as_deref(), Some("date:season"));
        // The date-less types are still added — just shelf-less.
        assert!(result.types.iter().any(|t| t.id == "franchise"));
        assert!(result.types.iter().any(|t| t.id == "event"));
    }

    #[test]
    fn multi_type_resolve_yields_at_most_one_shelf_per_type() {
        // Every media preset maps an ongoing status, so a three-type onboarding
        // yields exactly three shelves — all in-progress ones, no "Recent"
        // duplicates bloating the default home.
        let result = resolve(vec![], &["anime", "games", "movie"], None);
        assert_eq!(result.home_sections.len(), 3);
        assert!(result
            .home_sections
            .iter()
            .all(|section| section.id.starts_with("ongoing-")));
    }

    #[test]
    fn ongoing_home_section_criteria_derive_from_the_status_role() {
        // The criteria come from the status role's ongoing mapping: field name
        // and value are the mapped data, nothing hardcoded. Only the shelf
        // *title* comes from the vocabulary's per-language template.
        let result = resolve(vec![], &["games"], None);
        let ongoing = result
            .home_sections
            .iter()
            .find(|section| section.id == "ongoing-games")
            .expect("ongoing shelf");
        assert_eq!(ongoing.title, "Playing Games");
        let criteria = ongoing.criteria.as_ref().expect("criteria");
        assert_eq!(criteria.rules.len(), 1);
        let rule = &criteria.rules[0];
        assert_eq!(rule.field.as_deref(), Some("status"));
        assert_eq!(rule.value.as_deref(), Some("Playing"));
    }

    #[test]
    fn resolved_config_serializes_to_yaml() {
        let anime = resolve(vec![], &["anime"], Some("en")).types.remove(0);
        let yaml = serde_yaml::to_string(&anime).unwrap();
        assert!(yaml.contains("id: anime"));
        assert!(yaml.contains("enumRole: status"));
    }
}
