//! Starter vault schemas offered during onboarding and vault creation.
//!
//! This is the single source of truth for the presets that web onboarding, the
//! desktop "new vault" flow, and the iOS create-vault flow all use. Each frontend
//! reaches these through the `/api/vault-templates` endpoint (or, for the desktop
//! which links the core directly, through [`starter_vault_config`]). External
//! field wiring reuses the same provider catalog that powers
//! `/api/external/providers`, so a template never drifts from the providers it
//! references.

use crate::api::external::provider_catalog_items;
use crate::contract::{ExternalProviderCatalogItem, VaultTemplate};
use crate::types::{
    DailyNotesConfig, DateRole, EntityTypeConfig, ExternalBodyMapping, ExternalFieldMapping,
    FieldConfig, FieldType, FilenameConfig, HomeConfig, HomeSectionConfig, SeasonLanguage,
    SortDirection, VaultConfig,
};

const TAXONOMY_ROOT: &str = "Taxonomy";
const ASSET_ROOT: &str = "Assets";

/// The `id` of the default starter preset (a media tracker), used by the desktop
/// and iOS create-vault flows.
pub const STARTER_TEMPLATE_ID: &str = "media";

/// All onboarding presets. The list and their contents are identical across
/// every platform.
pub fn vault_templates() -> Vec<VaultTemplate> {
    let catalog = provider_catalog_items();
    vec![
        VaultTemplate {
            id: "media".to_string(),
            label: "Media Library".to_string(),
            config: VaultConfig {
                taxonomy_root: TAXONOMY_ROOT.to_string(),
                asset_root: Some(ASSET_ROOT.to_string()),
                daily_notes: Some(default_daily_notes()),
                home: Some(HomeConfig {
                    title: Some("Home".to_string()),
                    sections: vec![
                        home_section(
                            "recent-anime",
                            "Recent Anime",
                            "anime",
                            "date:season",
                            SortDirection::Desc,
                        ),
                        home_section("games", "Games", "games", "title", SortDirection::Asc),
                    ],
                }),
                types: vec![
                    media_type(
                        &catalog,
                        "anime",
                        "Anime",
                        "📺",
                        "Anime",
                        &[("bangumi_url", "bangumi")],
                        &["season", "release_date"],
                        &["complete_date"],
                    ),
                    media_type(
                        &catalog,
                        "drama",
                        "Drama",
                        "🎭",
                        "Drama",
                        &[("thetvdb_url", "thetvdb")],
                        &["season", "release_date"],
                        &["complete_date"],
                    ),
                    media_type(
                        &catalog,
                        "movie",
                        "Movie",
                        "🎬",
                        "Movie",
                        &[("bangumi_url", "bangumi"), ("thetvdb_url", "thetvdb")],
                        &["release_date"],
                        &["complete_date"],
                    ),
                    media_type(
                        &catalog,
                        "games",
                        "Games",
                        "🎮",
                        "Games",
                        &[("igdb_url", "igdb")],
                        &["release_date"],
                        &["complete_date"],
                    ),
                ],
            },
        },
        VaultTemplate {
            id: "watching".to_string(),
            label: "Anime + Drama + Movies".to_string(),
            config: VaultConfig {
                taxonomy_root: TAXONOMY_ROOT.to_string(),
                asset_root: Some(ASSET_ROOT.to_string()),
                daily_notes: Some(default_daily_notes()),
                home: Some(empty_home()),
                types: vec![
                    media_type(
                        &catalog,
                        "anime",
                        "Anime",
                        "📺",
                        "Anime",
                        &[("bangumi_url", "bangumi")],
                        &["season", "release_date"],
                        &["complete_date"],
                    ),
                    media_type(
                        &catalog,
                        "drama",
                        "Drama",
                        "🎭",
                        "Drama",
                        &[("thetvdb_url", "thetvdb")],
                        &["season", "release_date"],
                        &["complete_date"],
                    ),
                    media_type(
                        &catalog,
                        "movie",
                        "Movie",
                        "🎬",
                        "Movie",
                        &[("bangumi_url", "bangumi"), ("thetvdb_url", "thetvdb")],
                        &["release_date"],
                        &["complete_date"],
                    ),
                ],
            },
        },
        VaultTemplate {
            id: "games".to_string(),
            label: "Games".to_string(),
            config: VaultConfig {
                taxonomy_root: TAXONOMY_ROOT.to_string(),
                asset_root: Some(ASSET_ROOT.to_string()),
                daily_notes: Some(default_daily_notes()),
                home: Some(empty_home()),
                types: vec![media_type(
                    &catalog,
                    "games",
                    "Games",
                    "🎮",
                    "Games",
                    &[("igdb_url", "igdb")],
                    &["release_date"],
                    &["complete_date"],
                )],
            },
        },
        VaultTemplate {
            id: "books".to_string(),
            label: "Books".to_string(),
            config: VaultConfig {
                taxonomy_root: TAXONOMY_ROOT.to_string(),
                asset_root: Some(ASSET_ROOT.to_string()),
                daily_notes: Some(default_daily_notes()),
                home: Some(empty_home()),
                types: vec![media_type(
                    &catalog,
                    "books",
                    "Books",
                    "📚",
                    "Books",
                    &[],
                    &["release_date"],
                    &["complete_date"],
                )],
            },
        },
        VaultTemplate {
            id: "blank".to_string(),
            label: "Custom Blank".to_string(),
            config: VaultConfig {
                taxonomy_root: TAXONOMY_ROOT.to_string(),
                asset_root: Some(ASSET_ROOT.to_string()),
                daily_notes: Some(default_daily_notes()),
                home: Some(empty_home()),
                types: vec![blank_type()],
            },
        },
    ]
}

