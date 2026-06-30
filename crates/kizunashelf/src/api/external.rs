use super::error::{ApiError, ApiResult};
mod apple_podcast;
mod bangumi;
mod bgg;
mod comicvine;
mod discogs;
mod google_books;
mod hardcover;
mod igdb;
mod mal;
mod mangaupdates;
mod mapping;
mod musicbrainz;
mod open_library;
mod spotify;
mod steam;
mod thetvdb;
mod tmdb;

use crate::contract::{
    ExternalCandidate, ExternalProviderCatalogItem, ExternalProviderCatalogResponse,
    ExternalProviderCredentialField, ExternalProviderFieldOption, ExternalProviderSummary,
    ExternalProviderTypeOption, ExternalSearchResponse, ProviderEpisodes,
};
use crate::dates::clamp_number;
use crate::types::{BodySectionKind, FieldType, KizunaConfig};
use axum::extract::{Query, State};
use axum::Json;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::future::Future;
use std::pin::Pin;
use std::sync::OnceLock;
use std::time::Duration;

use super::state::{get_library, AppState, CachedAccessToken};

pub(super) const USER_AGENT: &str = concat!("KizunaShelf/", env!("CARGO_PKG_VERSION"));
const EXTERNAL_CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const EXTERNAL_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

trait ExternalProvider {
    const ID: &'static str;
    const LABEL: &'static str;
    /// Whether the provider answers free-text queries. `false` means it only
    /// resolves a pasted URL/ID (its `search` returns the resolved candidate for
    /// a recognized URL/ID and an empty list otherwise).
    const SEARCHABLE: bool = true;

    /// Credentials this provider requires. Empty (the default) means keyless.
    fn credentials() -> &'static [CredentialSpec] {
        &[]
    }

    /// Values from *this provider's own* [`Self::type_options`] to preselect as
    /// the `externalTypes` filter when the provider is first wired to a field in
    /// the schema editor. These are the provider's internal taxonomy (IGDB's
    /// `game`, Google Books' `book`), **never** a KizunaShelf entity-type id —
    /// nothing maps a provider to an entity type by name. Multi-type providers
    /// (Bangumi, TheTVDB) leave this empty so the user picks the constraint.
    fn default_external_types() -> &'static [&'static str] {
        &[]
    }

    /// The external fields this provider can populate, offered in the schema
    /// editor's field mapping.
    fn field_options() -> Vec<ExternalProviderFieldOption>;

    /// The external types this provider can be constrained to.
    fn type_options() -> Vec<ExternalProviderTypeOption>;

    fn configured_and_supported(provider_config: &ProviderSearchConfig) -> bool;

    fn available(_state: &AppState) -> bool {
        true
    }

    fn unavailable_reason(_state: &AppState) -> Option<String> {
        None
    }

    fn search(
        state: &AppState,
        q: &str,
        page: usize,
        page_size: usize,
        provider_config: &ProviderSearchConfig,
    ) -> impl Future<Output = Result<Vec<ExternalCandidate>, ApiError>> + Send;

    /// Whether this provider can supply an entity's episodes/tracks (`fetch_episodes`).
    const SUPPORTS_EPISODES: bool = false;

    /// Fetches an entity's episodes/tracks given its stored external ref value (a
    /// URL or id — the provider reuses its own URL→id parser). `language` is the
    /// viewer's content language (ISO 639-1); providers that support translated
    /// titles honor it. The default rejects; providers that set
    /// `SUPPORTS_EPISODES = true` override this.
    fn fetch_episodes(
        _state: &AppState,
        _ref_value: &str,
        _language: Option<&str>,
    ) -> impl Future<Output = Result<ProviderEpisodes, ApiError>> + Send {
        async {
            Err(ApiError::bad_request(
                "This provider does not support episode import",
            ))
        }
    }
}

/// One credential a provider needs. The `key` is the [`crate::secrets`] store key
/// (and the `KIZUNASHELF_<UPPER_KEY>` env var on web/desktop).
pub(super) struct CredentialSpec {
    pub key: &'static str,
    pub label: &'static str,
    pub secret: bool,
    pub required: bool,
}

