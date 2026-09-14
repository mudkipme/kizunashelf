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
//!   *binds* when its target type is present (existing or co-selected).
//!   Otherwise a provider-wired relation falls back to a plain text-list field
//!   with the same wiring — the provider names are still captured, just as text
//!   instead of links — and an unwired relation is dropped.
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
    ResolveTypePresetsRequest, ResolveTypePresetsResponse, SmartCompareOp, SmartDurationUnit,
    SmartFilterConjunction, SmartFilterGroup, SmartFilterRule, SmartFilterRuleKind,
    SmartFilterSubgroup, SmartRelativeDate, TypePresetBackfill, TypePresetCategory,
    TypePresetCategoryInfo, TypePresetCollision, TypePresetProvider, TypePresetSummary,
    TypePresetsResponse,
};
use crate::languages::primary_language;
use crate::types::{
    BodySection, BodySectionKind, CanonicalStatus, DateRole, EntityTypeConfig, EnumRole,
    EpisodeTracking, ExternalFieldMapping, FieldConfig, FieldType, FilenameConfig, SeasonLanguage,
    SortDirection, StatusValues, TitleRole, TypeLogConfig,
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
            // Target absent. A provider-wired relation falls back to a plain
            // text list — the providers still supply names worth capturing
            // (album artists, podcast hosts), just as text instead of links.
            // An unwired relation is dropped: nothing would ever fill it.
            if field.external_fields.is_empty() {
                return false;
            }
            field.field_type = FieldType::TextList;
            field.relation_type = None;
            true
        });
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
        collisions,
    }
}

/// A proposed ordinary smart list, derived from the live schema's roles.
#[derive(Clone, Debug)]
pub struct SuggestedList {
    pub id: String,
    pub title: String,
    pub entity_type: String,
    pub criteria: Option<SmartFilterGroup>,
    pub sort: crate::contract::SmartSortSpec,
}

pub fn suggested_lists(types: &[EntityTypeConfig], language: Option<&str>) -> Vec<SuggestedList> {
    let ctx = BuildCtx::new(language);
    let presets = built_presets(&ctx);
    let fallback = l(
        "In progress: {label}",
        "進行中：{label}",
        "进行中：{label}",
        "進行中：{label}",
    );
    types
        .iter()
        .filter_map(|config| {
            let template = presets
                .iter()
                .find(|preset| preset.config.id == config.id)
                .and_then(|preset| preset.ongoing_shelf)
                .unwrap_or(fallback.get(ctx.locale));
            upcoming_event_list_for(config, ctx.locale)
                .or_else(|| ongoing_list_for(config, Some(template)))
                .or_else(|| recent_list_for(config, ctx.locale))
        })
        .collect()
}