/// The default starter vault config (the "media" preset), for flows that create a
/// vault directly without a template picker (desktop & iOS create-vault).
pub fn starter_vault_config() -> VaultConfig {
    vault_templates()
        .into_iter()
        .find(|template| template.id == STARTER_TEMPLATE_ID)
        .map(|template| template.config)
        .expect("starter template is always present")
}

/// The default starter vault config serialized to the YAML written to
/// `<vault>/KizunaShelf/config.yaml`. Keeps YAML serialization in the core so
/// frontends that write the file directly need no YAML dependency.
pub fn starter_vault_config_yaml() -> String {
    serde_yaml::to_string(&starter_vault_config()).expect("starter vault config always serializes")
}

fn default_daily_notes() -> DailyNotesConfig {
    DailyNotesConfig {
        paths: vec!["Daily Notes".to_string()],
        date_format: Some("YYYY-MM-DD".to_string()),
    }
}

fn empty_home() -> HomeConfig {
    HomeConfig {
        title: Some("Home".to_string()),
        sections: Vec::new(),
    }
}

fn home_section(
    id: &str,
    title: &str,
    entity_type: &str,
    sort: &str,
    direction: SortDirection,
) -> HomeSectionConfig {
    HomeSectionConfig {
        id: id.to_string(),
        title: title.to_string(),
        entity_type: entity_type.to_string(),
        filters: Vec::new(),
        limit: Some(12),
        sort: Some(sort.to_string()),
        direction: Some(direction),
    }
}

