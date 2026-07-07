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
//! - **Title language.** Presets are language-neutral; the chosen language is
//!   stamped onto title fields, `filename`, and `seasonLanguage` at resolve time.
//! - **Relations.** A preset carries its relation fields, but a relation only
//!   *survives* when its target type is present — see the module's resolve rules.
//!
//! External-field wiring reuses the same provider catalog that backs
//! `/api/external/providers`, so a preset can never drift from the providers it
//! references.

use crate::api::external::provider_catalog_items;
use crate::contract::{
    ResolveTypePresetsRequest, ResolveTypePresetsResponse, TypePresetBackfill, TypePresetCategory,
    TypePresetCategoryInfo, TypePresetCollision, TypePresetProvider, TypePresetSummary,
    TypePresetsResponse,
};
use crate::types::{
    BodySection, BodySectionKind, CanonicalStatus, EntityTypeConfig, EnumRole, EpisodeTracking,
    ExternalFieldMapping, FieldConfig, FieldType, FilenameConfig, HomeSectionConfig,
    SeasonLanguage, SortDirection, StatusValues, TitleRole, TypeLogConfig,
};
use std::collections::HashSet;

// --- Public API ----------------------------------------------------------------

/// The full preset catalog for the picker: per-preset metadata plus the category
/// list in display order. The concrete configs are produced by [`resolve_presets`].
pub fn type_presets_response() -> TypePresetsResponse {
    // Metadata is language-agnostic, so a default context is fine for summaries.
    let ctx = BuildCtx::new(None);
    let presets = built_presets(&ctx);
    TypePresetsResponse {
        presets: presets.iter().map(|preset| preset.summary(&ctx)).collect(),
        categories: category_infos(),
    }
}

/// Materialize the selected presets into concrete types, merged against the
/// caller's current schema. Pure and stateless — onboarding passes an empty
/// `current_types`, the settings editor passes its in-progress types, and both
/// get the same relation wiring, back-fill proposals, and collision handling.
pub fn resolve_presets(request: &ResolveTypePresetsRequest) -> ResolveTypePresetsResponse {
    let ctx = BuildCtx::new(request.title_language.as_deref());
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
                return true; // links to an existing type as-is
            }
            if let Some(assigned_id) = assigned(target) {
                field.relation_type = Some(assigned_id.to_string()); // co-selected
                return true;
            }
            false // target absent → drop the relation field
        });
        if let Some(section) = home_section_for(&config) {
            home_sections.push(section);
        }
        types.push(config);
    }

    // 3. Back-fills: an *existing* type whose id matches a preset gains a link to a
    //    newly added type when its origin preset declares that relation and the
    //    live copy is missing the field. Recompute from the registry each time, so
    //    no provenance needs to be stored.
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

// --- The registry --------------------------------------------------------------

struct Preset {
    description: &'static str,
    /// Provider ids, in priority order, for the picker chips (the config already
    /// wires them via `externalPriority`/external-ref fields).
    provider_ids: &'static [&'static str],
    config: EntityTypeConfig,
}

impl Preset {
    fn summary(&self, ctx: &BuildCtx) -> TypePresetSummary {
        let providers = self
            .provider_ids
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
            category: preset_category(&self.config.id),
            icon: self.config.icon.clone().unwrap_or_default(),
            label: self.config.label.clone(),
            description: self.description.to_string(),
            providers,
            relation_targets,
        }
    }
}

fn category_infos() -> Vec<TypePresetCategoryInfo> {
    use TypePresetCategory::*;
    [
        (Watch, "Watch"),
        (Play, "Play"),
        (Read, "Read"),
        (Listen, "Listen"),
        (People, "People & connections"),
        (Life, "Life"),
    ]
    .into_iter()
    .map(|(id, label)| TypePresetCategoryInfo {
        id,
        label: label.to_string(),
    })
    .collect()
}

/// The category a preset id belongs to. Kept beside the registry so the summary
/// and the category list can't disagree.
fn preset_category(id: &str) -> TypePresetCategory {
    use TypePresetCategory::*;
    match id {
        "anime" | "drama" | "movie" => Watch,
        "games" | "board" => Play,
        "books" | "manga" => Read,
        "music" | "podcast" => Listen,
        "artist" | "franchise" | "character" => People,
        "event" => Life,
        _ => Watch,
    }
}