/// A future returned by a provider's boxed `search`. Boxed so the registry can
/// hold every provider behind one uniform, non-generic entry.
type SearchFut<'a> =
    Pin<Box<dyn Future<Output = Result<Vec<ExternalCandidate>, ApiError>> + Send + 'a>>;

/// The boxed counterpart for `fetch_episodes`.
type EpisodesFut<'a> =
    Pin<Box<dyn Future<Output = Result<ProviderEpisodes, ApiError>> + Send + 'a>>;

type FetchEpisodesFn = for<'a> fn(&'a AppState, &'a str, Option<&'a str>) -> EpisodesFut<'a>;

/// One provider, erased to plain fn pointers so the orchestration can iterate a
/// `Vec<ProviderEntry>` instead of naming each provider type. Adding a provider
/// is a single line in [`registry`] — no `tokio::join!` arm or match to update.
struct ProviderEntry {
    id: &'static str,
    label: &'static str,
    searchable: bool,
    credentials: &'static [CredentialSpec],
    default_external_types: &'static [&'static str],
    field_options: fn() -> Vec<ExternalProviderFieldOption>,
    type_options: fn() -> Vec<ExternalProviderTypeOption>,
    configured_and_supported: fn(&ProviderSearchConfig) -> bool,
    available: fn(&AppState) -> bool,
    unavailable_reason: fn(&AppState) -> Option<String>,
    search:
        for<'a> fn(&'a AppState, &'a str, usize, usize, &'a ProviderSearchConfig) -> SearchFut<'a>,
    supports_episodes: bool,
    fetch_episodes: FetchEpisodesFn,
}

fn search_boxed<'a, P: ExternalProvider + 'static>(
    state: &'a AppState,
    q: &'a str,
    page: usize,
    page_size: usize,
    provider_config: &'a ProviderSearchConfig,
) -> SearchFut<'a> {
    Box::pin(P::search(state, q, page, page_size, provider_config))
}

fn fetch_episodes_boxed<'a, P: ExternalProvider + 'static>(
    state: &'a AppState,
    ref_value: &'a str,
    language: Option<&'a str>,
) -> EpisodesFut<'a> {
    Box::pin(P::fetch_episodes(state, ref_value, language))
}

fn entry<P: ExternalProvider + 'static>() -> ProviderEntry {
    ProviderEntry {
        id: P::ID,
        label: P::LABEL,
        searchable: P::SEARCHABLE,
        credentials: P::credentials(),
        default_external_types: P::default_external_types(),
        field_options: P::field_options,
        type_options: P::type_options,
        configured_and_supported: P::configured_and_supported,
        available: P::available,
        unavailable_reason: P::unavailable_reason,
        search: search_boxed::<P>,
        supports_episodes: P::SUPPORTS_EPISODES,
        fetch_episodes: fetch_episodes_boxed::<P>,
    }
}

/// Whether a provider id can supply episodes (supports import + is configured).
pub(super) fn provider_supports_episodes(state: &AppState, provider_id: &str) -> bool {
    registry()
        .iter()
        .any(|entry| entry.id == provider_id && entry.supports_episodes && (entry.available)(state))
}

/// The display label for a provider id, if known.
pub(super) fn provider_label(provider_id: &str) -> Option<&'static str> {
    registry()
        .iter()
        .find(|entry| entry.id == provider_id)
        .map(|entry| entry.label)
}

/// Fetches episodes from `provider_id` for an entity's stored external `ref_value`,
/// in `language` (ISO 639-1) where the provider supports translated titles.
pub(super) async fn provider_fetch_episodes(
    state: &AppState,
    provider_id: &str,
    ref_value: &str,
    language: Option<&str>,
) -> Result<ProviderEpisodes, ApiError> {
    let Some(fetch) = registry()
        .iter()
        .find(|entry| entry.id == provider_id && entry.supports_episodes)
        .map(|entry| entry.fetch_episodes)
    else {
        return Err(ApiError::bad_request(
            "This provider does not support episode import",
        ));
    };
    fetch(state, ref_value, language).await
}