fn suggested_sort(field: Option<&str>, direction: SortDirection) -> crate::contract::SmartSortSpec {
    let property = field
        .map(|field| crate::smart_lists::SortProperty::Note(field.to_string()))
        .unwrap_or(crate::smart_lists::SortProperty::FileMtime);
    crate::contract::SmartSortSpec {
        property: crate::smart_lists::print_sort_property(&property),
        direction,
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
                providers: ctx.pick(
                    &["bangumi", "anilist", "myanimelist", "tmdb", "thetvdb"],
                    &["bangumi", "anilist", "myanimelist", "tmdb", "thetvdb"],
                    &["anilist", "myanimelist", "tmdb", "thetvdb", "bangumi"],
                ),
                external_types: &[
                    ("bangumi", &["2"]),
                    ("anilist", &["anime"]),
                    ("myanimelist", &["anime"]),
                    ("tmdb", &["tv"]),
                    ("thetvdb", &["series"]),
                ],
                title_sources: ctx.pick(
                    &[
                        ("bangumi", "name_cn"),
                        ("anilist", "title"),
                        ("myanimelist", "title"),
                        ("tmdb", "title"),
                    ],
                    &[
                        ("bangumi", "name"),
                        ("anilist", "title"),
                        ("myanimelist", "title"),
                        ("tmdb", "title"),
                    ],
                    &[
                        ("anilist", "title"),
                        ("myanimelist", "title"),
                        ("tmdb", "title"),
                    ],
                ),
                original_title: Some(&[
                    ("bangumi", "name"),
                    ("anilist", "native_title"),
                    ("tmdb", "original_title"),
                    ("thetvdb", "name"),
                ]),
                cover_sources: &[
                    ("bangumi", "cover_url"),
                    ("anilist", "cover_url"),
                    ("myanimelist", "cover_url"),
                    ("tmdb", "cover_url"),
                    ("thetvdb", "cover_url"),
                ],
                summary_sources: ctx.pick(
                    &[
                        ("bangumi", "summary"),
                        ("anilist", "synopsis"),
                        ("myanimelist", "synopsis"),
                        ("tmdb", "overview"),
                        ("thetvdb", "overview"),
                    ],
                    // Bangumi summaries are Chinese — zh only.
                    &[
                        ("anilist", "synopsis"),
                        ("myanimelist", "synopsis"),
                        ("tmdb", "overview"),
                        ("thetvdb", "overview"),
                    ],
                    &[
                        ("anilist", "synopsis"),
                        ("myanimelist", "synopsis"),
                        ("tmdb", "overview"),
                        ("thetvdb", "overview"),
                    ],
                ),
                // Air dates coerce to a season; AniList/MAL have explicit season labels.
                date_sources: &[
                    ("bangumi", "date"),
                    ("anilist", "season"),
                    ("myanimelist", "season"),
                    ("tmdb", "release_date"),
                    ("thetvdb", "first_air_time"),
                ],
                statuses: Some(&WATCH_STATUS),
                name_based: false,
                primary_date: PrimaryDate::Season,
                started_date: true,
                completed_date: true,
                progress: Some(&ANIME_PROGRESS),
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
                providers: &["tmdb", "thetvdb", "neodb"],
                external_types: &[
                    ("tmdb", &["tv"]),
                    ("thetvdb", &["series"]),
                    ("neodb", &["tv"]),
                ],
                title_sources: &[("tmdb", "title"), ("neodb", "title")],
                original_title: Some(&[
                    ("tmdb", "original_title"),
                    ("thetvdb", "name"),
                    ("neodb", "original_title"),
                ]),
                cover_sources: &[
                    ("tmdb", "cover_url"),
                    ("thetvdb", "cover_url"),
                    ("neodb", "cover_url"),
                ],
                summary_sources: &[
                    ("tmdb", "overview"),
                    ("thetvdb", "overview"),
                    ("neodb", "description"),
                ],
                date_sources: &[
                    ("tmdb", "release_date"),
                    ("thetvdb", "first_air_time"),
                    ("neodb", "release_date"),
                ],
                statuses: Some(&WATCH_STATUS),
                name_based: false,
                primary_date: PrimaryDate::Season,
                started_date: true,
                completed_date: true,
                progress: Some(&DRAMA_PROGRESS),
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
                providers: &["tmdb", "thetvdb", "neodb"],
                external_types: &[
                    ("tmdb", &["movie"]),
                    ("thetvdb", &["movie"]),
                    ("neodb", &["movie"]),
                ],
                title_sources: &[("tmdb", "title"), ("neodb", "title")],
                original_title: Some(&[
                    ("tmdb", "original_title"),
                    ("thetvdb", "name"),
                    ("neodb", "original_title"),
                ]),
                cover_sources: &[
                    ("tmdb", "cover_url"),
                    ("thetvdb", "cover_url"),
                    ("neodb", "cover_url"),
                ],
                summary_sources: &[
                    ("tmdb", "overview"),
                    ("thetvdb", "overview"),
                    ("neodb", "description"),
                ],
                date_sources: &[
                    ("tmdb", "release_date"),
                    ("thetvdb", "first_air_time"),
                    ("neodb", "release_date"),
                ],
                statuses: Some(&WATCH_STATUS),
                name_based: false,
                primary_date: PrimaryDate::ReleaseDate,
                started_date: false,
                completed_date: true,
                progress: None,
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
                providers: ctx.pick(
                    &["igdb", "steam", "bangumi", "neodb"],
                    &["igdb", "steam", "bangumi", "neodb"],
                    &["igdb", "steam", "neodb", "bangumi"],
                ),
                external_types: &[("bangumi", &["4"]), ("neodb", &["game"])],
                title_sources: ctx.pick(
                    &[
                        ("igdb", "name"),
                        ("steam", "name"),
                        ("bangumi", "name_cn"),
                        ("neodb", "title"),
                    ],
                    &[
                        ("igdb", "name"),
                        ("steam", "name"),
                        ("bangumi", "name"),
                        ("neodb", "title"),
                    ],
                    &[("igdb", "name"), ("steam", "name"), ("neodb", "title")],
                ),
                original_title: None,
                cover_sources: &[
                    ("igdb", "cover_url"),
                    ("steam", "cover_url"),
                    ("bangumi", "cover_url"),
                    ("neodb", "cover_url"),
                ],
                summary_sources: ctx.pick(
                    &[
                        ("igdb", "summary"),
                        ("steam", "description"),
                        ("bangumi", "summary"),
                        ("neodb", "description"),
                    ],
                    &[
                        ("igdb", "summary"),
                        ("steam", "description"),
                        ("neodb", "description"),
                    ],
                    &[
                        ("igdb", "summary"),
                        ("steam", "description"),
                        ("neodb", "description"),
                    ],
                ),
                date_sources: &[
                    ("igdb", "first_release_date"),
                    ("steam", "release_date"),
                    ("bangumi", "date"),
                    ("neodb", "release_date"),
                ],
                statuses: Some(&PLAY_STATUS),
                name_based: false,
                primary_date: PrimaryDate::ReleaseDate,
                started_date: true,
                completed_date: true,
                progress: None,
                list: None,
                extras: GAMES_EXTRAS,
                relations: &[GAMES_FRANCHISE_REL],
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
                external_types: &[("bgg", &["boardgame", "boardgameexpansion"])],
                title_sources: &[("bgg", "name")],
                original_title: None,
                cover_sources: &[("bgg", "cover_url")],
                summary_sources: &[("bgg", "description")],
                // BGG exposes only a year, which the release-date field can't hold.
                date_sources: &[],
                statuses: Some(&PLAY_STATUS),
                name_based: false,
                primary_date: PrimaryDate::ReleaseDate,
                started_date: false,
                completed_date: false,
                progress: None,
                list: None,
                extras: BOARD_EXTRAS,
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
                // The two key-less providers bracket the keyed ones: NeoDB's
                // book catalog skews Chinese, so it leads for zh users and
                // falls to the bottom for everyone else, where key-less
                // OpenLibrary leads instead — book search works out of the
                // box in every language with no credentials configured.
                providers: ctx.pick(
                    &["neodb", "openlibrary", "googlebooks", "hardcover"],
                    &["openlibrary", "googlebooks", "hardcover", "neodb"],
                    &["openlibrary", "googlebooks", "hardcover", "neodb"],
                ),
                external_types: &[("neodb", &["book"])],
                title_sources: &[
                    ("neodb", "title"),
                    ("openlibrary", "title"),
                    ("googlebooks", "title"),
                    ("hardcover", "title"),
                ],
                original_title: Some(&[("neodb", "original_title")]),
                cover_sources: &[
                    ("neodb", "cover_url"),
                    ("openlibrary", "cover_url"),
                    ("googlebooks", "cover_url"),
                    ("hardcover", "cover_url"),
                ],
                summary_sources: &[
                    ("neodb", "description"),
                    ("openlibrary", "description"),
                    ("googlebooks", "description"),
                    ("hardcover", "synopsis"),
                ],
                date_sources: &[
                    ("neodb", "published_date"),
                    ("openlibrary", "published_date"),
                    ("googlebooks", "published_date"),
                    ("hardcover", "publish_date"),
                ],
                statuses: Some(&READ_STATUS),
                name_based: false,
                primary_date: PrimaryDate::ReleaseDate,
                started_date: true,
                completed_date: true,
                progress: Some(&BOOK_PROGRESS),
                list: None,
                extras: BOOKS_EXTRAS,
                relations: &[BOOKS_FRANCHISE_REL],
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
                providers: ctx.pick(
                    &[
                        "bangumi",
                        "mangaupdates",
                        "anilist",
                        "myanimelist",
                        "comicvine",
                    ],
                    &[
                        "bangumi",
                        "mangaupdates",
                        "anilist",
                        "myanimelist",
                        "comicvine",
                    ],
                    &[
                        "mangaupdates",
                        "anilist",
                        "myanimelist",
                        "comicvine",
                        "bangumi",
                    ],
                ),
                // Bangumi files manga under its book (1) subject type.
                external_types: &[
                    ("bangumi", &["1"]),
                    ("anilist", &["manga"]),
                    ("myanimelist", &["manga"]),
                ],
                title_sources: ctx.pick(
                    &[
                        ("bangumi", "name_cn"),
                        ("mangaupdates", "title"),
                        ("anilist", "title"),
                        ("myanimelist", "title"),
                        ("comicvine", "title"),
                    ],
                    &[
                        ("bangumi", "name"),
                        ("mangaupdates", "title"),
                        ("anilist", "title"),
                        ("myanimelist", "title"),
                        ("comicvine", "title"),
                    ],
                    &[
                        ("mangaupdates", "title"),
                        ("anilist", "title"),
                        ("myanimelist", "title"),
                        ("comicvine", "title"),
                    ],
                ),
                original_title: Some(&[("bangumi", "name"), ("anilist", "native_title")]),
                cover_sources: &[
                    ("bangumi", "cover_url"),
                    ("mangaupdates", "cover_url"),
                    ("anilist", "cover_url"),
                    ("myanimelist", "cover_url"),
                    ("comicvine", "cover_url"),
                ],
                summary_sources: ctx.pick(
                    &[
                        ("bangumi", "summary"),
                        ("mangaupdates", "synopsis"),
                        ("anilist", "synopsis"),
                        ("myanimelist", "synopsis"),
                        ("comicvine", "description"),
                    ],
                    &[
                        ("mangaupdates", "synopsis"),
                        ("anilist", "synopsis"),
                        ("myanimelist", "synopsis"),
                        ("comicvine", "description"),
                    ],
                    &[
                        ("mangaupdates", "synopsis"),
                        ("anilist", "synopsis"),
                        ("myanimelist", "synopsis"),
                        ("comicvine", "description"),
                    ],
                ),
                date_sources: &[
                    ("bangumi", "date"),
                    ("anilist", "start_date"),
                    ("myanimelist", "start_date"),
                ],
                statuses: Some(&READ_STATUS),
                name_based: false,
                primary_date: PrimaryDate::ReleaseDate,
                started_date: true,
                completed_date: true,
                progress: Some(&MANGA_PROGRESS),
                list: Some(&CHAPTERS_LIST),
                extras: MANGA_EXTRAS,
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
                providers: ctx.pick(
                    &["musicbrainz", "applemusic", "discogs", "bangumi", "neodb"],
                    &["musicbrainz", "applemusic", "discogs", "bangumi", "neodb"],
                    &["musicbrainz", "applemusic", "discogs", "neodb", "bangumi"],
                ),
                external_types: &[
                    ("musicbrainz", &["release"]),
                    ("discogs", &["release"]),
                    ("bangumi", &["3"]),
                    ("neodb", &["music"]),
                ],
                title_sources: ctx.pick(
                    &[
                        ("musicbrainz", "title"),
                        ("applemusic", "title"),
                        ("discogs", "title"),
                        ("bangumi", "name_cn"),
                        ("neodb", "title"),
                    ],
                    &[
                        ("musicbrainz", "title"),
                        ("applemusic", "title"),
                        ("discogs", "title"),
                        ("bangumi", "name"),
                        ("neodb", "title"),
                    ],
                    &[
                        ("musicbrainz", "title"),
                        ("applemusic", "title"),
                        ("discogs", "title"),
                        ("neodb", "title"),
                    ],
                ),
                original_title: None,
                cover_sources: &[
                    ("musicbrainz", "cover_url"),
                    ("applemusic", "cover_url"),
                    ("discogs", "cover_url"),
                    ("bangumi", "cover_url"),
                    ("neodb", "cover_url"),
                ],
                summary_sources: ctx.pick(
                    &[("bangumi", "summary"), ("neodb", "description")],
                    &[("neodb", "description")],
                    &[("neodb", "description")],
                ),
                date_sources: &[
                    ("musicbrainz", "release_date"),
                    ("applemusic", "release_date"),
                    ("bangumi", "date"),
                    ("neodb", "release_date"),
                ],
                statuses: Some(&LISTEN_STATUS),
                name_based: false,
                primary_date: PrimaryDate::ReleaseDate,
                started_date: false,
                completed_date: false,
                progress: None,
                list: Some(&TRACKS_LIST),
                extras: MUSIC_EXTRAS,
                relations: &[MUSIC_ARTIST_REL, FRANCHISE_REL],
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
                providers: &["applepodcast", "neodb"],
                external_types: &[("neodb", &["podcast"])],
                title_sources: &[("applepodcast", "title"), ("neodb", "title")],
                original_title: None,
                cover_sources: &[("applepodcast", "cover_url"), ("neodb", "cover_url")],
                summary_sources: &[("neodb", "description")],
                date_sources: &[],
                statuses: Some(&LISTEN_STATUS),
                name_based: false,
                primary_date: PrimaryDate::None,
                started_date: false,
                completed_date: false,
                progress: None,
                list: Some(&PODCAST_EPISODES_LIST),
                extras: PODCAST_EXTRAS,
                relations: &[PODCAST_HOST_REL],
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
                providers: ctx.pick(
                    &["bangumi", "musicbrainz"],
                    &["bangumi", "musicbrainz"],
                    &["musicbrainz", "bangumi"],
                ),
                // Unconstrained MusicBrainz searches releases, not artists, so
                // this pin is what makes the artist field find people at all.
                external_types: &[("bangumi", &["person"]), ("musicbrainz", &["artist"])],
                title_sources: ctx.pick(
                    &[("bangumi", "name_cn"), ("musicbrainz", "title")],
                    &[("bangumi", "name"), ("musicbrainz", "title")],
                    // Bangumi has no Latin names; the Japanese original beats Chinese.
                    &[("musicbrainz", "title"), ("bangumi", "name")],
                ),
                original_title: None,
                cover_sources: &[("bangumi", "cover_url"), ("musicbrainz", "cover_url")],
                summary_sources: ctx.pick(&[("bangumi", "summary")], &[], &[]),
                date_sources: &[],
                statuses: None,
                name_based: true,
                primary_date: PrimaryDate::None,
                started_date: false,
                completed_date: false,
                progress: None,
                list: None,
                extras: PERSON_EXTRAS,
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
                external_types: &[],
                title_sources: &[],
                original_title: None,
                cover_sources: &[],
                summary_sources: &[],
                date_sources: &[],
                statuses: None,
                name_based: true,
                primary_date: PrimaryDate::None,
                started_date: false,
                completed_date: false,
                progress: None,
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
                external_types: &[("bangumi", &["character"])],
                title_sources: ctx.pick(
                    &[("bangumi", "name_cn")],
                    &[("bangumi", "name")],
                    &[("bangumi", "name")],
                ),
                original_title: None,
                cover_sources: &[("bangumi", "cover_url")],
                summary_sources: ctx.pick(&[("bangumi", "summary")], &[], &[]),
                date_sources: &[],
                statuses: None,
                name_based: true,
                primary_date: PrimaryDate::None,
                started_date: false,
                completed_date: false,
                progress: None,
                list: None,
                extras: PERSON_EXTRAS,
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
                // NeoDB performances (theatre, musicals, stage plays) are the
                // one catalog covering live events, but its coverage is
                // overwhelmingly Chinese — seed it only for zh; every other
                // language starts provider-less, like the hub types.
                providers: ctx.pick(&["neodb"], &[], &[]),
                external_types: ctx.pick(&[("neodb", &["performance"])], &[], &[]),
                title_sources: ctx.pick(&[("neodb", "title")], &[], &[]),
                original_title: ctx.pick(Some(&[("neodb", "original_title")]), None, None),
                cover_sources: ctx.pick(&[("neodb", "cover_url")], &[], &[]),
                summary_sources: ctx.pick(&[("neodb", "description")], &[], &[]),
                // The event date is the *attendance* date (role Event); the
                // production's run start is still the best default a catalog
                // can offer, so seed it and let the user adjust on review.
                date_sources: ctx.pick(&[("neodb", "opening_date")], &[], &[]),
                statuses: Some(&EVENT_STATUS),
                name_based: false,
                primary_date: PrimaryDate::EventDate,
                started_date: false,
                completed_date: false,
                progress: None,
                list: None,
                extras: EVENT_EXTRAS,
                relations: ctx.pick(
                    &[EVENT_ARTIST_REL, FRANCHISE_REL],
                    &[EVENT_ARTIST_PLAIN_REL, FRANCHISE_REL],
                    &[EVENT_ARTIST_PLAIN_REL, FRANCHISE_REL],
                ),
                log_hashtag: Some(l("Event", "イベント", "活动", "活動")),
            },
        ),
    ]
}