/// Build every preset for the given context (title language). The order here is
/// the picker's within-category order.
fn built_presets(ctx: &BuildCtx) -> Vec<Preset> {
    vec![
        // --- Watch ---
        Preset {
            description: "Track what you're watching and where you left off.",
            provider_ids: &["bangumi", "myanimelist", "tmdb", "thetvdb"],
            config: build_type(
                ctx,
                TypeSpec {
                    id: "anime",
                    label: "Anime",
                    icon: "📺",
                    path: "Anime",
                    providers: &["bangumi", "myanimelist", "tmdb", "thetvdb"],
                    statuses: WATCH_STATUS,
                    original_title: true,
                    primary_date: PrimaryDate::Season,
                    completed_date: true,
                    list: Some(EPISODES_LIST),
                    extras: &[],
                    relations: &[FRANCHISE_REL],
                    summary_body: true,
                    log_hashtag: Some("Anime"),
                },
            ),
        },
        Preset {
            description: "TV series and dramas, episode by episode.",
            provider_ids: &["tmdb", "thetvdb"],
            config: build_type(
                ctx,
                TypeSpec {
                    id: "drama",
                    label: "TV & Drama",
                    icon: "🎭",
                    path: "Drama",
                    providers: &["tmdb", "thetvdb"],
                    statuses: WATCH_STATUS,
                    original_title: true,
                    primary_date: PrimaryDate::Season,
                    completed_date: true,
                    list: Some(EPISODES_LIST),
                    extras: &[],
                    relations: &[FRANCHISE_REL],
                    summary_body: true,
                    log_hashtag: Some("Drama"),
                },
            ),
        },
        Preset {
            description: "Films you've seen or want to see.",
            provider_ids: &["tmdb", "bangumi", "thetvdb"],
            config: build_type(
                ctx,
                TypeSpec {
                    id: "movie",
                    label: "Movies",
                    icon: "🎬",
                    path: "Movie",
                    providers: &["tmdb", "bangumi", "thetvdb"],
                    statuses: WATCH_STATUS,
                    original_title: true,
                    primary_date: PrimaryDate::ReleaseDate,
                    completed_date: true,
                    list: None,
                    extras: &[],
                    relations: &[FRANCHISE_REL],
                    summary_body: true,
                    log_hashtag: Some("Movie"),
                },
            ),
        },
        // --- Play ---
        Preset {
            description: "Your backlog, what you're playing, and what you've beaten.",
            provider_ids: &["igdb", "steam", "bangumi"],
            config: build_type(
                ctx,
                TypeSpec {
                    id: "games",
                    label: "Games",
                    icon: "🎮",
                    path: "Games",
                    providers: &["igdb", "steam", "bangumi"],
                    statuses: PLAY_STATUS,
                    original_title: false,
                    primary_date: PrimaryDate::ReleaseDate,
                    completed_date: true,
                    list: None,
                    extras: &[Extra::Platform, Extra::Genre],
                    relations: &[FRANCHISE_REL],
                    summary_body: true,
                    log_hashtag: Some("Game"),
                },
            ),
        },
        Preset {
            description: "Board games and tabletop, with player count and playtime.",
            provider_ids: &["bgg"],
            config: build_type(
                ctx,
                TypeSpec {
                    id: "board",
                    label: "Board Games",
                    icon: "🎲",
                    path: "Board Games",
                    providers: &["bgg"],
                    statuses: PLAY_STATUS,
                    original_title: false,
                    primary_date: PrimaryDate::ReleaseDate,
                    completed_date: false,
                    list: None,
                    extras: &[Extra::Players, Extra::Playtime],
                    relations: &[FRANCHISE_REL],
                    summary_body: true,
                    log_hashtag: Some("BoardGame"),
                },
            ),
        },
        // --- Read ---
        Preset {
            description: "Books you're reading, with authors and ISBNs.",
            provider_ids: &["googlebooks", "openlibrary", "hardcover", "bangumi"],
            config: build_type(
                ctx,
                TypeSpec {
                    id: "books",
                    label: "Books",
                    icon: "📚",
                    path: "Books",
                    providers: &["googlebooks", "openlibrary", "hardcover", "bangumi"],
                    statuses: READ_STATUS,
                    original_title: true,
                    primary_date: PrimaryDate::ReleaseDate,
                    completed_date: true,
                    list: None,
                    extras: &[Extra::Author, Extra::Isbn],
                    relations: &[FRANCHISE_REL],
                    summary_body: true,
                    log_hashtag: Some("Book"),
                },
            ),
        },
        Preset {
            description: "Manga and comics, tracked by chapter.",
            provider_ids: &["bangumi", "mangaupdates", "myanimelist", "comicvine"],
            config: build_type(
                ctx,
                TypeSpec {
                    id: "manga",
                    label: "Manga & Comics",
                    icon: "📖",
                    path: "Manga",
                    providers: &["bangumi", "mangaupdates", "myanimelist", "comicvine"],
                    statuses: READ_STATUS,
                    original_title: true,
                    primary_date: PrimaryDate::ReleaseDate,
                    completed_date: true,
                    list: Some(CHAPTERS_LIST),
                    extras: &[Extra::Author],
                    relations: &[FRANCHISE_REL],
                    summary_body: true,
                    log_hashtag: Some("Manga"),
                },
            ),
        },
        // --- Listen ---
        Preset {
            description: "Albums and CDs — the music you own and love.",
            provider_ids: &["musicbrainz", "discogs", "spotify", "bangumi"],
            config: build_type(
                ctx,
                TypeSpec {
                    id: "music",
                    label: "Music Albums",
                    icon: "💿",
                    path: "Music",
                    providers: &["musicbrainz", "discogs", "spotify", "bangumi"],
                    statuses: LISTEN_STATUS,
                    original_title: false,
                    primary_date: PrimaryDate::ReleaseDate,
                    completed_date: false,
                    list: Some(TRACKS_LIST),
                    extras: &[Extra::OwnedFormats],
                    relations: &[ARTIST_REL, FRANCHISE_REL],
                    summary_body: true,
                    log_hashtag: Some("Music"),
                },
            ),
        },
        Preset {
            description: "Podcasts you follow.",
            provider_ids: &["apple_podcast"],
            config: build_type(
                ctx,
                TypeSpec {
                    id: "podcast",
                    label: "Podcasts",
                    icon: "🎙️",
                    path: "Podcasts",
                    providers: &["apple_podcast"],
                    statuses: LISTEN_STATUS,
                    original_title: false,
                    primary_date: PrimaryDate::None,
                    completed_date: false,
                    list: Some(EPISODES_LIST),
                    extras: &[],
                    relations: &[],
                    summary_body: false,
                    log_hashtag: Some("Podcast"),
                },
            ),
        },
        // --- People & connections ---
        Preset {
            description: "Artists, authors, studios — the people behind your library.",
            provider_ids: &["bangumi", "musicbrainz", "spotify"],
            config: build_type(
                ctx,
                TypeSpec {
                    id: "artist",
                    label: "Artists & Creators",
                    icon: "🎤",
                    path: "Artist",
                    providers: &["bangumi", "musicbrainz", "spotify"],
                    statuses: &[],
                    original_title: true,
                    primary_date: PrimaryDate::None,
                    completed_date: false,
                    list: None,
                    extras: &[Extra::Birthday],
                    relations: &[FRANCHISE_REL, ("groups", "Groups", "artist")],
                    summary_body: true,
                    log_hashtag: None,
                },
            ),
        },
        Preset {
            description: "Group related entries — a series, saga, or shared universe.",
            provider_ids: &[],
            config: build_type(
                ctx,
                TypeSpec {
                    id: "franchise",
                    label: "Franchises",
                    icon: "🧩",
                    path: "Franchise",
                    providers: &[],
                    statuses: &[],
                    original_title: true,
                    primary_date: PrimaryDate::None,
                    completed_date: false,
                    list: None,
                    extras: &[],
                    relations: &[],
                    summary_body: false,
                    log_hashtag: None,
                },
            ),
        },
        Preset {
            description: "Characters, with who voices or portrays them.",
            provider_ids: &["bangumi"],
            config: build_type(
                ctx,
                TypeSpec {
                    id: "character",
                    label: "Characters",
                    icon: "👤",
                    path: "Characters",
                    providers: &["bangumi"],
                    statuses: &[],
                    original_title: true,
                    primary_date: PrimaryDate::None,
                    completed_date: false,
                    list: None,
                    extras: &[Extra::Birthday],
                    relations: &[FRANCHISE_REL, ("voice_by", "Voice by", "artist")],
                    summary_body: true,
                    log_hashtag: None,
                },
            ),
        },
        // --- Life ---
        Preset {
            description: "Concerts, exhibitions, and events you attend.",
            provider_ids: &[],
            config: build_type(
                ctx,
                TypeSpec {
                    id: "event",
                    label: "Events",
                    icon: "🎫",
                    path: "Event",
                    providers: &[],
                    statuses: EVENT_STATUS,
                    original_title: false,
                    primary_date: PrimaryDate::EventDate,
                    completed_date: false,
                    list: None,
                    extras: &[Extra::Location],
                    relations: &[ARTIST_REL, FRANCHISE_REL],
                    summary_body: false,
                    log_hashtag: Some("Event"),
                },
            ),
        },
    ]
}