/// A media-tracker entity type wired to external providers — mirrors the web
/// `mediaType` builder.
#[allow(clippy::too_many_arguments)]
fn media_type(
    catalog: &[ExternalProviderCatalogItem],
    id: &str,
    label: &str,
    icon: &str,
    path: &str,
    external_refs: &[(&str, &str)],
    planning_dates: &[&str],
    completed_dates: &[&str],
) -> EntityTypeConfig {
    let state_options = [
        "Backlog",
        "Watching",
        "Playing",
        "Reading",
        "Completed",
        "Paused",
        "Dropped",
    ];
    let primary_source = external_refs
        .first()
        .map(|(_, source)| *source)
        .unwrap_or("");

    let mut title = title_field("title", "Title", FieldType::Title);
    title.title_language = Some("zh".to_string());
    title.external_fields = default_external_mappings(primary_source, "title");

    let mut title_original = title_field("title_original", "Title (Original)", FieldType::Title);
    title_original.title_role = Some(crate::types::TitleRole::Original);
    title_original.external_fields = default_external_mappings(primary_source, "originalTitle");

    let mut title_en = title_field("title_en", "Title (English)", FieldType::Title);
    title_en.title_language = Some("en".to_string());
    title_en.external_fields = default_external_mappings(primary_source, "titleEn");

    let mut title_ja = title_field("title_ja", "Title (Japanese)", FieldType::Title);
    title_ja.title_language = Some("ja".to_string());
    title_ja.external_fields = default_external_mappings(primary_source, "titleJa");

    let mut cover = field("cover_url", FieldType::Image, "Cover");
    cover.external_fields = default_external_mappings(primary_source, "cover");

    let mut state = field("state", FieldType::Enum, "State");
    state.enum_options = state_options
        .iter()
        .map(|value| value.to_string())
        .collect();

    let mut progress = field("progress", FieldType::Progress, "Progress");
    progress.total_progress_field = Some("episodes".to_string());

    let mut fields = vec![
        field("uid", FieldType::Id, "UID"),
        field("id", FieldType::Id, "ID"),
        title,
        title_original,
        title_en,
        title_ja,
        cover,
        state,
        progress,
        field("episodes", FieldType::TotalProgress, "Episodes"),
        field("rating", FieldType::Rating, "Rating"),
    ];

    for &name in planning_dates {
        fields.push(planning_date_field(primary_source, name));
    }
    for &name in completed_dates {
        let mut date = field(
            name,
            FieldType::Date,
            if name == "complete_date" {
                "Completed date"
            } else {
                name
            },
        );
        date.date_role = Some(DateRole::Completed);
        fields.push(date);
    }
    for &(ref_field, source) in external_refs {
        let mut external = field(ref_field, FieldType::ExternalRef, ref_field);
        external.external_ref = Some(source.to_string());
        external.external_types = external_types_for_source(catalog, source);
        fields.push(external);
    }
    let mut franchise = field("franchise", FieldType::Relation, "Franchise");
    franchise.relation_type = Some("franchise".to_string());
    fields.push(franchise);

    EntityTypeConfig {
        id: id.to_string(),
        label: label.to_string(),
        icon: Some(icon.to_string()),
        path: path.to_string(),
        external_priority: default_external_priority(
            catalog,
            external_refs.iter().map(|(_, source)| *source),
        ),
        filename: Some(FilenameConfig {
            title_language: Some("zh".to_string()),
            title_role: None,
        }),
        body_mappings: default_external_body_mappings(primary_source, "summary", "Summary"),
        fields,
    }
}

fn planning_date_field(source: &str, name: &str) -> FieldConfig {
    if name == "season" {
        let mut season = field("season", FieldType::Season, "Season");
        season.date_role = Some(DateRole::Planning);
        season.season_language = Some(SeasonLanguage::Zh);
        return season;
    }
    let mut date = field(
        name,
        FieldType::Date,
        if name == "release_date" {
            "Release date"
        } else {
            name
        },
    );
    date.date_role = Some(DateRole::Planning);
    if name == "release_date" {
        date.external_fields = default_external_mappings(source, "releaseDate");
    }
    date
}

/// The "Custom Blank" preset's single placeholder type — mirrors the web
/// `defaultEntityType`.
fn blank_type() -> EntityTypeConfig {
    let mut state = field("state", FieldType::Enum, "State");
    state.enum_options = ["Backlog", "Active", "Completed", "Paused", "Dropped"]
        .iter()
        .map(|value| value.to_string())
        .collect();
    EntityTypeConfig {
        id: "type".to_string(),
        label: "Type".to_string(),
        icon: Some(String::new()),
        path: "Type".to_string(),
        external_priority: Vec::new(),
        filename: Some(FilenameConfig {
            title_language: None,
            title_role: Some(crate::types::TitleRole::Original),
        }),
        body_mappings: Vec::new(),
        fields: vec![
            field("id", FieldType::Id, "ID"),
            state,
            field("progress", FieldType::Progress, "Progress"),
        ],
    }
}

fn field(field: &str, field_type: FieldType, display_name: &str) -> FieldConfig {
    FieldConfig {
        field: field.to_string(),
        field_type,
        display_name: Some(display_name.to_string()),
        title_language: None,
        title_role: None,
        external_fields: Vec::new(),
        enum_options: Vec::new(),
        total_progress_field: None,
        date_role: None,
        season_language: None,
        external_ref: None,
        external_types: Vec::new(),
        relation_type: None,
    }
}

fn title_field(field: &str, display_name: &str, field_type: FieldType) -> FieldConfig {
    self::field(field, field_type, display_name)
}

// --- Catalog-aware wiring (mirrors apps/web/src/lib/external-metadata.ts) ---