/// A relation field a preset seeds: field name (never localized), display label,
/// target preset id. When the target type is absent at resolve time, a spec
/// *with* sources becomes a plain text-list field instead (the provider names
/// survive as text); one without sources is dropped.
struct RelationSpec {
    field: &'static str,
    label: L,
    target: &'static str,
    sources: Sources,
}

const FRANCHISE_REL: RelationSpec = RelationSpec {
    field: "franchise",
    label: l("Franchise", "シリーズ", "系列", "系列"),
    target: "franchise",
    sources: &[],
};
const GAMES_FRANCHISE_REL: RelationSpec = RelationSpec {
    field: "franchise",
    label: l("Franchise", "シリーズ", "系列", "系列"),
    target: "franchise",
    sources: &[("igdb", "franchise")],
};
const BOOKS_FRANCHISE_REL: RelationSpec = RelationSpec {
    field: "franchise",
    label: l("Franchise", "シリーズ", "系列", "系列"),
    target: "franchise",
    sources: &[("neodb", "series")],
};
const MUSIC_ARTIST_REL: RelationSpec = RelationSpec {
    field: "artist",
    label: l("Artist", "アーティスト", "艺术家", "藝術家"),
    target: "artist",
    sources: &[
        ("musicbrainz", "artists"),
        ("applemusic", "artists"),
        ("discogs", "artists"),
        ("neodb", "artists"),
    ],
};
const PODCAST_HOST_REL: RelationSpec = RelationSpec {
    field: "host",
    label: l("Host", "ホスト", "主播", "主持人"),
    target: "artist",
    sources: &[("applepodcast", "host"), ("neodb", "hosts")],
};
const EVENT_ARTIST_REL: RelationSpec = RelationSpec {
    field: "artist",
    label: l("Artist", "アーティスト", "艺术家", "藝術家"),
    target: "artist",
    sources: &[("neodb", "performers")],
};
/// [`EVENT_ARTIST_REL`] without the NeoDB wiring — the event preset outside zh
/// ships provider-less, so its artist link is a bare relation.
const EVENT_ARTIST_PLAIN_REL: RelationSpec = RelationSpec {
    field: "artist",
    label: l("Artist", "アーティスト", "艺术家", "藝術家"),
    target: "artist",
    sources: &[],
};
const GROUPS_REL: RelationSpec = RelationSpec {
    field: "groups",
    label: l("Groups", "グループ", "团体", "團體"),
    target: "artist",
    sources: &[],
};
const VOICE_BY_REL: RelationSpec = RelationSpec {
    field: "voice_by",
    label: l("Voice by", "CV", "配音", "配音"),
    target: "artist",
    sources: &[("bangumi", "voice_actors")],
};

/// A status vocabulary: the enum options (with their canonical-status mapping)
/// plus the title template for the default in-progress smart list.
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

const UPCOMING_SHELF: L = l(
    "Upcoming {label}",
    "今後の{label}",
    "即将到来的{label}",
    "即將到來的{label}",
);