/// The provider registry: the single source of truth for which external
/// providers exist. Every other function derives from this — there is no
/// per-provider branching anywhere else in the orchestration.
fn registry() -> Vec<ProviderEntry> {
    vec![
        entry::<bangumi::BangumiProvider>(),
        entry::<igdb::IgdbProvider>(),
        entry::<thetvdb::ThetvdbProvider>(),
        entry::<google_books::GoogleBooksProvider>(),
        entry::<open_library::OpenLibraryProvider>(),
        entry::<apple_podcast::ApplePodcastProvider>(),
        entry::<steam::SteamProvider>(),
        entry::<musicbrainz::MusicBrainzProvider>(),
        entry::<bgg::BoardGameGeekProvider>(),
        entry::<tmdb::TmdbProvider>(),
        entry::<spotify::SpotifyProvider>(),
        entry::<discogs::DiscogsProvider>(),
        entry::<mal::MyAnimeListProvider>(),
        entry::<mangaupdates::MangaUpdatesProvider>(),
        entry::<comicvine::ComicVineProvider>(),
        entry::<hardcover::HardcoverProvider>(),
    ]
}

/// Every distinct credential-store key declared by any provider. Hosts that
/// enumerate credentials (the desktop keychain editor) derive their key list
/// from this so it stays in sync with the registry — no hard-coded provider
/// list outside core.
pub fn provider_credential_keys() -> Vec<&'static str> {
    let mut keys = Vec::new();
    for provider_entry in registry() {
        for credential in provider_entry.credentials {
            if !keys.contains(&credential.key) {
                keys.push(credential.key);
            }
        }
    }
    keys
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
    let Some(type_config) = library.config.type_config(entity_type) else {
        return Err(ApiError::bad_request("Unknown entity type"));
    };
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
    // latencies sequentially. `join_all` polls them all on this task, so no
    // spawning or 'static bound is needed; a provider not in `order`/disabled is
    // simply not given a future.
    let entries = registry();
    let state_ref = &state;
    let searches = entries
        .iter()
        .filter(|provider_entry| order.contains(&provider_entry.id))
        .filter(|provider_entry| {
            should_search_provider(requested_provider, &providers, provider_entry.id)
        })
        .filter_map(|provider_entry| {
            let provider_config = configured_providers.get(provider_entry.id)?;
            Some(async move {
                let result =
                    (provider_entry.search)(state_ref, q, page, page_size, provider_config).await;
                (provider_entry.id, result)
            })
        });
    let results = futures_util::future::join_all(searches).await;

    let mut by_provider: BTreeMap<&'static str, Vec<ExternalCandidate>> = BTreeMap::new();
    for (provider, result) in results {
        by_provider.insert(provider, result?);
    }

    // Reassemble in priority order so concurrency does not change result order.
    let mut raw_items = Vec::new();
    for provider in &order {
        if let Some(found) = by_provider.remove(provider) {
            raw_items.extend(found);
        }
    }
    // Resolve each candidate against the schema once, server-side, so every
    // runtime applies identical field/body values (the core's `mapping`).
    let items = raw_items
        .into_iter()
        .map(|candidate| mapping::match_candidate(candidate, type_config))
        .collect();
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
    registry()
        .iter()
        .map(|provider_entry| ExternalProviderCatalogItem {
            id: provider_entry.id.to_string(),
            label: provider_entry.label.to_string(),
            fields: (provider_entry.field_options)(),
            types: (provider_entry.type_options)(),
            default_external_types: provider_entry
                .default_external_types
                .iter()
                .map(|value| value.to_string())
                .collect(),
            credentials: provider_entry
                .credentials
                .iter()
                .map(|credential| ExternalProviderCredentialField {
                    key: credential.key.to_string(),
                    label: credential.label.to_string(),
                    secret: credential.secret,
                    required: credential.required,
                })
                .collect(),
            search_supported: provider_entry.searchable,
        })
        .collect()
}

