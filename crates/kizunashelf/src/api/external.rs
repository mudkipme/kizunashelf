use super::error::{ApiError, ApiResult};
mod bangumi;
mod igdb;
mod thetvdb;

use crate::contract::{
    ExternalCandidate, ExternalProviderCatalogItem, ExternalProviderCatalogResponse,
    ExternalProviderFieldOption, ExternalProviderSummary, ExternalProviderTypeOption,
    ExternalSearchResponse,
};
use crate::dates::clamp_number;
use crate::types::{FieldType, KizunaConfig};
use axum::extract::{Query, State};
use axum::Json;
use schemars::JsonSchema;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::sync::OnceLock;
use std::time::Duration;

use super::state::{get_library, AppState};

pub(super) const USER_AGENT: &str = concat!("KizunaShelf/", env!("CARGO_PKG_VERSION"));
const EXTERNAL_CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const EXTERNAL_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

trait ExternalProvider {
    const ID: &'static str;
    const LABEL: &'static str;

    fn configured_and_supported(provider_config: &ProviderSearchConfig) -> bool;

    fn available(_state: &AppState) -> bool {
        true
    }

    fn unavailable_reason(_state: &AppState) -> Option<String> {
        None
    }

    async fn search(
        state: &AppState,
        q: &str,
        page: usize,
        page_size: usize,
        provider_config: &ProviderSearchConfig,
    ) -> Result<Vec<ExternalCandidate>, ApiError>;
}

#[derive(Clone, Debug, Default)]
struct ProviderSearchConfig {
    unconstrained: bool,
    external_types: BTreeSet<String>,
}

impl ProviderSearchConfig {
    fn add_external_types(&mut self, external_types: &[String]) {
        let external_types = external_types
            .iter()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>();
        if external_types.is_empty() {
            self.unconstrained = true;
            return;
        }
        self.external_types
            .extend(external_types.into_iter().map(str::to_string));
    }

    fn add_unconstrained_source_if_empty(&mut self) {
        if self.external_types.is_empty() {
            self.unconstrained = true;
        }
    }

    fn external_types(&self) -> Option<&BTreeSet<String>> {
        (!self.unconstrained).then_some(&self.external_types)
    }
}

#[derive(Deserialize, JsonSchema)]
pub(crate) struct ExternalSearchQuery {
    provider: Option<String>,
    q: Option<String>,
    #[serde(rename = "type")]
    entity_type: Option<String>,
    #[serde(rename = "pageSize")]
    page_size: Option<f64>,
    page: Option<f64>,
}

pub(crate) async fn external_search(
    State(state): State<AppState>,
    Query(query): Query<ExternalSearchQuery>,
) -> ApiResult<ExternalSearchResponse> {
    let requested_provider = query.provider.as_deref().filter(|value| *value != "all");
    if let Some(provider) = requested_provider {
        if !is_known_provider(provider) {
            return Err(ApiError::bad_request("Unknown external provider"));
        }
    }

    let library = get_library(&state).await?;
    let Some(entity_type) = query
        .entity_type
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty() && *value != "all")
    else {
        return Err(ApiError::bad_request(
            "External search requires a concrete entity type",
        ));
    };
    if !library
        .config
        .types
        .iter()
        .any(|type_config| type_config.id == entity_type)
    {
        return Err(ApiError::bad_request("Unknown entity type"));
    }
    let configured_providers = configured_external_providers(&library.config, entity_type);
    let providers = provider_summaries(&state, &configured_providers);

    let q = query.q.as_deref().unwrap_or_default().trim();
    if q.is_empty() {
        return Ok(Json(ExternalSearchResponse {
            providers,
            items: Vec::new(),
        }));
    }
    let page_size = clamp_number(query.page_size.unwrap_or(10.0), 1, 25) as usize;
    let page = clamp_number(query.page.unwrap_or(1.0), 1, i64::MAX) as usize;

    let order = provider_order(&library.config, entity_type);

    // Run every selected provider concurrently rather than summing their
    // latencies sequentially. `tokio::join!` polls all three on this task, so
    // no spawning or 'static bound is needed; each `search_provider` returns an
    // empty Vec when its provider is not selected/enabled.
    let (bangumi_items, igdb_items, thetvdb_items) = tokio::join!(
        search_provider::<bangumi::BangumiProvider>(
            &state,
            &order,
            requested_provider,
            &providers,
            &configured_providers,
            q,
            page,
            page_size,
        ),
        search_provider::<igdb::IgdbProvider>(
            &state,
            &order,
            requested_provider,
            &providers,
            &configured_providers,
            q,
            page,
            page_size,
        ),
        search_provider::<thetvdb::ThetvdbProvider>(
            &state,
            &order,
            requested_provider,
            &providers,
            &configured_providers,
            q,
            page,
            page_size,
        ),
    );

    let mut by_provider: BTreeMap<&'static str, Vec<ExternalCandidate>> = BTreeMap::new();
    by_provider.insert(bangumi::BangumiProvider::ID, bangumi_items?);
    by_provider.insert(igdb::IgdbProvider::ID, igdb_items?);
    by_provider.insert(thetvdb::ThetvdbProvider::ID, thetvdb_items?);

    // Reassemble in priority order so concurrency does not change result order.
    let mut items = Vec::new();
    for provider in &order {
        if let Some(found) = by_provider.remove(provider) {
            items.extend(found);
        }
    }
    Ok(Json(ExternalSearchResponse { providers, items }))
}