const EPISODES_LIST: ListSpec = ListSpec {
    heading: l("Episodes", "エピソード", "剧集", "劇集"),
    tracking: EpisodeTracking::Checklist,
};
const CHAPTERS_LIST: ListSpec = ListSpec {
    heading: l("Chapters", "チャプター", "章节", "章節"),
    tracking: EpisodeTracking::Checklist,
};
const PODCAST_EPISODES_LIST: ListSpec = ListSpec {
    heading: l("Episodes", "エピソード", "单集", "單集"),
    tracking: EpisodeTracking::Checklist,
};
const TRACKS_LIST: ListSpec = ListSpec {
    heading: l("Tracks", "トラック", "曲目", "曲目"),
    tracking: EpisodeTracking::None,
};

// Numeric frontmatter progress is deliberately independent from provider-backed
// body lists. A type may seed both (Anime), progress only (Books), list only
// (Podcasts/Music), or neither.
const ANIME_PROGRESS: ProgressSpec = ProgressSpec {
    total_field: "episodes",
    total_label: l("Episodes", "話数", "总集数", "總集數"),
    total_sources: &[
        ("bangumi", "eps"),
        ("anilist", "episodes"),
        ("myanimelist", "episodes"),
        ("tmdb", "episode_count"),
    ],
};
const DRAMA_PROGRESS: ProgressSpec = ProgressSpec {
    total_field: "episodes",
    total_label: l("Episodes", "話数", "总集数", "總集數"),
    total_sources: &[("tmdb", "episode_count"), ("neodb", "episode_count")],
};
const BOOK_PROGRESS: ProgressSpec = ProgressSpec {
    total_field: "pages",
    total_label: l("Pages", "ページ数", "总页数", "總頁數"),
    total_sources: &[
        ("neodb", "pages"),
        ("openlibrary", "pages"),
        ("googlebooks", "pages"),
        ("hardcover", "pages"),
    ],
};
const MANGA_PROGRESS: ProgressSpec = ProgressSpec {
    total_field: "chapters",
    total_label: l("Chapters", "話数", "话数", "話數"),
    total_sources: &[
        ("bangumi", "eps"),
        ("mangaupdates", "latest_chapter"),
        ("anilist", "chapters"),
        ("myanimelist", "chapters"),
        ("comicvine", "issues_count"),
    ],
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

    /// Per-language wiring pick, keyed on the *stamped* title language's bare
    /// primary subtag — the wiring analogue of [`L`] for text. Bangumi is
    /// structurally zh/ja data (`name_cn` Chinese, `name` the Japanese
    /// original, summaries Chinese), so bangumi-involving presets vary their
    /// provider priority and bangumi title/summary slots by language; `other`
    /// is also right for ko/fr/… since TMDB/TheTVDB/MAL localize per request.
    fn pick<T>(&self, zh: T, ja: T, other: T) -> T {
        match self.lang.as_str() {
            "zh" => zh,
            "ja" => ja,
            _ => other,
        }
    }
}

#[derive(Clone, Copy)]
enum PrimaryDate {
    None,
    Season,
    ReleaseDate,
    EventDate,
}

/// An additional seeded field. The variants mirror [`FieldType`] — nothing
/// about the field's *meaning* lives here; the field name, label, and wiring
/// are spelled out literally at each preset's declaration.
#[derive(Clone, Copy)]
enum Extra {
    Text {
        field: &'static str,
        label: L,
        sources: Sources,
    },
    TextList {
        field: &'static str,
        label: L,
        sources: Sources,
    },
    Date {
        field: &'static str,
        label: L,
        sources: Sources,
    },
    TotalProgress {
        field: &'static str,
        label: L,
        sources: Sources,
    },
}

const GAMES_EXTRAS: &[Extra] = &[
    Extra::TextList {
        field: "platform",
        label: l("Platform", "プラットフォーム", "平台", "平台"),
        sources: &[
            ("igdb", "platforms"),
            ("steam", "platform"),
            ("neodb", "platforms"),
        ],
    },
    Extra::TextList {
        field: "genre",
        label: l("Genre", "ジャンル", "类型", "類型"),
        sources: &[
            ("igdb", "genres"),
            ("steam", "genres"),
            ("bangumi", "genre"),
            ("neodb", "genres"),
        ],
    },
];

const BOARD_EXTRAS: &[Extra] = &[
    Extra::Text {
        field: "players",
        label: l("Players", "プレイ人数", "玩家人数", "玩家人數"),
        sources: &[("bgg", "players")],
    },
    Extra::Text {
        field: "playtime",
        label: l("Playtime", "プレイ時間", "游玩时长", "遊玩時長"),
        sources: &[("bgg", "playtime")],
    },
];

const BOOKS_EXTRAS: &[Extra] = &[
    Extra::TextList {
        field: "author",
        label: l("Author", "著者", "作者", "作者"),
        sources: &[
            ("neodb", "authors"),
            ("openlibrary", "authors"),
            ("googlebooks", "authors"),
            ("hardcover", "authors"),
        ],
    },
    Extra::Text {
        field: "isbn",
        label: l("ISBN", "ISBN", "ISBN", "ISBN"),
        sources: &[
            ("neodb", "isbn"),
            ("openlibrary", "isbn"),
            ("googlebooks", "isbn"),
            ("hardcover", "isbn"),
        ],
    },
];

const MANGA_EXTRAS: &[Extra] = &[Extra::TextList {
    field: "author",
    label: l("Author", "著者", "作者", "作者"),
    sources: &[("bangumi", "author"), ("mangaupdates", "authors")],
}];

/// Artists and characters share the one Bangumi-backed birthday field.
const PERSON_EXTRAS: &[Extra] = &[Extra::Date {
    field: "birthday",
    label: l("Birthday", "誕生日", "生日", "生日"),
    sources: &[("bangumi", "birthday")],
}];

const EVENT_EXTRAS: &[Extra] = &[Extra::Text {
    field: "location",
    label: l("Location", "場所", "地点", "地點"),
    sources: &[],
}];

const MUSIC_EXTRAS: &[Extra] = &[
    Extra::TextList {
        field: "owned",
        label: l("Owned", "所持形式", "收藏形式", "收藏形式"),
        sources: &[("discogs", "format"), ("neodb", "format")],
    },
    Extra::TotalProgress {
        field: "track_count",
        label: l("Track count", "曲数", "曲目数", "曲目數"),
        sources: &[("applemusic", "track_count")],
    },
    Extra::TextList {
        field: "genres",
        label: l("Genres", "ジャンル", "流派", "曲風"),
        sources: &[
            ("musicbrainz", "genres"),
            ("applemusic", "genre"),
            ("discogs", "genres"),
            ("bangumi", "genre"),
            ("neodb", "genres"),
        ],
    },
    Extra::TextList {
        field: "styles",
        label: l("Styles", "スタイル", "风格", "風格"),
        sources: &[("discogs", "styles")],
    },
];

// Hosts are deliberately *not* an extra here: PODCAST_HOST_REL carries the
// same wiring, binding to the artist type when present and falling back to a
// text list when not — one field either way, never both.
const PODCAST_EXTRAS: &[Extra] = &[
    Extra::TextList {
        field: "genres",
        label: l("Genres", "ジャンル", "类型", "類型"),
        sources: &[("applepodcast", "genre"), ("neodb", "genres")],
    },
    Extra::Text {
        field: "feed_url",
        label: l("Feed URL", "フィードURL", "订阅源 URL", "訂閱來源 URL"),
        sources: &[("applepodcast", "feed_url")],
    },
];

struct ListSpec {
    heading: L,
    tracking: EpisodeTracking,
}