fn provider_summaries(
    state: &AppState,
    configured_providers: &BTreeMap<&'static str, ProviderSearchConfig>,
) -> Vec<ExternalProviderSummary> {
    registry()
        .iter()
        .map(|provider_entry| provider_summary(provider_entry, state, configured_providers))
        .collect()
}

fn provider_summary(
    provider_entry: &ProviderEntry,
    state: &AppState,
    configured_providers: &BTreeMap<&'static str, ProviderSearchConfig>,
) -> ExternalProviderSummary {
    let configured = configured_providers
        .get(provider_entry.id)
        .is_some_and(|provider_config| (provider_entry.configured_and_supported)(provider_config));
    ExternalProviderSummary {
        id: provider_entry.id.to_string(),
        label: provider_entry.label.to_string(),
        enabled: configured && (provider_entry.available)(state),
        search_supported: provider_entry.searchable,
        reason: provider_reason(provider_entry, state, configured_providers),
    }
}

fn provider_reason(
    provider_entry: &ProviderEntry,
    state: &AppState,
    configured_providers: &BTreeMap<&'static str, ProviderSearchConfig>,
) -> Option<String> {
    let Some(provider_config) = configured_providers.get(provider_entry.id) else {
        return Some("No external source mapping configured for this source".to_string());
    };
    if !(provider_entry.configured_and_supported)(provider_config) {
        return Some("No supported externalTypes configured for this source".to_string());
    }
    (provider_entry.unavailable_reason)(state)
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
    registry().iter().any(|entry| entry.id == provider)
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
        for external in type_config
            .body_sections
            .iter()
            .filter(|section| section.kind == BodySectionKind::External)
            .flat_map(|section| &section.external_fields)
        {
            if let Some(provider) = provider_for_external_ref(&external.source) {
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
    if let Some(type_config) = config.type_config(entity_type) {
        for provider in &type_config.external_priority {
            if let Some(provider) = provider_for_external_ref(provider) {
                if configured.contains_key(provider) && !order.contains(&provider) {
                    order.push(provider);
                }
            }
        }
    }
    for provider_entry in registry() {
        if configured.contains_key(provider_entry.id) && !order.contains(&provider_entry.id) {
            order.push(provider_entry.id);
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

/// Single-flighted cached-token acquisition shared by the OAuth/login providers
/// (IGDB, Spotify, TheTVDB). Returns the cached access token when still fresh;
/// otherwise takes the per-provider lock, re-checks the cache, and on a miss runs
/// `fetch` to mint a token, stores it, and returns its access token. `force_refresh`
/// skips the first cache check (used after a 401); `label` names the provider in
/// the cache-failure error. This owns the cache/lock/store pattern that each token
/// function otherwise repeated verbatim.
pub(super) async fn cached_or_fetch_token<F, Fut>(
    state: &AppState,
    key: &str,
    label: &str,
    force_refresh: bool,
    fetch: F,
) -> Result<String, ApiError>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<CachedAccessToken, ApiError>>,
{
    if !force_refresh {
        if let Some(token) = state.cached_access_token(key).await {
            return Ok(token.access_token);
        }
    }
    // Single-flight the fetch: under concurrent searches a cold cache would
    // otherwise stampede the provider's token endpoint and risk rate limits.
    let fetch_lock = state.token_fetch_lock(key).await;
    let _guard = fetch_lock.lock().await;
    // Another task may have populated the cache while we waited for the lock.
    if let Some(token) = state.cached_access_token(key).await {
        return Ok(token.access_token);
    }
    let token = fetch().await?;
    let access_token = token.access_token.clone();
    state
        .store_access_token(key, token)
        .await
        .map_err(|error| {
            ApiError::bad_request(&format!("failed to cache {label} token: {error}"))
        })?;
    Ok(access_token)
}

/// Sends a bearer-authenticated request, refreshing the token once on a `401`.
/// `build` produces the request for a given access token (so the retry
/// re-authorizes with the fresh token); `refresh` mints a new token after the
/// cached one is invalidated. Returns the raw response (the caller checks status).
pub(super) async fn send_with_token_retry<B, R, RFut>(
    state: &AppState,
    key: &str,
    token: &str,
    build: B,
    refresh: R,
) -> Result<reqwest::Response, ApiError>
where
    B: Fn(&str) -> reqwest::RequestBuilder,
    R: FnOnce() -> RFut,
    RFut: Future<Output = Result<String, ApiError>>,
{
    let response = build(token).send().await.map_err(provider_error)?;
    if response.status() != reqwest::StatusCode::UNAUTHORIZED {
        return Ok(response);
    }
    state.invalidate_access_token(key).await;
    let token = refresh().await?;
    build(&token).send().await.map_err(provider_error)
}

pub(super) fn field_option(field: &str, label: &str) -> ExternalProviderFieldOption {
    ExternalProviderFieldOption {
        field: field.to_string(),
        label: label.to_string(),
    }
}

pub(super) fn type_option(value: &str, label: &str) -> ExternalProviderTypeOption {
    ExternalProviderTypeOption {
        value: value.to_string(),
        label: label.to_string(),
    }
}

/// Normalizes a book identifier to ISBN-13: a 10-digit ISBN is converted to its
/// `978`-prefixed EAN-13 form (recomputing the check digit), a 13-digit value is
/// returned digits-only, and anything else is returned trimmed unchanged. Used by
/// the book providers so they surface a consistent ISBN-13.
pub(super) fn normalize_isbn(raw: &str) -> String {
    let digits: String = raw
        .chars()
        .filter(|character| character.is_ascii_digit())
        .collect();
    if digits.len() == 13 {
        return digits;
    }
    if digits.len() != 10 {
        return raw.trim().to_string();
    }
    let core: String = format!("978{}", &digits[..9]);
    let sum: u32 = core
        .bytes()
        .enumerate()
        .map(|(index, byte)| {
            let value = u32::from(byte - b'0');
            if index % 2 == 0 {
                value
            } else {
                value * 3
            }
        })
        .sum();
    let check = (10 - (sum % 10)) % 10;
    format!("{core}{check}")
}

// ---------------------------------------------------------------------------
// Shared JSON-shaping helpers used by the provider modules. Each provider maps a
// raw API response into the KizunaShelf candidate shape, and these cover the
// pieces that were otherwise re-spelled per module: collecting string arrays,
// pulling a named field out of an array of objects, inserting trimmed strings,
// coercing scalars, and flattening light HTML. (See `external/*`.)
// ---------------------------------------------------------------------------

/// Collects a JSON array's items into a trimmed, non-empty `Value::Array`, or
/// `None` when the input isn't an array or yields nothing. `extract` pulls the
/// string out of each element — pass [`Value::as_str`] for an array of plain
/// strings (see [`string_list`]).
pub(super) fn string_list_with<'a>(
    value: Option<&'a Value>,
    extract: impl Fn(&'a Value) -> Option<&'a str>,
) -> Option<Value> {
    let items: Vec<Value> = value?
        .as_array()?
        .iter()
        .filter_map(extract)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(|text| Value::String(text.to_string()))
        .collect();
    (!items.is_empty()).then_some(Value::Array(items))
}

/// [`string_list_with`] for a JSON array of plain strings.
pub(super) fn string_list(value: Option<&Value>) -> Option<Value> {
    string_list_with(value, Value::as_str)
}

/// Wraps a list of strings into `Some(Value::Array)`, or `None` when empty.
pub(super) fn string_array(values: Vec<String>) -> Option<Value> {
    (!values.is_empty()).then(|| Value::Array(values.into_iter().map(Value::String).collect()))
}

/// Collects the `key` field of every object in a JSON array into trimmed,
/// non-empty strings, preserving order (no deduplication).
pub(super) fn named_strings(value: Option<&Value>, key: &str) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|entry| entry.get(key).and_then(Value::as_str))
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// [`named_strings`] over the conventional `"name"` field, as a JSON array.
pub(super) fn named_list(value: Option<&Value>) -> Option<Value> {
    string_array(named_strings(value, "name"))
}

/// Inserts a trimmed, non-empty string under `key`; a no-op when the source is
/// absent or blank.
pub(super) fn insert_str(metadata: &mut Map<String, Value>, key: &str, value: Option<&Value>) {
    if let Some(text) = value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        metadata.insert(key.to_string(), Value::String(text.to_string()));
    }
}

/// Coerces a JSON value to a string: a trimmed non-empty string, else an integer
/// (`i64` or `u64`) rendered as text. `None` for anything else.
pub(super) fn non_empty_string_or_integer(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| value.as_i64().map(|value| value.to_string()))
        .or_else(|| value.as_u64().map(|value| value.to_string()))
}

/// Removes `<…>` tags, returning the text between them. Does no trimming or
/// `<br>` handling — callers layer those on via [`strip_html`].
fn strip_tags(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut in_tag = false;
    for character in value.chars() {
        match character {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(character),
            _ => {}
        }
    }
    out
}

/// Strips HTML tags, turning `<br>` into newlines, then trims — for the light
/// HTML some providers return in descriptions.
pub(super) fn strip_html(value: &str) -> String {
    strip_tags(&value.replace("<br", "\n<br"))
        .trim()
        .to_string()
}

/// Like [`strip_html`] but collapses every run of whitespace (including the
/// inserted breaks) to single spaces — a one-line plain-text form.
pub(super) fn strip_html_collapsed(value: &str) -> String {
    strip_tags(&value.replace("<br", "\n<br"))
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

pub(super) fn provider_for_external_ref(external_ref: &str) -> Option<&'static str> {
    let external_ref = external_ref.trim().to_ascii_lowercase();
    registry()
        .iter()
        .find(|entry| entry.id == external_ref)
        .map(|entry| entry.id)
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
    use crate::types::{
        BodySection, BodySectionKind, EntityTypeConfig, ExternalFieldMapping, FieldConfig,
    };

    #[test]
    fn normalize_isbn_converts_isbn10_to_isbn13() {
        // ISBN-10 → 978-prefixed ISBN-13 with a recomputed check digit.
        assert_eq!(normalize_isbn("0-306-40615-2"), "9780306406157");
        // An existing ISBN-13 is returned digits-only.
        assert_eq!(normalize_isbn("978-1-7185-0310-6"), "9781718503106");
        // Anything that isn't a 10/13-digit ISBN is returned trimmed.
        assert_eq!(normalize_isbn("  not-an-isbn "), "not-an-isbn");
    }

    #[test]
    fn external_providers_are_derived_from_schema_mappings() {
        let config = KizunaConfig {
            vault_root: "/vault".to_string(),
            taxonomy_root: "Taxonomy".to_string(),
            asset_root: None,
            content_writable: None,
            home: None,
            daily_notes: None,
            tags: None,
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
        type_config.body_sections = vec![BodySection {
            heading: "Summary".to_string(),
            kind: BodySectionKind::External,
            external_fields: vec![ExternalFieldMapping {
                source: "igdb".to_string(),
                field: "summary".to_string(),
            }],
            tracking: None,
        }];
        let config = KizunaConfig {
            vault_root: "/vault".to_string(),
            taxonomy_root: "Taxonomy".to_string(),
            asset_root: None,
            content_writable: None,
            home: None,
            daily_notes: None,
            tags: None,
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
        type_config.body_sections = vec![BodySection {
            heading: "Summary".to_string(),
            kind: BodySectionKind::External,
            external_fields: vec![ExternalFieldMapping {
                source: "thetvdb".to_string(),
                field: "overview".to_string(),
            }],
            tracking: None,
        }];
        let config = KizunaConfig {
            vault_root: "/vault".to_string(),
            taxonomy_root: "Taxonomy".to_string(),
            asset_root: None,
            content_writable: None,
            home: None,
            daily_notes: None,
            tags: None,
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
            tags: None,
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
            tags: None,
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
            body_sections: Vec::new(),
            log: None,
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