pub(crate) async fn external_provider_catalog() -> Json<ExternalProviderCatalogResponse> {
    Json(ExternalProviderCatalogResponse {
        providers: provider_catalog_items(),
    })
}

/// The static provider catalog (id, label, field/type options, default
/// role→field mappings). Shared between the `/api/external/providers` endpoint
/// and the vault-template builder so external-field wiring has a single source.
pub(crate) fn provider_catalog_items() -> Vec<ExternalProviderCatalogItem> {
    vec![
        provider_catalog_item::<bangumi::BangumiProvider>(
            bangumi::field_options(),
            bangumi::type_options(),
            &[],
        ),
        provider_catalog_item::<igdb::IgdbProvider>(
            igdb::field_options(),
            igdb::type_options(),
            &["game"],
        ),
        provider_catalog_item::<thetvdb::ThetvdbProvider>(
            thetvdb::field_options(),
            thetvdb::type_options(),
            &[],
        ),
    ]
}

fn provider_catalog_item<P: ExternalProvider>(
    fields: Vec<ExternalProviderFieldOption>,
    types: Vec<ExternalProviderTypeOption>,
    default_external_types: &[&str],
) -> ExternalProviderCatalogItem {
    ExternalProviderCatalogItem {
        id: P::ID.to_string(),
        label: P::LABEL.to_string(),
        fields,
        types,
        default_external_types: default_external_types
            .iter()
            .map(|value| value.to_string())
            .collect(),
    }
}

#[allow(clippy::too_many_arguments)]
async fn search_provider<P: ExternalProvider>(
    state: &AppState,
    order: &[&'static str],
    requested_provider: Option<&str>,
    providers: &[ExternalProviderSummary],
    configured_providers: &BTreeMap<&'static str, ProviderSearchConfig>,
    q: &str,
    page: usize,
    page_size: usize,
) -> Result<Vec<ExternalCandidate>, ApiError> {
    if !order.contains(&P::ID) {
        return Ok(Vec::new());
    }
    if !should_search_provider(requested_provider, providers, P::ID) {
        return Ok(Vec::new());
    }
    let Some(provider_config) = configured_providers.get(P::ID) else {
        return Ok(Vec::new());
    };
    P::search(state, q, page, page_size, provider_config).await
}

fn provider_summaries(
    state: &AppState,
    configured_providers: &BTreeMap<&'static str, ProviderSearchConfig>,
) -> Vec<ExternalProviderSummary> {
    vec![
        provider_summary::<bangumi::BangumiProvider>(state, configured_providers),
        provider_summary::<igdb::IgdbProvider>(state, configured_providers),
        provider_summary::<thetvdb::ThetvdbProvider>(state, configured_providers),
    ]
}

fn provider_summary<P: ExternalProvider>(
    state: &AppState,
    configured_providers: &BTreeMap<&'static str, ProviderSearchConfig>,
) -> ExternalProviderSummary {
    ExternalProviderSummary {
        id: P::ID.to_string(),
        label: P::LABEL.to_string(),
        enabled: provider_configured_and_supported::<P>(configured_providers)
            && P::available(state),
        reason: provider_reason::<P>(state, configured_providers),
    }
}

fn provider_reason<P: ExternalProvider>(
    state: &AppState,
    configured_providers: &BTreeMap<&'static str, ProviderSearchConfig>,
) -> Option<String> {
    if !configured_providers.contains_key(P::ID) {
        return Some("No external source mapping configured for this source".to_string());
    }
    if !provider_configured_and_supported::<P>(configured_providers) {
        return Some("No supported externalTypes configured for this source".to_string());
    }
    P::unavailable_reason(state)
}

fn provider_configured_and_supported<P: ExternalProvider>(
    configured_providers: &BTreeMap<&'static str, ProviderSearchConfig>,
) -> bool {
    let Some(provider_config) = configured_providers.get(P::ID) else {
        return false;
    };
    P::configured_and_supported(provider_config)
}

fn should_search_provider(
    requested_provider: Option<&str>,
    providers: &[ExternalProviderSummary],
    provider: &str,
) -> bool {
    if requested_provider.is_some_and(|requested| requested != provider) {
        return false;
    }
    providers
        .iter()
        .any(|item| item.id == provider && item.enabled)
}

fn is_known_provider(provider: &str) -> bool {
    matches!(provider, "bangumi" | "igdb" | "thetvdb")
}

fn configured_external_providers(
    config: &KizunaConfig,
    entity_type: &str,
) -> BTreeMap<&'static str, ProviderSearchConfig> {
    let mut providers = BTreeMap::new();
    for type_config in &config.types {
        if type_config.id != entity_type {
            continue;
        }
        for field in &type_config.fields {
            if field.field_type != FieldType::ExternalRef {
                continue;
            }
            if let Some(provider) = field
                .external_ref
                .as_deref()
                .and_then(provider_for_external_ref)
            {
                providers
                    .entry(provider)
                    .or_insert_with(ProviderSearchConfig::default)
                    .add_external_types(&field.external_types);
            }
        }
        for mapping in &type_config.body_mappings {
            if let Some(provider) = provider_for_external_ref(&mapping.source) {
                providers
                    .entry(provider)
                    .or_insert_with(ProviderSearchConfig::default)
                    .add_unconstrained_source_if_empty();
            }
        }
    }
    providers
}