fn provider<'a>(
    catalog: &'a [ExternalProviderCatalogItem],
    source: &str,
) -> Option<&'a ExternalProviderCatalogItem> {
    let expected = source.trim().to_ascii_lowercase();
    catalog.iter().find(|item| item.id == expected)
}

/// Default role→field guesses used ONLY to pre-fill a fresh template. The running
/// app is strictly schema-driven — a type's explicit `externalFields` declare
/// which provider field maps to which internal field — and never infers field
/// meaning from a role. So this guessing lives here, in the template seed, and
/// nowhere else (it is intentionally absent from the provider catalog contract).
fn template_default_field(source: &str, role: &str) -> Option<&'static str> {
    let mappings: &[(&str, &[&str])] = match source.trim().to_ascii_lowercase().as_str() {
        "bangumi" => &[
            ("name_cn", &["title"]),
            ("name", &["originalTitle", "titleJa"]),
            ("cover_url", &["cover"]),
            ("date", &["releaseDate"]),
            ("summary", &["summary"]),
        ],
        "igdb" => &[
            ("name", &["title", "originalTitle"]),
            ("cover_url", &["cover"]),
            ("first_release_date", &["releaseDate"]),
            ("summary", &["summary"]),
        ],
        "thetvdb" => &[
            ("name", &["title", "originalTitle"]),
            ("cover_url", &["cover"]),
            ("first_air_time", &["releaseDate"]),
            ("overview", &["summary"]),
        ],
        _ => return None,
    };
    mappings
        .iter()
        .find(|(_, roles)| roles.contains(&role))
        .map(|(field, _)| *field)
}

fn default_external_mappings(source: &str, role: &str) -> Vec<ExternalFieldMapping> {
    match template_default_field(source, role) {
        Some(field) => vec![ExternalFieldMapping {
            source: source.to_string(),
            field: field.to_string(),
        }],
        None => Vec::new(),
    }
}

fn default_external_body_mappings(
    source: &str,
    role: &str,
    heading: &str,
) -> Vec<ExternalBodyMapping> {
    match template_default_field(source, role) {
        Some(field) => vec![ExternalBodyMapping {
            source: source.to_string(),
            field: field.to_string(),
            heading: heading.to_string(),
        }],
        None => Vec::new(),
    }
}

fn external_types_for_source(catalog: &[ExternalProviderCatalogItem], source: &str) -> Vec<String> {
    provider(catalog, source)
        .map(|item| item.default_external_types.clone())
        .unwrap_or_default()
}

fn default_external_priority<'a>(
    catalog: &[ExternalProviderCatalogItem],
    sources: impl Iterator<Item = &'a str>,
) -> Vec<String> {
    let mut priority = Vec::new();
    for source in sources {
        let normalized = source.trim().to_ascii_lowercase();
        if provider(catalog, &normalized).is_some() && !priority.contains(&normalized) {
            priority.push(normalized);
        }
    }
    priority
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_presets_are_present() {
        let ids: Vec<_> = vault_templates()
            .into_iter()
            .map(|template| template.id)
            .collect();
        assert_eq!(ids, ["media", "watching", "games", "books", "blank"]);
    }

    #[test]
    fn media_template_wires_external_fields_from_catalog() {
        let media = starter_vault_config();
        let anime = media
            .types
            .iter()
            .find(|type_config| type_config.id == "anime")
            .unwrap();

        // Title maps to Bangumi's `name_cn` (the catalog's `title` role).
        let title = anime
            .fields
            .iter()
            .find(|field| field.field == "title")
            .unwrap();
        assert_eq!(
            title.external_fields,
            vec![ExternalFieldMapping {
                source: "bangumi".into(),
                field: "name_cn".into()
            }]
        );
        // The external-ref field carries the provider id.
        assert!(anime.fields.iter().any(|field| field.field == "bangumi_url"
            && field.external_ref.as_deref() == Some("bangumi")));
        // Summary body mapping is wired from the catalog.
        assert_eq!(anime.body_mappings.len(), 1);
        assert_eq!(anime.body_mappings[0].field, "summary");
    }

    #[test]
    fn starter_config_serializes_to_yaml() {
        // The desktop create-vault flow and the example config both depend on
        // this serializing cleanly.
        let yaml = serde_yaml::to_string(&starter_vault_config()).unwrap();
        assert!(yaml.contains("taxonomyRoot: Taxonomy"));
        assert!(yaml.contains("dateFormat: YYYY-MM-DD"));
    }
}