// Shared relation tuples: (field name, display label, target preset id).
const FRANCHISE_REL: (&str, &str, &str) = ("franchise", "Franchise", "franchise");
const ARTIST_REL: (&str, &str, &str) = ("artist", "Artist", "artist");

// Shared status vocabularies: (canonical, user-facing label). The first label for
// a canonical is the write target for a log flip.
const WATCH_STATUS: &[(CanonicalStatus, &str)] = &[
    (CanonicalStatus::Planning, "Wishlist"),
    (CanonicalStatus::Ongoing, "Watching"),
    (CanonicalStatus::Completed, "Watched"),
    (CanonicalStatus::Paused, "Paused"),
    (CanonicalStatus::Dropped, "Dropped"),
];
const PLAY_STATUS: &[(CanonicalStatus, &str)] = &[
    (CanonicalStatus::Planning, "Backlog"),
    (CanonicalStatus::Ongoing, "Playing"),
    (CanonicalStatus::Completed, "Completed"),
    (CanonicalStatus::Paused, "Paused"),
    (CanonicalStatus::Dropped, "Dropped"),
];
const READ_STATUS: &[(CanonicalStatus, &str)] = &[
    (CanonicalStatus::Planning, "Backlog"),
    (CanonicalStatus::Ongoing, "Reading"),
    (CanonicalStatus::Completed, "Finished"),
    (CanonicalStatus::Paused, "Paused"),
    (CanonicalStatus::Dropped, "Dropped"),
];
const LISTEN_STATUS: &[(CanonicalStatus, &str)] = &[
    (CanonicalStatus::Planning, "Wishlist"),
    (CanonicalStatus::Ongoing, "Listening"),
    (CanonicalStatus::Completed, "Listened"),
    (CanonicalStatus::Paused, "Paused"),
    (CanonicalStatus::Dropped, "Dropped"),
];
const EVENT_STATUS: &[(CanonicalStatus, &str)] = &[
    (CanonicalStatus::Planning, "Planned"),
    (CanonicalStatus::Completed, "Attended"),
];