fn provider_order(config: &KizunaConfig, entity_type: &str) -> Vec<&'static str> {
    let configured = configured_external_providers(config, entity_type);
    let mut order = Vec::new();
    if let Some(type_config) = config.types.iter().find(|item| item.id == entity_type) {
        for provider in &type_config.external_priority {
            if let Some(provider) = provider_for_external_ref(provider) {
                if configured.contains_key(provider) && !order.contains(&provider) {
                    order.push(provider);
                }
            }
        }
    }
    for provider in ["bangumi", "igdb", "thetvdb"] {
        if configured.contains_key(provider) && !order.contains(&provider) {
            order.push(provider);
        }
    }
    order
}

/// Shared, connection-pooled HTTP client for outbound provider requests. Built
/// once on first use so repeated searches reuse keep-alive connections instead
/// of paying a fresh TLS handshake per request.
pub(super) fn external_client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .connect_timeout(EXTERNAL_CONNECT_TIMEOUT)
            .timeout(EXTERNAL_REQUEST_TIMEOUT)
            .build()
            .unwrap_or_else(|_| reqwest::Client::new())
    })
}

fn provider_for_external_ref(external_ref: &str) -> Option<&'static str> {
    match external_ref.trim().to_ascii_lowercase().as_str() {
        "bangumi" => Some("bangumi"),
        "igdb" => Some("igdb"),
        "thetvdb" => Some("thetvdb"),
        _ => None,
    }
}