struct ProgressSpec {
    total_field: &'static str,
    total_label: L,
    total_sources: Sources,
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
    /// Per-provider `externalTypes` constraint for the generated external-ref
    /// fields, `(provider id, type values)`. A preset knows its subject, so it
    /// pins multi-type providers here and searches do not return unrelated media
    /// kinds. Providers absent from this list fall back to the catalog default.
    /// Every value must be one of the provider's type options (registry test
    /// enforced).
    external_types: &'static [(&'static str, &'static [&'static str])],
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
    /// `None` → no status field (people/hub types).
    statuses: Option<&'static StatusVocab>,
    /// People/hub types: the filename and primary title use the
    /// language-neutral *original* name instead of the chosen title language,
    /// and there is no rating field. Explicit — meaning is never inferred from
    /// the shape of the other fields.
    name_based: bool,
    primary_date: PrimaryDate,
    /// Whether logging a `started` activity stamps a dedicated date field.
    started_date: bool,
    completed_date: bool,
    /// Optional scalar `progress` + total field pair. Independent from `list`:
    /// this is for users who prefer a simple number over checking every item.
    progress: Option<&'static ProgressSpec>,
    /// Optional provider-backed episode/chapter/track section in the Markdown
    /// body. This does not imply or derive any scalar progress fields.
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

    if let Some(progress) = spec.progress {
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
        total.external_fields = source_mappings(progress.total_sources);
        fields.push(total);
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
            date.external_fields = source_mappings(spec.date_sources);
            fields.push(date);
        }
    }

    if spec.started_date {
        let mut started = field(
            "started_date",
            FieldType::Date,
            ctx.text(l("Started date", "開始日", "开始日期", "開始日期")),
        );
        started.date_role = Some(DateRole::Started);
        fields.push(started);
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
            external.external_types = spec
                .external_types
                .iter()
                .find(|(id, _)| id == provider)
                .map(|(_, types)| types.iter().map(|t| (*t).to_string()).collect())
                .unwrap_or_else(|| item.default_external_types.clone());
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
        relation.external_fields = source_mappings(relation_spec.sources);
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
    let (name, field_type, label, sources) = match extra {
        Extra::Text {
            field,
            label,
            sources,
        } => (field, FieldType::Text, label, sources),
        Extra::TextList {
            field,
            label,
            sources,
        } => (field, FieldType::TextList, label, sources),
        Extra::Date {
            field,
            label,
            sources,
        } => (field, FieldType::Date, label, sources),
        Extra::TotalProgress {
            field,
            label,
            sources,
        } => (field, FieldType::TotalProgress, label, sources),
    };
    let mut f = field(name, field_type, ctx.text(label));
    f.external_fields = source_mappings(sources);
    f
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

/// The default upcoming shelf for a type with an event date and a status role
/// that maps at least one planning value. Events are sorted soonest-first and
/// exclude past dates, so the initial Home highlights what is actually ahead.
fn upcoming_event_list_for(config: &EntityTypeConfig, locale: SeedLocale) -> Option<SuggestedList> {
    let event = config
        .fields
        .iter()
        .find(|field| field.date_role == Some(DateRole::Event))?;
    let status = config
        .fields
        .iter()
        .find(|field| field.enum_role == Some(EnumRole::Status))?;
    let planning = status
        .status_values
        .as_ref()
        .map(|values| values.planning.as_slice())
        .unwrap_or_default();
    planning.first()?;

    let date_rule = SmartFilterRule {
        kind: SmartFilterRuleKind::Compare,
        field: Some(event.field.clone()),
        op: Some(SmartCompareOp::Gte),
        relative: Some(SmartRelativeDate {
            amount: 0,
            unit: SmartDurationUnit::Days,
            future: false,
        }),
        ..Default::default()
    };
    let criteria = status_criteria(&status.field, planning, vec![date_rule]);
    Some(SuggestedList {
        id: format!("upcoming-{}", config.id),
        title: UPCOMING_SHELF.get(locale).replace("{label}", &config.label),
        entity_type: config.id.clone(),
        criteria: Some(criteria),
        sort: suggested_sort(Some(&event.field), SortDirection::Asc),
    })
}

/// The default "in progress" suggested smart list ("Watching Anime", "Playing Games")
/// for a type whose status **role** maps at least one ongoing value. The
/// *criteria* — field name and matched values — derive entirely from the role's
/// `statusValues` mapping, never from field or option names. Only the shelf
/// *title* comes from `shelf_template` (the status vocabulary's per-language
/// pattern), because natural titles can't be composed from a label across
/// languages. Types with no ongoing status (events: planned/attended) get no
/// such shelf.
fn ongoing_list_for(
    config: &EntityTypeConfig,
    shelf_template: Option<&str>,
) -> Option<SuggestedList> {
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
    let criteria = status_criteria(&status.field, ongoing, Vec::new());
    // Newest release first when the type has a planning date (season, release
    // date); otherwise the most recently touched file leads.
    let sort = config
        .fields
        .iter()
        .find(|field| field.date_role == Some(DateRole::Planning))
        .map(|field| field.field.as_str());
    Some(SuggestedList {
        id: format!("ongoing-{}", config.id),
        title: template.replace("{label}", &config.label),
        entity_type: config.id.clone(),
        criteria: Some(criteria),
        sort: suggested_sort(sort, SortDirection::Desc),
    })
}

/// Criteria matching `values` on a status field, in the canonical shapes the
/// settings editor produces: one equality rule (followed by `extra_rules`) when
/// a single value maps to the bucket, or an "any of" subgroup alongside
/// `extra_rules` when several do.
fn status_criteria(
    status_field: &str,
    values: &[String],
    extra_rules: Vec<SmartFilterRule>,
) -> SmartFilterGroup {
    let rule = |value: &String| SmartFilterRule {
        kind: SmartFilterRuleKind::Compare,
        field: Some(status_field.to_string()),
        op: Some(SmartCompareOp::Eq),
        value: Some(value.clone()),
        ..Default::default()
    };
    if let [value] = values {
        let mut rules = vec![rule(value)];
        rules.extend(extra_rules);
        SmartFilterGroup {
            conjunction: SmartFilterConjunction::All,
            rules,
            groups: Vec::new(),
        }
    } else {
        SmartFilterGroup {
            conjunction: SmartFilterConjunction::All,
            rules: extra_rules,
            groups: vec![SmartFilterSubgroup {
                conjunction: SmartFilterConjunction::Any,
                rules: values.iter().map(rule).collect(),
            }],
        }
    }
}

/// A default "Recent {label}" suggested smart list for a type — but **only** when the type
/// has a release/completion date to sort by. A chronological shelf is meaningless
/// for types with no such date (people, franchises) or whose only date is an
/// attendance date (events have their dedicated upcoming shelf), so those do not
/// fall back to a title-sorted "recent" shelf that isn't really recent.
fn recent_list_for(config: &EntityTypeConfig, locale: SeedLocale) -> Option<SuggestedList> {
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
    Some(SuggestedList {
        id: format!("recent-{}", config.id),
        title: RECENT_SHELF.get(locale).replace("{label}", &config.label),
        entity_type: config.id.clone(),
        criteria: None,
        sort: suggested_sort(Some(&date_field.field), SortDirection::Desc),
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

    fn find_type<'a>(result: &'a ResolveTypePresetsResponse, id: &str) -> &'a EntityTypeConfig {
        result
            .types
            .iter()
            .find(|type_config| type_config.id == id)
            .unwrap_or_else(|| panic!("missing resolved type {id}"))
    }

    fn assert_mapping(field: &FieldConfig, source: &str, source_field: &str) {
        assert!(
            field
                .external_fields
                .iter()
                .any(|mapping| mapping.source == source && mapping.field == source_field),
            "{}.{} should map from {source}.{source_field}",
            field.field,
            field
                .display_name
                .as_deref()
                .unwrap_or("unnamed preset field")
        );
    }

    fn mapping_pairs(field: &FieldConfig) -> Vec<(&str, &str)> {
        field
            .external_fields
            .iter()
            .map(|mapping| (mapping.source.as_str(), mapping.field.as_str()))
            .collect()
    }

    fn assert_external_types(config: &EntityTypeConfig, provider: &str, expected: &[&str]) {
        let field = find_field(config, &format!("{provider}_url"))
            .unwrap_or_else(|| panic!("{}.{} external ref missing", config.id, provider));
        assert_eq!(
            field.external_types,
            expected
                .iter()
                .map(|value| value.to_string())
                .collect::<Vec<_>>()
        );
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
        // Wiring varies per language, so every branch is validated.
        for language in [None, Some("zh"), Some("ja")] {
            validate_registry_against_catalog(language);
        }
    }

    fn validate_registry_against_catalog(language: Option<&str>) {
        let ctx = BuildCtx::new(language);
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
                    let item = ctx
                        .catalog
                        .iter()
                        .find(|item| item.id == *provider)
                        .unwrap_or_else(|| {
                            panic!(
                                "{}.{}: externalRef {provider} not in the catalog",
                                config.id, field.field
                            )
                        });
                    // The type constraint is literal per preset too: every
                    // value must be one the provider actually offers.
                    for external_type in &field.external_types {
                        assert!(
                            item.types
                                .iter()
                                .any(|option| option.value == *external_type),
                            "{}.{}: {} declares no external type {:?}",
                            config.id,
                            field.field,
                            provider,
                            external_type
                        );
                    }
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
    fn wiring_varies_with_the_language() {
        // Bangumi is zh/ja data: zh leads with it and maps `name_cn`; ja keeps
        // it but maps the Japanese `name`; every other language demotes it in
        // the search priority and drops it from the title (TMDB/MAL localize
        // per request instead). Its Chinese summaries are zh-only.
        let zh = resolve(vec![], &["anime"], Some("zh-Hant")).types.remove(0);
        assert_eq!(zh.external_priority[0], "bangumi");
        let title = find_field(&zh, "title").unwrap();
        assert_eq!(title.external_fields[0].source, "bangumi");
        assert_eq!(title.external_fields[0].field, "name_cn");
        assert!(zh
            .body_sections
            .iter()
            .any(|s| s.external_fields.iter().any(|m| m.source == "bangumi")));

        let ja = resolve(vec![], &["anime"], Some("ja")).types.remove(0);
        assert_eq!(ja.external_priority[0], "bangumi");
        let title = find_field(&ja, "title").unwrap();
        assert_eq!(title.external_fields[0].source, "bangumi");
        assert_eq!(title.external_fields[0].field, "name");

        let en = resolve(vec![], &["anime"], Some("en")).types.remove(0);
        assert_eq!(en.external_priority[0], "anilist");
        assert_eq!(
            en.external_priority.last().map(String::as_str),
            Some("bangumi")
        );
        let title = find_field(&en, "title").unwrap();
        assert!(title.external_fields.iter().all(|m| m.source != "bangumi"));
        // Bangumi still wires covers and the external-ref field for everyone.
        let cover = find_field(&en, "cover_url").unwrap();
        assert!(cover.external_fields.iter().any(|m| m.source == "bangumi"));
        assert!(find_field(&en, "bangumi_url").is_some());
        // NeoDB provides a language-independent fallback summary for Music.
        let music = resolve(vec![], &["music"], Some("en")).types.remove(0);
        assert!(music
            .body_sections
            .iter()
            .any(|s| s.kind == BodySectionKind::External
                && s.external_fields
                    .iter()
                    .any(|mapping| mapping.source == "neodb" && mapping.field == "description")));
        // Characters fall back to the Japanese original name, never Chinese.
        let character = resolve(vec![], &["character"], Some("ko")).types.remove(0);
        let title = find_field(&character, "title").unwrap();
        assert_eq!(title.external_fields[0].field, "name");
    }

    #[test]
    fn event_preset_is_neodb_backed_only_for_zh() {
        // NeoDB's performance catalog is overwhelmingly Chinese, so only zh
        // seeds it — including the run's opening date as the event-date
        // default (the user adjusts it to the attended date on review).
        let zh = resolve(vec![], &["event"], Some("zh-Hans")).types.remove(0);
        assert_eq!(zh.external_priority, vec!["neodb".to_string()]);
        assert!(find_field(&zh, "neodb_url").is_some());
        assert!(find_field(&zh, "title_original").is_some());
        let date = find_field(&zh, "date").expect("event date");
        assert_eq!(date.date_role, Some(DateRole::Event));
        assert_mapping(date, "neodb", "opening_date");

        for language in [None, Some("ja"), Some("en")] {
            let other = resolve(vec![], &["event"], language).types.remove(0);
            assert!(
                other.external_priority.is_empty(),
                "{language:?} event preset should be provider-less"
            );
            assert!(find_field(&other, "neodb_url").is_none());
            assert!(find_field(&other, "title_original").is_none());
            let date = find_field(&other, "date").expect("event date");
            assert!(date.external_fields.is_empty());
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
    fn wired_relation_falls_back_to_a_text_list_when_target_absent() {
        // Music alone: no artist type, so the provider-wired artist relation
        // keeps its wiring as a plain text list instead of vanishing.
        let music = resolve(vec![], &["music"], None).types.remove(0);
        let artist = find_field(&music, "artist").expect("artist fallback");
        assert_eq!(artist.field_type, FieldType::TextList);
        assert!(artist.relation_type.is_none());
        assert_mapping(artist, "musicbrainz", "artists");

        // Same for event performers (zh — elsewhere the event preset is
        // provider-less) and the credit-wired franchise links.
        let event = resolve(vec![], &["event"], Some("zh")).types.remove(0);
        let performer = find_field(&event, "artist").expect("performer fallback");
        assert_eq!(performer.field_type, FieldType::TextList);
        assert_mapping(performer, "neodb", "performers");

        let books = resolve(vec![], &["books"], None).types.remove(0);
        let series = find_field(&books, "franchise").expect("series fallback");
        assert_eq!(series.field_type, FieldType::TextList);
        assert_mapping(series, "neodb", "series");
    }

    #[test]
    fn podcast_hosts_seed_exactly_one_field() {
        // Alone: the host relation falls back to a text list; there is no
        // second hosts field duplicating the same wiring.
        let podcast = resolve(vec![], &["podcast"], None).types.remove(0);
        let host = find_field(&podcast, "host").expect("host fallback");
        assert_eq!(host.field_type, FieldType::TextList);
        assert_mapping(host, "applepodcast", "host");
        assert_mapping(host, "neodb", "hosts");
        assert!(find_field(&podcast, "hosts").is_none());

        // With the artist preset co-selected: a real relation, still one field.
        let result = resolve(vec![], &["podcast", "artist"], None);
        let podcast = find_type(&result, "podcast");
        let host = find_field(podcast, "host").expect("host relation");
        assert_eq!(host.field_type, FieldType::Relation);
        assert_eq!(host.relation_type.as_deref(), Some("artist"));
        assert!(find_field(podcast, "hosts").is_none());
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
        assert_eq!(
            suggested_lists(&result.types, Some("zh-Hant"))[0].title,
            "在看的動畫"
        );
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
        assert_eq!(
            suggested_lists(&ja.types, Some("ja"))[0].title,
            "プレイ中のゲーム"
        );
    }

    #[test]
    fn unwritten_languages_keep_titles_but_fall_back_to_english_text() {
        // Korean isn't a language the presets are written in: the title
        // language is still honored (ko titles/filenames), only the text is
        // English. Absent language → English throughout.
        let ko = resolve(vec![], &["anime"], Some("ko"));
        assert_eq!(ko.types[0].label, "Anime");
        assert_eq!(
            suggested_lists(&ko.types, Some("ko"))[0].title,
            "Watching Anime"
        );
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
        // Chinese wiring — the branch where Bangumi feeds the title.
        let anime = resolve(vec![], &["anime"], Some("zh-Hans")).types.remove(0);
        // Title maps from Bangumi's localized name.
        let title = find_field(&anime, "title").unwrap();
        assert!(title
            .external_fields
            .iter()
            .any(|m| m.source == "bangumi" && m.field == "name_cn"));
        // External-ref field carries the provider id and the preset's type
        // constraint — Bangumi is multi-type, so anime pins subject type 2.
        let bgm = find_field(&anime, "bangumi_url").expect("bangumi_url");
        assert_eq!(bgm.external_ref.as_deref(), Some("bangumi"));
        assert_eq!(bgm.field_type, FieldType::ExternalRef);
        assert_eq!(bgm.external_types, ["2"]);
        // Summary body section wired from providers.
        assert!(anime
            .body_sections
            .iter()
            .any(|s| s.kind == BodySectionKind::External && !s.external_fields.is_empty()));
    }

    #[test]
    fn provider_priorities_and_type_constraints_match_each_domain() {
        for (preset_id, neodb_type) in [
            ("drama", "tv"),
            ("movie", "movie"),
            ("games", "game"),
            ("music", "music"),
            ("podcast", "podcast"),
        ] {
            let config = resolve(vec![], &[preset_id], None).types.remove(0);
            assert_external_types(&config, "neodb", &[neodb_type]);
        }

        // Key-less fallbacks sit at the bottom for a non-zh/ja user: NeoDB
        // last where Bangumi isn't wired, and Bangumi (structurally zh/ja
        // data) below even NeoDB where it is. zh keeps Bangumi above NeoDB.
        for preset_id in ["drama", "movie", "podcast", "books"] {
            let config = resolve(vec![], &[preset_id], None).types.remove(0);
            assert_eq!(
                config.external_priority.last().map(String::as_str),
                Some("neodb"),
                "{preset_id} should keep NeoDB at lowest priority"
            );
        }
        for preset_id in ["games", "music"] {
            let config = resolve(vec![], &[preset_id], None).types.remove(0);
            assert_eq!(
                config.external_priority.last().map(String::as_str),
                Some("bangumi"),
                "{preset_id} should demote Bangumi below NeoDB for non-zh/ja"
            );
            let zh = resolve(vec![], &[preset_id], Some("zh")).types.remove(0);
            assert_eq!(
                zh.external_priority.last().map(String::as_str),
                Some("neodb"),
                "{preset_id} should keep Bangumi above NeoDB for zh"
            );
        }

        // Books: NeoDB's catalog skews Chinese, so it leads only for zh.
        let books_zh = resolve(vec![], &["books"], Some("zh")).types.remove(0);
        assert_eq!(
            books_zh.external_priority.first().map(String::as_str),
            Some("neodb")
        );
        let books_ja = resolve(vec![], &["books"], Some("ja")).types.remove(0);
        assert_eq!(
            books_ja.external_priority.last().map(String::as_str),
            Some("neodb")
        );

        for preset_id in ["drama", "movie", "books"] {
            let config = resolve(vec![], &[preset_id], None).types.remove(0);
            assert!(!config.external_priority.iter().any(|id| id == "bangumi"));
            assert!(find_field(&config, "bangumi_url").is_none());
        }
        for preset_id in ["anime", "games", "manga", "music", "character", "artist"] {
            let config = resolve(vec![], &[preset_id], None).types.remove(0);
            assert!(config.external_priority.iter().any(|id| id == "bangumi"));
        }

        let music = resolve(vec![], &["music"], None).types.remove(0);
        assert_external_types(&music, "discogs", &["release"]);
        let board_games = resolve(vec![], &["board"], None).types.remove(0);
        assert_external_types(&board_games, "bgg", &["boardgame", "boardgameexpansion"]);
        assert_mapping(
            find_field(&board_games, "players").unwrap(),
            "bgg",
            "players",
        );
        assert_mapping(
            find_field(&board_games, "playtime").unwrap(),
            "bgg",
            "playtime",
        );
    }

    #[test]
    fn localized_and_original_title_sources_keep_their_semantics() {
        for language in [None, Some("zh"), Some("ja")] {
            for preset_id in ["anime", "drama", "movie"] {
                let config = resolve(vec![], &[preset_id], language).types.remove(0);
                let title = find_field(&config, "title").unwrap();
                assert!(
                    mapping_pairs(title)
                        .iter()
                        .all(|mapping| *mapping != ("thetvdb", "name")),
                    "{preset_id} must use TheTVDB's localized candidate title fallback"
                );
            }
        }

        let anime = resolve(vec![], &["anime"], None).types.remove(0);
        assert_eq!(
            mapping_pairs(find_field(&anime, "title_original").unwrap()),
            vec![
                ("bangumi", "name"),
                ("anilist", "native_title"),
                ("tmdb", "original_title"),
                ("thetvdb", "name")
            ]
        );

        for preset_id in ["drama", "movie"] {
            let config = resolve(vec![], &[preset_id], None).types.remove(0);
            assert_eq!(
                mapping_pairs(find_field(&config, "title_original").unwrap()),
                vec![
                    ("tmdb", "original_title"),
                    ("thetvdb", "name"),
                    ("neodb", "original_title")
                ]
            );
        }

        let books = resolve(vec![], &["books"], None).types.remove(0);
        assert_eq!(
            mapping_pairs(find_field(&books, "title_original").unwrap()),
            vec![("neodb", "original_title")]
        );
        let manga = resolve(vec![], &["manga"], None).types.remove(0);
        assert_eq!(
            mapping_pairs(find_field(&manga, "title_original").unwrap()),
            vec![("bangumi", "name"), ("anilist", "native_title")]
        );
    }

    #[test]
    fn started_dates_are_seeded_only_for_progressive_media() {
        for preset_id in ["anime", "drama", "games", "books", "manga"] {
            let config = resolve(vec![], &[preset_id], None).types.remove(0);
            let started = find_field(&config, "started_date").expect("started date");
            assert_eq!(started.field_type, FieldType::Date);
            assert_eq!(started.date_role, Some(DateRole::Started));
        }
        for preset_id in [
            "movie",
            "board",
            "music",
            "podcast",
            "artist",
            "franchise",
            "character",
            "event",
        ] {
            let config = resolve(vec![], &[preset_id], None).types.remove(0);
            assert!(
                find_field(&config, "started_date").is_none(),
                "{preset_id} should not seed a started date"
            );
        }
    }

    #[test]
    fn open_taxonomies_and_new_metadata_fields_preserve_provider_values() {
        let games = resolve(vec![], &["games"], None).types.remove(0);
        for field_name in ["platform", "genre"] {
            let taxonomy = find_field(&games, field_name).expect("game taxonomy");
            assert_eq!(taxonomy.field_type, FieldType::TextList);
            assert!(taxonomy.enum_options.is_empty());
        }

        let music = resolve(vec![], &["music"], None).types.remove(0);
        for field_name in ["owned", "genres", "styles"] {
            let taxonomy = find_field(&music, field_name).expect("music taxonomy");
            assert_eq!(taxonomy.field_type, FieldType::TextList);
            assert!(taxonomy.enum_options.is_empty());
        }
        assert_mapping(find_field(&music, "owned").unwrap(), "discogs", "format");
        assert_mapping(find_field(&music, "owned").unwrap(), "neodb", "format");
        assert_mapping(
            find_field(&music, "track_count").unwrap(),
            "applemusic",
            "track_count",
        );
        assert_mapping(find_field(&music, "genres").unwrap(), "discogs", "genres");
        assert_mapping(find_field(&music, "styles").unwrap(), "discogs", "styles");

        let podcast = resolve(vec![], &["podcast"], None).types.remove(0);
        assert_eq!(
            find_field(&podcast, "genres").unwrap().field_type,
            FieldType::TextList
        );
        assert_eq!(
            find_field(&podcast, "feed_url").unwrap().field_type,
            FieldType::Text
        );
        assert_mapping(find_field(&podcast, "genres").unwrap(), "neodb", "genres");
        assert_mapping(
            find_field(&podcast, "feed_url").unwrap(),
            "applepodcast",
            "feed_url",
        );
        assert!(podcast.body_sections.iter().any(|section| {
            section.kind == BodySectionKind::External
                && section
                    .external_fields
                    .iter()
                    .any(|mapping| mapping.source == "neodb" && mapping.field == "description")
        }));
    }

    #[test]
    fn provider_credits_map_into_relations_when_targets_survive_resolution() {
        for (preset_id, target_id, field_name, language, expected) in [
            (
                "games",
                "franchise",
                "franchise",
                None,
                &[("igdb", "franchise")][..],
            ),
            (
                "books",
                "franchise",
                "franchise",
                None,
                &[("neodb", "series")][..],
            ),
            (
                "music",
                "artist",
                "artist",
                None,
                &[
                    ("musicbrainz", "artists"),
                    ("applemusic", "artists"),
                    ("discogs", "artists"),
                    ("neodb", "artists"),
                ][..],
            ),
            (
                "podcast",
                "artist",
                "host",
                None,
                &[("applepodcast", "host"), ("neodb", "hosts")][..],
            ),
            // NeoDB backs the event preset only for zh.
            (
                "event",
                "artist",
                "artist",
                Some("zh"),
                &[("neodb", "performers")][..],
            ),
            (
                "character",
                "artist",
                "voice_by",
                None,
                &[("bangumi", "voice_actors")][..],
            ),
        ] {
            let result = resolve(vec![], &[preset_id, target_id], language);
            let relation = find_field(find_type(&result, preset_id), field_name)
                .unwrap_or_else(|| panic!("{preset_id}.{field_name} relation missing"));
            assert_eq!(relation.relation_type.as_deref(), Some(target_id));
            for (source, source_field) in expected {
                assert_mapping(relation, source, source_field);
            }
        }
    }

    #[test]
    fn progress_totals_are_mapped_from_each_presets_providers() {
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

        // Manga's chapter count wires from MAL/MangaUpdates/Comic Vine.
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
        assert_mapping(chapters, "comicvine", "issues_count");

        // Books use the same scalar pair for current/total pages without
        // declaring a provider-backed chapter list.
        let books = resolve(vec![], &["books"], None).types.remove(0);
        let progress = find_field(&books, "progress").expect("book progress field");
        assert_eq!(progress.field_type, FieldType::Progress);
        assert_eq!(progress.total_progress_field.as_deref(), Some("pages"));
        let pages = find_field(&books, "pages").expect("book pages field");
        assert_eq!(pages.field_type, FieldType::TotalProgress);
        for provider in ["neodb", "googlebooks", "openlibrary", "hardcover"] {
            assert_mapping(pages, provider, "pages");
        }
        assert!(!books
            .body_sections
            .iter()
            .any(|section| section.kind == BodySectionKind::Episodes));
    }

    #[test]
    fn scalar_progress_and_provider_lists_are_independent() {
        // Anime opts into both independent features.
        let anime = resolve(vec![], &["anime"], None).types.remove(0);
        assert_eq!(
            find_field(&anime, "progress").and_then(|field| field.total_progress_field.as_deref()),
            Some("episodes")
        );
        assert!(anime
            .body_sections
            .iter()
            .any(|section| section.kind == BodySectionKind::Episodes));

        // Books opt into scalar page progress only; this is asserted in detail
        // above, and should remain list-free even though they have a total.
        let books = resolve(vec![], &["books"], None).types.remove(0);
        assert!(find_field(&books, "progress").is_some());
        assert!(find_field(&books, "pages").is_some());
        assert!(!books
            .body_sections
            .iter()
            .any(|section| section.kind == BodySectionKind::Episodes));

        // Podcasts opt into a provider-backed checklist only. Being open-ended
        // does not create scalar "x of y" fields.
        let podcast = resolve(vec![], &["podcast"], None).types.remove(0);
        assert!(find_field(&podcast, "progress").is_none());
        assert!(find_field(&podcast, "episodes").is_none());
        let list = podcast
            .body_sections
            .iter()
            .find(|s| s.kind == BodySectionKind::Episodes)
            .expect("episodes section");
        assert_eq!(list.tracking, Some(EpisodeTracking::Checklist));

        // Music also opts into a list only: imported tracks are plain rows, and
        // Apple Music's standalone track_count is not paired to a progress field.
        let music = resolve(vec![], &["music"], None).types.remove(0);
        assert!(find_field(&music, "progress").is_none());
        assert!(find_field(&music, "tracks").is_none());
        assert!(find_field(&music, "track_count").is_some());
        let list = music
            .body_sections
            .iter()
            .find(|s| s.kind == BodySectionKind::Episodes)
            .expect("tracks section");
        assert_eq!(list.tracking, Some(EpisodeTracking::None));

        // Movies opt into neither feature.
        let movie = resolve(vec![], &["movie"], None).types.remove(0);
        assert!(find_field(&movie, "progress").is_none());
        assert!(!movie
            .body_sections
            .iter()
            .any(|section| section.kind == BodySectionKind::Episodes));
    }

    #[test]
    fn one_suggested_list_per_type_preferring_ongoing() {
        // Anime has an ongoing shelf, Event has an upcoming shelf, and the
        // date-less Franchise type has no default shelf.
        let result = resolve(vec![], &["anime", "franchise", "event"], None);
        let suggestions = suggested_lists(&result.types, None);
        assert_eq!(suggestions.len(), 2);
        let ongoing = &suggestions[0];
        assert_eq!(ongoing.id, "ongoing-anime");
        assert_eq!(ongoing.entity_type, "anime");
        assert_eq!(ongoing.title, "Watching Anime");
        assert_eq!(ongoing.sort.property.as_str(), "note.season");

        let upcoming = &suggestions[1];
        assert_eq!(upcoming.id, "upcoming-event");
        assert_eq!(upcoming.entity_type, "event");
        assert_eq!(upcoming.title, "Upcoming Events");
        assert_eq!(upcoming.sort.property.as_str(), "note.date");
        assert_eq!(upcoming.sort.direction, SortDirection::Asc);
        let criteria = upcoming.criteria.as_ref().expect("upcoming criteria");
        assert!(criteria.rules.iter().any(|rule| {
            rule.field.as_deref() == Some("status") && rule.value.as_deref() == Some("Planned")
        }));
        assert!(criteria.rules.iter().any(|rule| {
            rule.field.as_deref() == Some("date")
                && rule.op == Some(SmartCompareOp::Gte)
                && rule.relative.as_ref().is_some_and(|relative| {
                    relative.amount == 0
                        && relative.unit == SmartDurationUnit::Days
                        && !relative.future
                })
        }));

        // The date-less type is still added — just shelf-less.
        assert!(result.types.iter().any(|t| t.id == "franchise"));
        assert!(result.types.iter().any(|t| t.id == "event"));
    }

    #[test]
    fn multi_type_resolve_yields_at_most_one_shelf_per_type() {
        // Every media preset maps an ongoing status, so a three-type onboarding
        // yields exactly three shelves — all in-progress ones, no "Recent"
        // duplicates bloating the default home.
        let result = resolve(vec![], &["anime", "games", "movie"], None);
        let suggestions = suggested_lists(&result.types, None);
        assert_eq!(suggestions.len(), 3);
        assert!(suggestions
            .iter()
            .all(|section| section.id.starts_with("ongoing-")));
    }

    #[test]
    fn ongoing_list_criteria_derive_from_the_status_role() {
        // The criteria come from the status role's ongoing mapping: field name
        // and value are the mapped data, nothing hardcoded. Only the shelf
        // *title* comes from the vocabulary's per-language template.
        let result = resolve(vec![], &["games"], None);
        let suggestions = suggested_lists(&result.types, None);
        let ongoing = suggestions
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
    fn suggestions_follow_custom_roles_and_values() {
        let mut config = resolve(vec![], &["anime"], None).types.remove(0);
        config.id = "custom".to_string();
        config.label = "Stories / tales".to_string();
        for field in &mut config.fields {
            if field.enum_role == Some(EnumRole::Status) {
                field.field = "進捗".to_string();
                field.status_values.as_mut().unwrap().ongoing = vec!["Doing".to_string()];
            }
            if field.date_role == Some(DateRole::Planning) {
                field.field = "when".to_string();
            }
        }
        let suggestions = suggested_lists(&[config], None);
        assert_eq!(suggestions.len(), 1);
        let suggestion = &suggestions[0];
        assert_eq!(suggestion.title, "In progress: Stories / tales");
        assert_eq!(suggestion.sort.property, "note.when");
        let rule = &suggestion.criteria.as_ref().unwrap().rules[0];
        assert_eq!(rule.field.as_deref(), Some("進捗"));
        assert_eq!(rule.value.as_deref(), Some("Doing"));
    }

    #[test]
    fn resolved_config_serializes_to_yaml() {
        let anime = resolve(vec![], &["anime"], Some("en")).types.remove(0);
        let yaml = serde_yaml::to_string(&anime).unwrap();
        assert!(yaml.contains("id: anime"));
        assert!(yaml.contains("enumRole: status"));
    }
}