const EPISODES_LIST: ListSpec = ListSpec {
    heading: "Episodes",
    total_field: "episodes",
    total_label: "Episodes",
    tracking: EpisodeTracking::Checklist,
    total_role: Some(Role::TotalEpisodes),
};
const CHAPTERS_LIST: ListSpec = ListSpec {
    heading: "Chapters",
    total_field: "chapters",
    total_label: "Chapters",
    tracking: EpisodeTracking::Checklist,
    total_role: Some(Role::Chapters),
};
const TRACKS_LIST: ListSpec = ListSpec {
    heading: "Tracks",
    total_field: "tracks",
    total_label: "Tracks",
    tracking: EpisodeTracking::None,
    // Providers don't expose a reliable scalar track count.
    total_role: None,
};

// --- The builder ---------------------------------------------------------------

struct BuildCtx {
    catalog: Vec<crate::contract::ExternalProviderCatalogItem>,
    /// Normalized ISO 639-1 title language; `en` when unset/unknown.
    lang: String,
}

impl BuildCtx {
    fn new(lang: Option<&str>) -> Self {
        // Primary subtag only: a `zh-Hans`/`zh-Hant` preference must stamp bare
        // `zh` into the generated schema — script subtags never enter config.
        let lang = lang
            .map(crate::languages::primary_language)
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "en".to_string());
        Self {
            catalog: provider_catalog_items(),
            lang,
        }
    }

    fn season_language(&self) -> SeasonLanguage {
        match self.lang.as_str() {
            "zh" => SeasonLanguage::Zh,
            "ja" => SeasonLanguage::Ja,
            _ => SeasonLanguage::En,
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

#[derive(Clone, Copy)]
enum Extra {
    Platform,
    Genre,
    Author,
    Isbn,
    Players,
    Playtime,
    OwnedFormats,
    Location,
    Birthday,
}

struct ListSpec {
    heading: &'static str,
    total_field: &'static str,
    total_label: &'static str,
    tracking: EpisodeTracking,
    /// External role for the total-count field, when providers expose one.
    total_role: Option<Role>,
}

struct TypeSpec {
    id: &'static str,
    label: &'static str,
    icon: &'static str,
    path: &'static str,
    /// Providers in priority order; the primary is the first.
    providers: &'static [&'static str],
    /// Empty → no status field (people/hub types).
    statuses: &'static [(CanonicalStatus, &'static str)],
    original_title: bool,
    primary_date: PrimaryDate,
    completed_date: bool,
    list: Option<ListSpec>,
    extras: &'static [Extra],
    /// (field name, display label, target preset id).
    relations: &'static [(&'static str, &'static str, &'static str)],
    summary_body: bool,
    /// Per-type daily-note hashtag, e.g. `Anime` → `- {title} {note} #Anime`.
    /// `None` → the type is not loggable.
    log_hashtag: Option<&'static str>,
}