fn provider_error(error: reqwest::Error) -> ApiError {
    // The full source chain can include transport/TLS/DNS internals and request
    // URLs (which may carry credentials), so it is logged server-side only and
    // never returned to the client.
    let mut detail = format!("External provider request failed: {error}");
    let mut source = error.source();
    while let Some(inner) = source {
        detail.push_str(&format!(": {inner}"));
        source = inner.source();
    }
    eprintln!("{detail}");
    // Upstream failures are not the caller's fault: surface them as gateway
    // errors so clients can distinguish a flaky provider from a bad request.
    if error.is_timeout() {
        ApiError::gateway_timeout("The external provider timed out")
    } else {
        ApiError::bad_gateway("The external provider request failed")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{EntityTypeConfig, ExternalBodyMapping, FieldConfig};

    #[test]
    fn external_providers_are_derived_from_schema_mappings() {
        let config = KizunaConfig {
            vault_root: "/vault".to_string(),
            taxonomy_root: "Taxonomy".to_string(),
            asset_root: None,
            content_writable: None,
            home: None,
            daily_notes: None,
            types: vec![
                entity_type("animation", "BGM Link", "bangumi"),
                entity_type("interactive", "IGDB Link", "igdb"),
                entity_type("series", "TVDB Link", "thetvdb"),
            ],
        };

        assert!(configured_external_providers(&config, "animation").contains_key("bangumi"));
        assert!(configured_external_providers(&config, "interactive").contains_key("igdb"));
        assert!(configured_external_providers(&config, "series").contains_key("thetvdb"));
        assert!(configured_external_providers(&config, "all").is_empty());
    }

    #[test]
    fn external_body_mappings_enable_provider_without_guessing_type_name() {
        let mut type_config = entity_type("drama", "IGDB Body", "igdb");
        type_config.fields.clear();
        type_config.body_mappings = vec![ExternalBodyMapping {
            source: "igdb".to_string(),
            field: "summary".to_string(),
            heading: "Summary".to_string(),
        }];
        let config = KizunaConfig {
            vault_root: "/vault".to_string(),
            taxonomy_root: "Taxonomy".to_string(),
            asset_root: None,
            content_writable: None,
            home: None,
            daily_notes: None,
            types: vec![type_config],
        };

        let configured = configured_external_providers(&config, "drama");

        assert!(configured.contains_key("igdb"));
        assert!(!configured.contains_key("thetvdb"));
    }

    #[test]
    fn external_body_mappings_do_not_override_external_ref_type_filters() {
        let mut type_config =
            entity_type_with_external_types("drama", "TVDB Link", "thetvdb", &["series"]);
        type_config.body_mappings = vec![ExternalBodyMapping {
            source: "thetvdb".to_string(),
            field: "overview".to_string(),
            heading: "Summary".to_string(),
        }];
        let config = KizunaConfig {
            vault_root: "/vault".to_string(),
            taxonomy_root: "Taxonomy".to_string(),
            asset_root: None,
            content_writable: None,
            home: None,
            daily_notes: None,
            types: vec![type_config],
        };

        let configured = configured_external_providers(&config, "drama");

        assert_eq!(
            thetvdb::thetvdb_type_filters(configured.get("thetvdb").unwrap()),
            Some(vec![Some("series".to_string())])
        );
    }

    #[test]
    fn external_provider_type_gates_are_provider_specific() {
        let config = KizunaConfig {
            vault_root: "/vault".to_string(),
            taxonomy_root: "Taxonomy".to_string(),
            asset_root: None,
            content_writable: None,
            home: None,
            daily_notes: None,
            types: vec![
                entity_type_with_external_types("animation", "BGM Link", "bangumi", &["2"]),
                entity_type_with_external_types("series", "TVDB Link", "thetvdb", &["series"]),
                entity_type_with_external_types("video", "TVDB Link", "thetvdb", &["movie"]),
                entity_type_with_external_types("bad", "TVDB Link", "thetvdb", &["game"]),
            ],
        };

        let bangumi = configured_external_providers(&config, "animation");
        assert_eq!(
            bangumi::bangumi_types(bangumi.get("bangumi").unwrap()),
            Some(vec![2])
        );

        let series = configured_external_providers(&config, "series");
        assert_eq!(
            thetvdb::thetvdb_type_filters(series.get("thetvdb").unwrap()),
            Some(vec![Some("series".to_string())])
        );

        let movie = configured_external_providers(&config, "video");
        assert_eq!(
            thetvdb::thetvdb_type_filters(movie.get("thetvdb").unwrap()),
            Some(vec![Some("movie".to_string())])
        );

        let invalid = configured_external_providers(&config, "bad");
        assert_eq!(
            thetvdb::thetvdb_type_filters(invalid.get("thetvdb").unwrap()),
            None
        );
    }

    #[test]
    fn unknown_type_has_no_configured_external_providers() {
        let config = KizunaConfig {
            vault_root: "/vault".to_string(),
            taxonomy_root: "Taxonomy".to_string(),
            asset_root: None,
            content_writable: None,
            home: None,
            daily_notes: None,
            types: vec![entity_type("animation", "Bangumi Link", "bangumi")],
        };

        assert!(configured_external_providers(&config, "anime").is_empty());
    }

    fn entity_type(id: &str, field: &str, external_ref: &str) -> EntityTypeConfig {
        entity_type_with_external_types(id, field, external_ref, &[])
    }

    fn entity_type_with_external_types(
        id: &str,
        field: &str,
        external_ref: &str,
        external_types: &[&str],
    ) -> EntityTypeConfig {
        EntityTypeConfig {
            id: id.to_string(),
            label: id.to_string(),
            icon: None,
            path: id.to_string(),
            external_priority: Vec::new(),
            filename: None,
            body_mappings: Vec::new(),
            fields: vec![FieldConfig {
                field: field.to_string(),
                field_type: FieldType::ExternalRef,
                display_name: None,
                title_language: None,
                title_role: None,
                external_fields: Vec::new(),
                enum_options: Vec::new(),
                total_progress_field: None,
                date_role: None,
                season_language: None,
                external_ref: Some(external_ref.to_string()),
                external_types: external_types
                    .iter()
                    .map(|value| value.to_string())
                    .collect(),
                relation_type: None,
            }],
        }
    }
}