fn build_type(ctx: &BuildCtx, spec: TypeSpec) -> EntityTypeConfig {
    // People/hub types use the language-agnostic "original" name as the filename;
    // everything else uses the chosen language.
    let name_based =
        spec.statuses.is_empty() && !matches!(spec.primary_date, PrimaryDate::EventDate);

    let mut fields = vec![field("id", FieldType::Id, "ID")];

    // Primary title, in the chosen language, mapped from every provider's title.
    let mut title = field("title", FieldType::Title, "Title");
    if !name_based {
        title.title_language = Some(ctx.lang.clone());
    } else {
        title.title_role = Some(TitleRole::Original);
    }
    title.external_fields = ext_maps(ctx, spec.providers, Role::Title);
    fields.push(title);

    if spec.original_title && !name_based {
        let mut original = field("title_original", FieldType::Title, "Title (original)");
        original.title_role = Some(TitleRole::Original);
        original.external_fields = ext_maps(ctx, spec.providers, Role::OriginalTitle);
        fields.push(original);
    }

    let mut cover = field("cover_url", FieldType::Image, "Cover");
    cover.external_fields = ext_maps(ctx, spec.providers, Role::Cover);
    fields.push(cover);

    if !spec.statuses.is_empty() {
        fields.push(status_field(spec.statuses));
    }

    if !name_based {
        fields.push(field("rating", FieldType::Rating, "Rating"));
    }

    if let Some(list) = &spec.list {
        let mut progress = field("progress", FieldType::Progress, "Progress");
        progress.total_progress_field = Some(list.total_field.to_string());
        fields.push(progress);
        let mut total = field(list.total_field, FieldType::TotalProgress, list.total_label);
        if let Some(role) = list.total_role {
            total.external_fields = ext_maps(ctx, spec.providers, role);
        }
        fields.push(total);
    }

    for extra in spec.extras {
        fields.push(extra_field(ctx, spec.providers, *extra));
    }

    match spec.primary_date {
        PrimaryDate::None => {}
        PrimaryDate::Season => {
            let mut season = field("season", FieldType::Season, "Season");
            season.date_role = Some(crate::types::DateRole::Planning);
            season.season_language = Some(ctx.season_language());
            season.external_fields = ext_maps(ctx, spec.providers, Role::Season);
            fields.push(season);
        }
        PrimaryDate::ReleaseDate => {
            let mut release = field("release_date", FieldType::Date, "Release date");
            release.date_role = Some(crate::types::DateRole::Planning);
            release.external_fields = ext_maps(ctx, spec.providers, Role::ReleaseDate);
            fields.push(release);
        }
        PrimaryDate::EventDate => {
            let mut date = field("date", FieldType::Date, "Date");
            date.date_role = Some(crate::types::DateRole::Event);
            fields.push(date);
        }
    }

    if spec.completed_date {
        let mut completed = field("complete_date", FieldType::Date, "Completed date");
        completed.date_role = Some(crate::types::DateRole::Completed);
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

    for (name, label, target) in spec.relations {
        let mut relation = field(name, FieldType::Relation, label);
        relation.relation_type = Some((*target).to_string());
        fields.push(relation);
    }

    let body_sections = build_body_sections(ctx, &spec);

    EntityTypeConfig {
        id: spec.id.to_string(),
        label: spec.label.to_string(),
        icon: Some(spec.icon.to_string()),
        path: spec.path.to_string(),
        external_priority: spec
            .providers
            .iter()
            .filter(|p| ctx.catalog.iter().any(|item| item.id == **p))
            .map(|p| (*p).to_string())
            .collect(),
        filename: Some(if name_based {
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
            line_format: Some(format!("- {{title}} {{note}} #{tag}")),
        }),
        fields,
    }
}

fn build_body_sections(ctx: &BuildCtx, spec: &TypeSpec) -> Vec<BodySection> {
    let mut sections = Vec::new();
    if spec.summary_body {
        let external_fields = ext_maps(ctx, spec.providers, Role::Summary);
        if !external_fields.is_empty() {
            sections.push(BodySection {
                heading: "Summary".to_string(),
                kind: BodySectionKind::External,
                external_fields,
                tracking: None,
            });
        }
    }
    if let Some(list) = &spec.list {
        sections.push(BodySection {
            heading: list.heading.to_string(),
            kind: BodySectionKind::Episodes,
            external_fields: Vec::new(),
            tracking: Some(list.tracking),
        });
    }
    sections
}

fn extra_field(ctx: &BuildCtx, providers: &[&str], extra: Extra) -> FieldConfig {
    match extra {
        Extra::Platform => {
            let mut f = field("platform", FieldType::EnumList, "Platform");
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
            f.external_fields = ext_maps(ctx, providers, Role::Platform);
            f
        }
        Extra::Genre => {
            let mut f = field("genre", FieldType::EnumList, "Genre");
            f.enum_options = [
                "RPG",
                "Action",
                "Adventure",
                "Strategy",
                "Simulation",
                "Puzzle",
            ]
            .iter()
            .map(|v| v.to_string())
            .collect();
            f.external_fields = ext_maps(ctx, providers, Role::Genre);
            f
        }
        Extra::Author => {
            let mut f = field("author", FieldType::TextList, "Author");
            f.external_fields = ext_maps(ctx, providers, Role::Author);
            f
        }
        Extra::Isbn => {
            let mut f = field("isbn", FieldType::Text, "ISBN");
            f.external_fields = ext_maps(ctx, providers, Role::Isbn);
            f
        }
        Extra::Players => field("players", FieldType::Text, "Players"),
        Extra::Playtime => field("playtime", FieldType::Text, "Playtime"),
        Extra::OwnedFormats => {
            let mut f = field("owned", FieldType::EnumList, "Owned");
            f.enum_options = ["CD", "Vinyl", "Digital"]
                .iter()
                .map(|v| v.to_string())
                .collect();
            f
        }
        Extra::Location => field("location", FieldType::Text, "Location"),
        Extra::Birthday => {
            let mut f = field("birthday", FieldType::Date, "Birthday");
            f.external_fields = ext_maps(ctx, providers, Role::Birthday);
            f
        }
    }
}

fn status_field(options: &[(CanonicalStatus, &str)]) -> FieldConfig {
    let mut f = field("status", FieldType::Enum, "Status");
    f.enum_options = options.iter().map(|(_, label)| label.to_string()).collect();
    f.enum_role = Some(EnumRole::Status);
    let mut values = StatusValues::default();
    for (canonical, label) in options {
        let bucket = match canonical {
            CanonicalStatus::Planning => &mut values.planning,
            CanonicalStatus::Ongoing => &mut values.ongoing,
            CanonicalStatus::Paused => &mut values.paused,
            CanonicalStatus::Completed => &mut values.completed,
            CanonicalStatus::Dropped => &mut values.dropped,
        };
        bucket.push((*label).to_string());
    }
    f.status_values = Some(values);
    f
}

/// A default "Recent {label}" home section for a type — but **only** when the type
/// has a release/completion date to sort by. A chronological shelf is meaningless
/// for types with no such date (people, franchises) or whose only date is an
/// attendance date (events), so those are left out of the default Home entirely
/// rather than getting a title-sorted "recent" shelf that isn't really recent.
fn home_section_for(config: &EntityTypeConfig) -> Option<HomeSectionConfig> {
    let date_field = config
        .fields
        .iter()
        .find(|field| field.date_role == Some(crate::types::DateRole::Planning))
        .or_else(|| {
            config
                .fields
                .iter()
                .find(|field| field.date_role == Some(crate::types::DateRole::Completed))
        })?;
    Some(HomeSectionConfig {
        id: format!("recent-{}", config.id),
        title: format!("Recent {}", config.label),
        entity_type: config.id.clone(),
        filters: Vec::new(),
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
/// … Path comparison is case-insensitive.
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

// --- Provider role → field mapping ---------------------------------------------
//
// Default role→field guesses used ONLY to pre-fill a preset. The running app is
// strictly schema-driven — a type's explicit `externalFields` declare which
// provider field maps to which internal field — so this guessing lives here, in
// the preset seed, and nowhere else.

#[derive(Clone, Copy)]
enum Role {
    Title,
    OriginalTitle,
    Cover,
    ReleaseDate,
    Summary,
    Author,
    Isbn,
    Platform,
    Genre,
    Birthday,
    /// The airing season. Mapped to a provider's air date (coerced date→season by
    /// the core) or an explicit season label (MAL).
    Season,
    /// Total episode count (the `TotalProgress` denominator).
    TotalEpisodes,
    /// Total chapter count for manga/comics.
    Chapters,
}

/// The provider field that fills `role` for `source`, or `None` when the provider
/// has nothing meaningful for it.
fn role_field(source: &str, role: Role) -> Option<&'static str> {
    let table: &[(Role, &str)] = match source {
        "bangumi" => &[
            (Role::Title, "name_cn"),
            (Role::OriginalTitle, "name"),
            (Role::Cover, "cover_url"),
            (Role::ReleaseDate, "date"),
            (Role::Season, "date"),
            (Role::TotalEpisodes, "eps"),
            (Role::Chapters, "eps"),
            (Role::Summary, "summary"),
            (Role::Author, "author"),
            (Role::Isbn, "isbn"),
            (Role::Genre, "genre"),
            (Role::Birthday, "birthday"),
        ],
        "myanimelist" => &[
            (Role::Title, "title"),
            (Role::OriginalTitle, "title"),
            (Role::Cover, "cover_url"),
            (Role::ReleaseDate, "start_date"),
            (Role::Season, "season"),
            (Role::TotalEpisodes, "episodes"),
            (Role::Chapters, "chapters"),
            (Role::Summary, "synopsis"),
            (Role::Genre, "genres"),
        ],
        "tmdb" => &[
            (Role::Title, "title"),
            (Role::OriginalTitle, "original_title"),
            (Role::Cover, "cover_url"),
            (Role::ReleaseDate, "release_date"),
            (Role::Season, "release_date"),
            (Role::TotalEpisodes, "episode_count"),
            (Role::Summary, "overview"),
            (Role::Genre, "genres"),
        ],
        "thetvdb" => &[
            (Role::Title, "name"),
            (Role::OriginalTitle, "name"),
            (Role::Cover, "cover_url"),
            (Role::ReleaseDate, "first_air_time"),
            (Role::Season, "first_air_time"),
            (Role::Summary, "overview"),
            (Role::Genre, "genres"),
        ],
        "igdb" => &[
            (Role::Title, "name"),
            (Role::OriginalTitle, "name"),
            (Role::Cover, "cover_url"),
            (Role::ReleaseDate, "first_release_date"),
            (Role::Summary, "summary"),
            (Role::Genre, "genres"),
            (Role::Platform, "platforms"),
        ],
        "steam" => &[
            (Role::Title, "name"),
            (Role::OriginalTitle, "name"),
            (Role::Cover, "cover_url"),
            (Role::ReleaseDate, "release_date"),
            (Role::Summary, "description"),
            (Role::Genre, "genres"),
            (Role::Platform, "platform"),
        ],
        "googlebooks" => &[
            (Role::Title, "title"),
            (Role::OriginalTitle, "title"),
            (Role::Cover, "cover_url"),
            (Role::ReleaseDate, "published_date"),
            (Role::Summary, "description"),
            (Role::Author, "authors"),
            (Role::Isbn, "isbn"),
            (Role::Genre, "categories"),
        ],
        "openlibrary" => &[
            (Role::Title, "title"),
            (Role::OriginalTitle, "title"),
            (Role::Cover, "cover_url"),
            (Role::ReleaseDate, "published_date"),
            (Role::Summary, "description"),
            (Role::Author, "authors"),
            (Role::Isbn, "isbn"),
        ],
        "hardcover" => &[
            (Role::Title, "title"),
            (Role::OriginalTitle, "title"),
            (Role::Cover, "cover_url"),
            (Role::ReleaseDate, "publish_date"),
            (Role::Summary, "synopsis"),
            (Role::Author, "authors"),
            (Role::Isbn, "isbn"),
            (Role::Genre, "genres"),
        ],
        "mangaupdates" => &[
            (Role::Title, "title"),
            (Role::OriginalTitle, "title"),
            (Role::Cover, "cover_url"),
            (Role::Chapters, "latest_chapter"),
            (Role::Summary, "synopsis"),
            (Role::Author, "authors"),
            (Role::Genre, "genres"),
        ],
        "comicvine" => &[
            (Role::Title, "title"),
            (Role::OriginalTitle, "title"),
            (Role::Cover, "cover_url"),
            (Role::Summary, "description"),
            (Role::Genre, "genres"),
        ],
        "musicbrainz" => &[
            (Role::Title, "title"),
            (Role::OriginalTitle, "title"),
            (Role::Cover, "cover_url"),
            (Role::ReleaseDate, "release_date"),
            (Role::Genre, "genres"),
        ],
        "discogs" => &[
            (Role::Title, "title"),
            (Role::OriginalTitle, "title"),
            (Role::Cover, "cover_url"),
            (Role::Genre, "genres"),
        ],
        "spotify" => &[
            (Role::Title, "title"),
            (Role::OriginalTitle, "title"),
            (Role::Cover, "cover_url"),
            (Role::ReleaseDate, "release_date"),
            (Role::Genre, "genres"),
        ],
        "bgg" => &[
            (Role::Title, "name"),
            (Role::OriginalTitle, "name"),
            (Role::Cover, "cover_url"),
            (Role::Summary, "description"),
            (Role::Genre, "categories"),
        ],
        "apple_podcast" => &[
            (Role::Title, "title"),
            (Role::OriginalTitle, "title"),
            (Role::Cover, "cover_url"),
            (Role::Genre, "genre"),
        ],
        _ => return None,
    };
    table
        .iter()
        .find(|(candidate, _)| role_eq(*candidate, role))
        .map(|(_, field)| *field)
}

fn role_eq(a: Role, b: Role) -> bool {
    std::mem::discriminant(&a) == std::mem::discriminant(&b)
}

/// External mappings for `role` across every provider (in priority order) that
/// has a field for it — mirroring how the real vault fills a field from several
/// providers.
fn ext_maps(ctx: &BuildCtx, providers: &[&str], role: Role) -> Vec<ExternalFieldMapping> {
    providers
        .iter()
        .filter(|p| ctx.catalog.iter().any(|item| item.id == **p))
        .filter_map(|p| {
            role_field(p, role).map(|field| ExternalFieldMapping {
                source: (*p).to_string(),
                field: field.to_string(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolve(
        current: Vec<EntityTypeConfig>,
        ids: &[&str],
        lang: Option<&str>,
    ) -> ResolveTypePresetsResponse {
        resolve_presets(&ResolveTypePresetsRequest {
            current_types: current,
            preset_ids: ids.iter().map(|s| s.to_string()).collect(),
            title_language: lang.map(str::to_string),
        })
    }

    fn find_field<'a>(type_config: &'a EntityTypeConfig, name: &str) -> Option<&'a FieldConfig> {
        type_config.fields.iter().find(|f| f.field == name)
    }

    #[test]
    fn every_preset_appears_in_the_catalog() {
        let response = type_presets_response();
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
    fn title_language_is_stamped() {
        let en = resolve(vec![], &["anime"], Some("en"));
        let title = find_field(&en.types[0], "title").unwrap();
        assert_eq!(title.title_language.as_deref(), Some("en"));
        let season = find_field(&en.types[0], "season").unwrap();
        assert_eq!(season.season_language, Some(SeasonLanguage::En));

        let zh = resolve(vec![], &["anime"], Some("zh"));
        let season = find_field(&zh.types[0], "season").unwrap();
        assert_eq!(season.season_language, Some(SeasonLanguage::Zh));
        assert_eq!(
            zh.types[0]
                .filename
                .as_ref()
                .unwrap()
                .title_language
                .as_deref(),
            Some("zh")
        );
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
    fn home_section_only_for_types_with_a_release_date() {
        // Anime has a planning (season) date → gets a shelf. Franchise has no date
        // field and Event's only date is an attendance date (role Event), so both
        // are left out of the default Home rather than getting a bogus "recent" shelf.
        let result = resolve(vec![], &["anime", "franchise", "event"], None);
        assert_eq!(result.home_sections.len(), 1);
        let anime = &result.home_sections[0];
        assert_eq!(anime.entity_type, "anime");
        assert_eq!(anime.sort.as_deref(), Some("date:season"));
        // The date-less types are still added — just shelf-less.
        assert!(result.types.iter().any(|t| t.id == "franchise"));
        assert!(result.types.iter().any(|t| t.id == "event"));
    }

    #[test]
    fn resolved_config_serializes_to_yaml() {
        let anime = resolve(vec![], &["anime"], Some("en")).types.remove(0);
        let yaml = serde_yaml::to_string(&anime).unwrap();
        assert!(yaml.contains("id: anime"));
        assert!(yaml.contains("enumRole: status"));
    }
}
