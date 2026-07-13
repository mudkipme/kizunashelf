use super::error::{ApiError, ApiResult};
mod apple_podcast;
mod applemusic;
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
mod steam;
mod thetvdb;
mod tmdb;

use crate::contract::{
    ExistingEntityRef, ExternalCandidate, ExternalProviderCatalogItem,
    ExternalProviderCatalogResponse, ExternalProviderCredentialField, ExternalProviderFieldOption,
    ExternalProviderSummary, ExternalProviderTypeOption, ExternalSearchResponse, MappedFieldValue,
    ProviderEpisodes, QuickAddRequest, QuickAddResponse,
};
use crate::dates::clamp_number;
use crate::library::load_entity;
use crate::status::status_field;
use crate::types::{
    BodySectionKind, CanonicalStatus, EntityTypeConfig, FieldType, KizunaConfig, Library,
};
use axum::extract::{Query, State};
use axum::Json;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::error::Error;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use super::assets::download_new_entity_covers;
use super::episodes::import_new_entity_episodes;
use super::mutations::{
    derive_basename, resolve_free_basename, sanitize_basename, type_config_or_err,
    write_new_entity_file,
};
use super::state::{get_library, require_content_writes, AppState, CachedAccessToken};

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

    /// Whether `query` is a URL this provider owns, decided by a pure host check
    /// (no network). Quick Capture routes a pasted provider URL to the single
    /// owning provider and skips everyone else; a URL no provider claims searches
    /// nothing. Must be host-anchored so at most one provider claims a URL — some
    /// id parsers accept any digit-tailed URL, so those override this with an
    /// explicit host check rather than delegating. Default: unclaimed.
    fn recognizes_url(_query: &str) -> bool {
        false
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
    /// viewer's language preference, which may carry a script subtag
    /// (`zh-Hans`/`zh-Hant`); providers that support translated titles normalize
    /// it to whatever their API distinguishes. The default rejects; providers
    /// that set `SUPPORTS_EPISODES = true` override this.
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
    recognizes_url: fn(&str) -> bool,
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
        recognizes_url: P::recognizes_url,
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
/// in `language` (the viewer's preference, possibly `zh-Hans`/`zh-Hant`) where
/// the provider supports translated titles.
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
        entry::<applemusic::AppleMusicProvider>(),
        entry::<steam::SteamProvider>(),
        entry::<musicbrainz::MusicBrainzProvider>(),
        entry::<bgg::BoardGameGeekProvider>(),
        entry::<tmdb::TmdbProvider>(),
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
    /// The viewer's language preference for this search (may carry a script
    /// subtag — `zh-Hans`/`zh-Hant`), stamped per request by the orchestration
    /// rather than coming from the schema. Providers that localize map it to
    /// whatever their API distinguishes; everyone else ignores it. Any titles it
    /// yields are tagged under the bare primary language — script subtags never
    /// enter candidate title maps.
    language: Option<String>,
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

/// Whether an item of provider-type `kind` is allowed by a field's
/// `externalTypes`, for the direct URL/id-lookup path where the type is known
/// from the pasted URL (`/movie/…`, `/artist/…`). An unconstrained field allows
/// any kind; an explicitly constrained field allows only its declared kinds — so
/// a pasted typed URL surfaces under just the matching entity type instead of
/// once per provider-mapped type, and the non-matching types skip the request
/// entirely. Case-insensitive.
fn url_type_allowed(config: &ProviderSearchConfig, kind: &str) -> bool {
    match config.external_types() {
        None => true,
        Some(types) => types
            .iter()
            .any(|external_type| external_type.trim().eq_ignore_ascii_case(kind)),
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
    /// The viewer's language preference (may carry a script subtag —
    /// `zh-Hans`/`zh-Hant`). Providers that localize honor it.
    language: Option<String>,
}

/// Whether a (trimmed) query is an `http(s)` URL — the trigger for routing a
/// Quick Capture paste to the one provider that owns the URL.
fn query_is_url(q: &str) -> bool {
    q.starts_with("http://") || q.starts_with("https://")
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

    // A concrete `type` searches just that type; omitted or `all` searches every
    // type that has any external source configured (cross-type "search anything").
    let requested_type = query
        .entity_type
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty() && *value != "all");
    let searched_types: Vec<&EntityTypeConfig> = match requested_type {
        Some(entity_type) => {
            let type_config = library
                .config
                .type_config(entity_type)
                .ok_or_else(|| ApiError::bad_request("Unknown entity type"))?;
            vec![type_config]
        }
        None => library
            .config
            .types
            .iter()
            .filter(|type_config| {
                !configured_external_providers(&library.config, &type_config.id).is_empty()
            })
            .collect(),
    };

    // The provider summary spans every searched type: a provider is enabled if any
    // searched type maps it. Per-type configs (not this merge) drive the actual
    // searches, so unioning the filters here only affects what the UI lists.
    let mut merged_providers: BTreeMap<&'static str, ProviderSearchConfig> = BTreeMap::new();
    for type_config in &searched_types {
        for (provider, config) in configured_external_providers(&library.config, &type_config.id) {
            let entry = merged_providers.entry(provider).or_default();
            entry.unconstrained |= config.unconstrained;
            entry.external_types.extend(config.external_types);
        }
    }
    let mut providers = provider_summaries(&state, &merged_providers);

    let q = query.q.as_deref().unwrap_or_default().trim();
    if q.is_empty() {
        return Ok(Json(ExternalSearchResponse {
            providers,
            items: Vec::new(),
        }));
    }
    let page_size = clamp_number(query.page_size.unwrap_or(10.0), 1, 25) as usize;
    let page = clamp_number(query.page.unwrap_or(1.0), 1, i64::MAX) as usize;
    let language = query
        .language
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    // Gather the per-type provider order/config, and the deduplicated set of
    // searches to run: a provider queried under identical `externalTypes` for two
    // types hits its API once and both types reuse the result.
    let entries = registry();

    // A pasted provider URL is exact-match intent: route it to the single provider
    // that owns the URL and skip every other provider (still subject to the type
    // filter — the owning provider must be configured for a searched type below).
    // A URL no known provider claims searches nothing, saving every request.
    let url_provider = if query_is_url(q) {
        match entries.iter().find(|entry| (entry.recognizes_url)(q)) {
            Some(entry) => Some(entry.id),
            None => {
                return Ok(Json(ExternalSearchResponse {
                    providers,
                    items: Vec::new(),
                }))
            }
        }
    } else {
        None
    };

    let mut per_type: Vec<(
        &EntityTypeConfig,
        BTreeMap<&'static str, ProviderSearchConfig>,
        Vec<&'static str>,
    )> = Vec::new();
    let mut specs: BTreeMap<SearchKey, (&ProviderEntry, ProviderSearchConfig)> = BTreeMap::new();
    for type_config in &searched_types {
        let configured = configured_external_providers(&library.config, &type_config.id);
        let order = provider_order(&library.config, &type_config.id);
        for (provider, config) in &configured {
            if url_provider.is_some_and(|only| only != *provider) {
                continue;
            }
            if !should_search_provider(requested_provider, &providers, provider) {
                continue;
            }
            if let Some(entry) = entries.iter().find(|entry| entry.id == *provider) {
                // The viewer language is per-request context, not part of the
                // dedup key (it is identical across every spec of one request).
                specs
                    .entry(search_key(provider, config))
                    .or_insert_with(|| {
                        let mut config = config.clone();
                        config.language = language.clone();
                        (entry, config)
                    });
            }
        }
        per_type.push((type_config, configured, order));
    }

    // Run every unique search concurrently rather than summing their latencies.
    let state_ref = &state;
    let searches = specs.iter().map(|(key, (entry, config))| {
        let key = key.clone();
        async move {
            let result = (entry.search)(state_ref, q, page, page_size, config).await;
            (key, result)
        }
    });
    let results = futures_util::future::join_all(searches).await;

    // A single provider failing must not blank the whole search: capture its error
    // onto the summary and keep every other provider's results.
    let mut by_key: BTreeMap<SearchKey, Vec<ExternalCandidate>> = BTreeMap::new();
    let mut errors: BTreeMap<&'static str, String> = BTreeMap::new();
    for (key, result) in results {
        match result {
            Ok(found) => {
                by_key.insert(key, found);
            }
            Err(error) => {
                errors
                    .entry(key.0)
                    .or_insert_with(|| error.message().to_string());
            }
        }
    }
    for (provider, message) in errors {
        if let Some(summary) = providers.iter_mut().find(|summary| summary.id == provider) {
            summary.error = Some(message);
        }
    }

    // Resolve each candidate against every type it was searched for (once,
    // server-side, so every runtime applies identical values), tag any that
    // already exist in the library, and keep priority order within each type.
    let existing_index = build_existing_index(&library);
    let mut items = Vec::new();
    for (type_config, configured, order) in &per_type {
        let mut seen: HashSet<(&str, &str)> = HashSet::new();
        for provider in order {
            let Some(config) = configured.get(provider) else {
                continue;
            };
            let Some(candidates) = by_key.get(&search_key(provider, config)) else {
                continue;
            };
            for candidate in candidates {
                if !seen.insert((candidate.provider.as_str(), candidate.source_id.as_str())) {
                    continue;
                }
                let mut item = mapping::match_candidate(candidate.clone(), type_config);
                item.existing = lookup_existing(&existing_index, candidate, &type_config.id);
                items.push(item);
            }
        }
    }
    Ok(Json(ExternalSearchResponse { providers, items }))
}

/// A deduplication key for an outbound provider search: the provider plus the
/// exact `externalTypes` constraint it will be queried under. Two types that map
/// the same provider identically share one API call.
type SearchKey = (&'static str, bool, Vec<String>);

fn search_key(provider: &'static str, config: &ProviderSearchConfig) -> SearchKey {
    (
        provider,
        config.unconstrained,
        config.external_types.iter().cloned().collect(),
    )
}

/// Normalizes an external-ref value (a stored URL/id, or a candidate's URL/id)
/// into a comparison key: scheme- and case-insensitive, no trailing slash, NFC.
/// Lets a candidate match a hand-edited ref regardless of `http`/`https` or a
/// bare-id vs full-URL storage style.
fn normalize_external_ref(value: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    let lowered = value.trim().to_lowercase();
    let without_scheme = lowered
        .strip_prefix("https://")
        .or_else(|| lowered.strip_prefix("http://"))
        .unwrap_or(&lowered);
    without_scheme.trim_end_matches('/').nfc().collect()
}

/// Normalizes a title (or a filename) into a loose comparison key: NFC,
/// lowercased, with the filename-forbidden punctuation (both the ASCII forms and
/// the full-width stand-ins [`derive_basename`](super::mutations) writes) folded to
/// spaces and runs of whitespace collapsed. Folding lets a title compare equal to
/// the basename derived from it (`Fate/stay night` ⇔ `Fate／stay night`) and keeps
/// the match forgiving of punctuation differences. Returns an empty string for a
/// blank title (never a match key).
fn normalize_title(value: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    const FOLD: &[char] = &[
        '/', '\\', ':', '*', '?', '"', '<', '>', '|', '／', '＼', '：', '＊', '？', '＂', '＜',
        '＞', '｜',
    ];
    let mut out = String::new();
    let mut pending_space = false;
    for character in value.trim().nfc() {
        if character.is_whitespace() || FOLD.contains(&character) {
            pending_space = !out.is_empty();
            continue;
        }
        if pending_space {
            out.push(' ');
            pending_space = false;
        }
        out.extend(character.to_lowercase());
    }
    out
}

/// Reverse indices from the resident library, so a search result can be flagged
/// as already-in-library (and a quick-add can short-circuit to it). Pure and
/// library-only (no network), cheap to rebuild per request.
///
/// The external-ref index spans every type (a ref is precise, and the same work
/// may already exist under a different type). The title indices are keyed by
/// entity type so a loose title match only fires within the type being added —
/// same-titled works of different types aren't conflated.
#[derive(Default)]
pub(super) struct ExistingIndex {
    /// `(provider, normalized-ref) → entity` — the candidate's URL or source id.
    by_ref: HashMap<(&'static str, String), ExistingEntityRef>,
    /// `(type, normalized-title) → entity` for each entity's filename and its
    /// canonical (original-role) title — matched against *any* candidate title.
    by_title: HashMap<(String, String), ExistingEntityRef>,
    /// `(type, language, normalized-title) → entity` for each per-language title —
    /// matched only against the candidate's title *in the same language*.
    by_lang_title: HashMap<(String, String, String), ExistingEntityRef>,
}

pub(super) fn build_existing_index(library: &Library) -> ExistingIndex {
    let mut index = ExistingIndex::default();
    for summary in library.summaries() {
        let entity = ExistingEntityRef {
            id: summary.id.clone(),
            title: summary.title.clone(),
        };
        if let Some(type_config) = library.config.type_config(&summary.entity_type) {
            for (field_name, stored_value) in &summary.external_refs {
                let provider = type_config
                    .fields
                    .iter()
                    .find(|field| field.field == *field_name)
                    .and_then(|field| field.external_ref.as_deref())
                    .and_then(provider_for_external_ref);
                if let Some(provider) = provider {
                    index
                        .by_ref
                        .entry((provider, normalize_external_ref(stored_value)))
                        .or_insert_with(|| entity.clone());
                }
            }
        }
        // Filename and canonical title match against any candidate title.
        for title in [summary.basename.as_str(), summary.title.as_str()] {
            let key = normalize_title(title);
            if !key.is_empty() {
                index
                    .by_title
                    .entry((summary.entity_type.clone(), key))
                    .or_insert_with(|| entity.clone());
            }
        }
        // Per-language titles match same-language only.
        for (language, title) in &summary.titles {
            let key = normalize_title(title);
            if !key.is_empty() {
                index
                    .by_lang_title
                    .entry((summary.entity_type.clone(), language.clone(), key))
                    .or_insert_with(|| entity.clone());
            }
        }
    }
    index
}

/// The existing library entity a candidate (resolved for `entity_type`) already
/// maps to, if any — a deliberately loose "already in library" check. Checks 2–3
/// are scoped to `entity_type`:
///
/// 1. an external ref matching the candidate's provider + URL/source id (any type);
/// 2. the entity's filename or canonical title equal to any candidate title;
/// 3. a per-language title equal to the candidate's title in that same language.
pub(super) fn lookup_existing(
    index: &ExistingIndex,
    candidate: &ExternalCandidate,
    entity_type: &str,
) -> Option<ExistingEntityRef> {
    // 1. External ref — the precise signal, so it wins.
    if let Some(provider) = provider_for_external_ref(&candidate.provider) {
        for raw in [candidate.url.as_str(), candidate.source_id.as_str()] {
            if let Some(existing) = index.by_ref.get(&(provider, normalize_external_ref(raw))) {
                return Some(existing.clone());
            }
        }
    }

    // 2. Any candidate title equal to the entity's filename or canonical title.
    let candidate_titles = std::iter::once(candidate.title.as_str())
        .chain(candidate.original_title.as_deref())
        .chain(candidate.titles.values().map(String::as_str));
    for title in candidate_titles {
        let key = normalize_title(title);
        if key.is_empty() {
            continue;
        }
        if let Some(existing) = index.by_title.get(&(entity_type.to_string(), key)) {
            return Some(existing.clone());
        }
    }

    // 3. Same-language title match.
    for (language, title) in &candidate.titles {
        let key = normalize_title(title);
        if key.is_empty() {
            continue;
        }
        if let Some(existing) =
            index
                .by_lang_title
                .get(&(entity_type.to_string(), language.clone(), key))
        {
            return Some(existing.clone());
        }
    }

    None
}

/// Quick-add: create a library entity straight from an external search candidate.
/// Re-runs the schema mapping server-side, derives a safe filename (full-width
/// forbidden chars, collision-disambiguated), writes the entity, then downloads
/// covers and imports episodes — both fail-safe, so a flaky provider never undoes
/// the creation. If the candidate already resolves to a library entity, returns
/// that one untouched (`alreadyExisted`) so a raced double-click just navigates.
pub(crate) async fn quick_add_entity(
    State(state): State<AppState>,
    Json(request): Json<QuickAddRequest>,
) -> ApiResult<QuickAddResponse> {
    let library = require_content_writes(&state).await?;
    let type_config = type_config_or_err(&library.config, &request.entity_type)?;
    let candidate = &request.candidate;
    let vfs = state.vault_vfs(&library.config.vault_root);

    // Already in the library (external ref or a loose title match for this type)?
    // Return it, create nothing.
    let existing_index = build_existing_index(&library);
    if let Some(existing) = lookup_existing(&existing_index, candidate, &request.entity_type) {
        if let Some(record) = library.record_by_id(&existing.id) {
            let entity = load_entity(&library.config, vfs.as_ref(), &record.summary).await?;
            return Ok(Json(QuickAddResponse {
                entity,
                already_existed: true,
                basename_adjusted: false,
                cover: Vec::new(),
                episodes: None,
            }));
        }
    }

    // Re-map the candidate against the schema ourselves — never trust client values.
    let (mut frontmatter, body, mapped_fields) = build_mapped_document(candidate, type_config);
    // Quick Capture files things you intend to get to, so seed a "planning" status
    // when the schema models one and the candidate didn't already supply a value.
    apply_default_planning_status(&mut frontmatter, type_config);

    // Filename: the type's filename title language, falling back to the candidate
    // title, then the provider id. Collisions with a *different* work of the same
    // type are auto-disambiguated (` (year)`, then provider, then numeric).
    let (basename, basename_adjusted) = match request
        .basename
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(explicit) => (
            sanitize_basename(explicit)
                .map_err(|error| ApiError::bad_request(&error.to_string()))?,
            false,
        ),
        None => {
            let base = candidate_basename_base(candidate, type_config)?;
            let year = candidate_year(type_config, &mapped_fields);
            resolve_free_basename(
                vfs.as_ref(),
                &library.config.taxonomy_root,
                type_config,
                &library,
                &base,
                year.as_deref(),
                Some(&candidate.provider),
            )
            .await?
        }
    };

    let path = write_new_entity_file(
        vfs.as_ref(),
        &library.config.taxonomy_root,
        type_config,
        &basename,
        &frontmatter,
        &body,
    )
    .await?;
    state.invalidate_cache().await;

    let reloaded = get_library(&state).await?;
    let entity_id = reloaded
        .record_by_path(&path)
        .ok_or_else(|| ApiError::not_found("Created entity was not indexed"))?
        .summary
        .id
        .clone();

    // Fail-safe enrichment: a cover or episode failure leaves the entity intact
    // (the remote URL stays in frontmatter, episode errors are reported).
    let cover = download_new_entity_covers(&state, &reloaded, &entity_id)
        .await
        .unwrap_or_default();
    let episodes =
        import_new_entity_episodes(&state, &reloaded, &entity_id, request.language.as_deref())
            .await;

    state.invalidate_cache().await;
    let final_library = get_library(&state).await?;
    let record = final_library
        .record_by_id(&entity_id)
        .ok_or_else(|| ApiError::not_found("Created entity was not indexed"))?;
    let entity = load_entity(&final_library.config, vfs.as_ref(), &record.summary).await?;
    Ok(Json(QuickAddResponse {
        entity,
        already_existed: false,
        basename_adjusted,
        cover,
        episodes,
    }))
}

/// The title a new entity's filename is derived from: the type's filename title
/// language, else the candidate title, else the provider id. Returns a validated
/// basename or 400 if nothing usable remains.
pub(super) fn candidate_basename_base(
    candidate: &ExternalCandidate,
    type_config: &EntityTypeConfig,
) -> Result<String, ApiError> {
    let title = type_config
        .filename
        .as_ref()
        .and_then(|filename| filename.title_language.as_deref())
        .and_then(|language| candidate.titles.get(language))
        .map(String::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(candidate.title.as_str());
    derive_basename(title)
        .or_else(|| derive_basename(&candidate.source_id))
        .ok_or_else(|| ApiError::bad_request("Could not derive a filename from the candidate"))
}

/// The four-digit year from the type's date-role field value, for filename
/// disambiguation. Best-effort: the first run of four ASCII digits.
pub(super) fn candidate_year(
    type_config: &EntityTypeConfig,
    fields: &[MappedFieldValue],
) -> Option<String> {
    let date_field = type_config
        .fields
        .iter()
        .find(|field| field.date_role.is_some())?;
    let value = fields
        .iter()
        .find(|mapped| mapped.field == date_field.field)?
        .value
        .as_str()?;
    let digits: Vec<char> = value.chars().collect();
    digits
        .windows(4)
        .find(|window| window.iter().all(char::is_ascii_digit))
        .map(|window| window.iter().collect())
}

/// Builds the frontmatter map and body Markdown a candidate maps to for a type,
/// plus the resolved field list (for filename year derivation). The single
/// source of the quick-add / batch-import "candidate → new entity document"
/// step, so the two paths can't drift.
pub(super) fn build_mapped_document(
    candidate: &ExternalCandidate,
    type_config: &EntityTypeConfig,
) -> (Map<String, Value>, String, Vec<MappedFieldValue>) {
    let mapped = mapping::match_candidate(candidate.clone(), type_config);
    let mut frontmatter = Map::new();
    for field in &mapped.fields {
        if field.has_value {
            frontmatter.insert(field.field.clone(), field.value.clone());
        }
    }
    let mut sections = Vec::new();
    for section in &mapped.body_sections {
        if section.has_value {
            sections.push(format!(
                "## {}\n\n{}",
                section.heading.trim(),
                section.markdown.trim()
            ));
        }
    }
    let body = if sections.is_empty() {
        String::new()
    } else {
        format!("{}\n", sections.join("\n\n"))
    };
    (frontmatter, body, mapped.fields)
}

/// Seeds the type's status field with its first planning option when the mapped
/// candidate supplied none. Meaning is schema-driven: this only fires when the type
/// has an `enumRole: status` field that maps at least one planning option, and it
/// never overwrites a value the candidate already mapped. Scoped to Quick Capture —
/// batch import takes the status from the user's per-item choice instead.
fn apply_default_planning_status(
    frontmatter: &mut Map<String, Value>,
    type_config: &EntityTypeConfig,
) {
    let Some(field) = status_field(type_config) else {
        return;
    };
    if frontmatter.contains_key(field.field.as_str()) {
        return;
    }
    let Some(value) = field
        .status_values
        .as_ref()
        .and_then(|values| values.write_value(CanonicalStatus::Planning))
    else {
        return;
    };
    frontmatter.insert(field.field.clone(), Value::String(value.to_string()));
}

/// Resolves a full candidate for a provider from a `query` (a stored external-ref
/// URL or a free-text title), constrained to `external_types` (the source
/// bucket). Batch import uses this to fetch provider *detail* when the item it
/// carries lacks a metadata key the target type maps — the gap quick-add never
/// has (it echoes the search candidate). Prefers an exact URL/id match, else the
/// first result. `None` means the provider returned nothing.
pub(super) async fn resolve_candidate(
    state: &AppState,
    provider_id: &str,
    query: &str,
    external_types: &[String],
    language: Option<&str>,
) -> Result<Option<ExternalCandidate>, ApiError> {
    let registry = registry();
    let Some(provider_entry) = registry.iter().find(|entry| entry.id == provider_id) else {
        return Err(ApiError::bad_request("Unknown external provider"));
    };
    let mut config = ProviderSearchConfig::default();
    config.add_external_types(external_types);
    config.language = language
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let results = (provider_entry.search)(state, query, 1, 10, &config).await?;
    let normalized = normalize_external_ref(query);
    let exact = results.iter().position(|candidate| {
        normalize_external_ref(&candidate.url) == normalized || candidate.source_id == query
    });
    Ok(match exact {
        Some(index) => results.into_iter().nth(index),
        None => results.into_iter().next(),
    })
}

/// The `fields=` selection for a MyAnimeList user list, and the candidate builder
/// for one list entry — re-exported so the batch-import MAL adapter can request
/// rich entries and reuse the exact search-path mapping (the `mal` submodule is
/// private to this module).
pub(super) use mal::{mal_list_candidate, LIST_FIELDS as MAL_LIST_FIELDS};
/// Re-exported for the batch-import IMDB adapter, which resolves an IMDb id to a
/// TMDB ref (the `tmdb` submodule is private to this module).
pub(super) use tmdb::tmdb_find_imdb;

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
        // Populated per search by the handler when a provider's request fails.
        error: None,
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

/// How long a cached provider GET-by-id response stays fresh. Deliberately short:
/// this exists to *coalesce* the identical requests one action fans out, not to be
/// a real cache — the payloads are effectively static reference data.
const RESPONSE_CACHE_TTL: Duration = Duration::from_secs(300);

/// Process-wide, single-flighted cache for idempotent provider GET-by-id fetches
/// (see [`cached_json_get`]).
#[derive(Default)]
struct ResponseCache {
    entries: tokio::sync::Mutex<HashMap<String, (Instant, Arc<Value>)>>,
    locks: tokio::sync::Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
}

fn response_cache() -> &'static ResponseCache {
    static CACHE: OnceLock<ResponseCache> = OnceLock::new();
    CACHE.get_or_init(ResponseCache::default)
}

async fn cached_response(cache: &ResponseCache, key: &str) -> Option<Arc<Value>> {
    let entries = cache.entries.lock().await;
    entries.get(key).and_then(|(at, value)| {
        (Instant::now().duration_since(*at) < RESPONSE_CACHE_TTL).then(|| Arc::clone(value))
    })
}

/// Returns a fresh cached response for `key`, else runs `fetch` — single-flighted
/// per key so concurrent identical requests collapse to one call. The motivating
/// case: a pasted provider URL is searched once per entity type that maps the
/// provider, and every one resolves the *same* record; without this that's N
/// identical requests for one paste, an easy way to trip a provider's rate limit.
/// `key` must uniquely identify the request (its URL). Only successful responses
/// are cached, so a transient failure is retried rather than remembered.
pub(super) async fn cached_json_get<F, Fut>(key: &str, fetch: F) -> Result<Arc<Value>, ApiError>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<Value, ApiError>>,
{
    let cache = response_cache();
    if let Some(value) = cached_response(cache, key).await {
        return Ok(value);
    }
    // Take the per-key lock so concurrent callers for the same URL wait here and
    // reuse the first fetch's result instead of each hitting the provider.
    let lock = {
        let mut locks = cache.locks.lock().await;
        Arc::clone(locks.entry(key.to_string()).or_default())
    };
    let _guard = lock.lock().await;
    // Another caller may have populated the cache while we waited for the lock.
    if let Some(value) = cached_response(cache, key).await {
        return Ok(value);
    }
    let value = Arc::new(fetch().await?);
    {
        let mut entries = cache.entries.lock().await;
        let now = Instant::now();
        entries.retain(|_, (at, _)| now.duration_since(*at) < RESPONSE_CACHE_TTL);
        entries.insert(key.to_string(), (now, Arc::clone(&value)));
    }
    // The result is cached now, so no later caller needs this per-key lock; drop it
    // to keep the lock map from growing with every distinct URL. Callers already
    // waiting hold their own clone, so they still re-check the (now-populated) cache.
    cache.locks.lock().await.remove(key);
    Ok(value)
}

/// Single-flighted cached-token acquisition shared by the OAuth/login providers
/// (IGDB, TheTVDB). Returns the cached access token when still fresh;
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

/// The most body we fold into the error message. Provider error payloads are
/// usually a short JSON object (`{"status_message":"Invalid API key"}`); this
/// caps a runaway HTML error page from bloating a toast.
const MAX_ERROR_BODY: usize = 500;

/// Builds a gateway error carrying the exact upstream HTTP status and a
/// truncated copy of the provider's response body, so the concrete reason
/// (`401 Unauthorized`, a rate-limit message, an "invalid api key" payload)
/// reaches the user instead of a generic "request failed". Unlike the request
/// URL, the response body is the provider's own error text and safe to surface.
fn provider_status_error(status: reqwest::StatusCode, body: &str) -> ApiError {
    let mut message = match status.canonical_reason() {
        Some(reason) => format!(
            "The external provider returned HTTP {} {reason}",
            status.as_u16()
        ),
        None => format!("The external provider returned HTTP {}", status.as_u16()),
    };
    let body = body.trim();
    if !body.is_empty() {
        let snippet: String = body.chars().take(MAX_ERROR_BODY).collect();
        message.push_str(": ");
        message.push_str(&snippet);
        if body.chars().count() > MAX_ERROR_BODY {
            message.push('…');
        }
    }
    eprintln!("{message}");
    ApiError::bad_gateway(&message)
}

/// Extension on [`reqwest::Response`] that, unlike
/// [`reqwest::Response::error_for_status`], reads the response body on a
/// non-success status so the concrete status + body can be surfaced to the user
/// (see [`provider_status_error`]). Use this in provider request paths in place
/// of `error_for_status().map_err(provider_error)`.
pub(super) trait ProviderResponseExt: Sized {
    async fn error_for_status_body(self) -> Result<reqwest::Response, ApiError>;
}

impl ProviderResponseExt for reqwest::Response {
    async fn error_for_status_body(self) -> Result<reqwest::Response, ApiError> {
        let status = self.status();
        if status.is_success() {
            return Ok(self);
        }
        // Read the body before discarding the response; `error_for_status`
        // would drop it, losing the provider's own explanation.
        let body = self.text().await.unwrap_or_default();
        Err(provider_status_error(status, &body))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{
        BodySection, BodySectionKind, EntityTypeConfig, ExternalFieldMapping, FieldConfig,
    };

    /// The single registry entry whose `recognizes_url` claims `url`, if any.
    fn url_owner(url: &str) -> Option<&'static str> {
        let owners: Vec<&'static str> = registry()
            .iter()
            .filter(|entry| (entry.recognizes_url)(url))
            .map(|entry| entry.id)
            .collect();
        // A URL must be owned by at most one provider, or Quick Capture routing is
        // ambiguous. Assert that here so a lenient recognizer is caught early.
        assert!(
            owners.len() <= 1,
            "url claimed by multiple providers: {owners:?}"
        );
        owners.first().copied()
    }

    #[test]
    fn each_provider_url_is_claimed_only_by_its_owner() {
        let cases = [
            ("https://bgm.tv/subject/998877", "bangumi"),
            ("https://www.igdb.com/games/hollow-knight", "igdb"),
            ("https://thetvdb.com/series/answer-me-1988", "thetvdb"),
            (
                "https://books.google.com/books?id=zyTCAlFPjgYC",
                "googlebooks",
            ),
            ("https://openlibrary.org/works/OL45804W", "openlibrary"),
            (
                "https://podcasts.apple.com/us/podcast/x/id1535809341",
                "applepodcast",
            ),
            (
                "https://store.steampowered.com/app/367520/Hollow_Knight/",
                "steam",
            ),
            (
                "https://musicbrainz.org/release/12345678-1234-1234-1234-123456789012",
                "musicbrainz",
            ),
            (
                "https://boardgamegeek.com/boardgame/174430/gloomhaven",
                "bgg",
            ),
            ("https://www.themoviedb.org/movie/27205", "tmdb"),
            (
                "https://music.apple.com/us/album/thriller/269572838",
                "applemusic",
            ),
            (
                "https://www.discogs.com/release/249504-Rick-Astley",
                "discogs",
            ),
            ("https://myanimelist.net/anime/5114/", "myanimelist"),
            (
                "https://www.mangaupdates.com/series/153046/name",
                "mangaupdates",
            ),
            (
                "https://comicvine.gamespot.com/volume/4050-18166/",
                "comicvine",
            ),
            ("https://hardcover.app/books/the-hobbit", "hardcover"),
        ];
        for (url, expected) in cases {
            assert_eq!(url_owner(url), Some(expected), "for {url}");
        }
    }

    #[test]
    fn foreign_urls_are_claimed_by_no_provider() {
        // Comic Vine's id parser accepts any digit-tailed path, so a bare-parser
        // recognizer would misclaim these; the host-anchored recognizer must not.
        for url in [
            "https://bgm.tv/subject/998877",  // digit-tailed, but bangumi's
            "https://example.com/foo/123456", // digit-tailed foreign URL
            "https://letterboxd.com/film/parasite-2019/",
            "https://en.wikipedia.org/wiki/Gloomhaven",
        ] {
            assert_ne!(url_owner(url), Some("comicvine"), "for {url}");
        }
        // Wholly unknown hosts belong to nobody → Quick Capture searches nothing.
        assert_eq!(url_owner("https://example.com/foo/123456"), None);
        assert_eq!(url_owner("https://nintendo.com/store/games/x"), None);
    }

    #[tokio::test]
    async fn cached_json_get_coalesces_concurrent_identical_fetches() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let calls = Arc::new(AtomicUsize::new(0));
        // A unique key so the process-wide cache can't collide with another test.
        let key = "test://coalesce/subject/541285";
        let futures = (0..5).map(|_| {
            let calls = Arc::clone(&calls);
            async move {
                cached_json_get(key, || {
                    let calls = Arc::clone(&calls);
                    async move {
                        // Yield so the other four callers reach the per-key lock
                        // before this fetch completes — exercising single-flight,
                        // not just cache reuse.
                        tokio::task::yield_now().await;
                        calls.fetch_add(1, Ordering::SeqCst);
                        Ok(serde_json::json!({ "id": 541285 }))
                    }
                })
                .await
            }
        });
        let results = futures_util::future::join_all(futures).await;
        assert!(results.iter().all(|result| result.is_ok()));
        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "five concurrent identical GETs must fetch once"
        );
        let first = results[0].as_ref().ok().expect("first get ok");
        assert_eq!(first["id"], 541285);
    }

    #[test]
    fn url_type_allowed_filters_only_explicitly_constrained_fields() {
        // Unconstrained (no externalTypes) accepts any pasted type — a single field
        // keeps resolving a pasted URL of any kind.
        let mut unconstrained = ProviderSearchConfig::default();
        unconstrained.add_unconstrained_source_if_empty();
        assert!(url_type_allowed(&unconstrained, "movie"));
        assert!(url_type_allowed(&unconstrained, "artist"));

        // A field constrained to `tv` rejects a pasted `/movie/` URL (so the movie
        // won't surface under this type too) but accepts `tv`, case-insensitively.
        let mut tv_only = ProviderSearchConfig::default();
        tv_only.add_external_types(&["tv".to_string()]);
        assert!(url_type_allowed(&tv_only, "tv"));
        assert!(url_type_allowed(&tv_only, "TV"));
        assert!(!url_type_allowed(&tv_only, "movie"));
    }

    #[test]
    fn query_is_url_detects_http_and_https_only() {
        assert!(query_is_url("https://bgm.tv/subject/1"));
        assert!(query_is_url("http://bgm.tv/subject/1"));
        assert!(!query_is_url("bgm.tv/subject/1"));
        assert!(!query_is_url("Hollow Knight"));
        assert!(!query_is_url("ftp://example.com/x"));
    }

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
                enum_role: None,
                status_values: None,
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

    fn candidate(provider: &str, source_id: &str, url: &str) -> ExternalCandidate {
        ExternalCandidate {
            provider: provider.to_string(),
            source_id: source_id.to_string(),
            url: url.to_string(),
            title: "Some Title".to_string(),
            original_title: None,
            brief: None,
            cover_url: None,
            titles: BTreeMap::new(),
            metadata: Map::new(),
        }
    }

    fn titled_candidate(
        title: &str,
        original: Option<&str>,
        titles: &[(&str, &str)],
    ) -> ExternalCandidate {
        ExternalCandidate {
            provider: "bangumi".to_string(),
            source_id: "x".to_string(),
            url: "https://bgm.tv/subject/x".to_string(),
            title: title.to_string(),
            original_title: original.map(str::to_string),
            brief: None,
            cover_url: None,
            titles: titles
                .iter()
                .map(|(language, value)| (language.to_string(), value.to_string()))
                .collect(),
            metadata: Map::new(),
        }
    }

    #[test]
    fn normalize_external_ref_is_scheme_slash_and_case_insensitive() {
        assert_eq!(
            normalize_external_ref("HTTPS://Bgm.tv/subject/123/"),
            normalize_external_ref("http://bgm.tv/subject/123")
        );
        assert_eq!(normalize_external_ref("  12345 "), "12345");
    }

    #[test]
    fn normalize_title_folds_forbidden_punctuation_and_case() {
        // The full-width basename form folds equal to the ASCII title form.
        assert_eq!(
            normalize_title("Fate/stay night"),
            normalize_title("Fate／stay night")
        );
        assert_eq!(normalize_title("  Re:ZERO  "), "re zero");
        assert_eq!(normalize_title("A   B"), "a b");
        assert_eq!(normalize_title("   "), "");
    }

    #[test]
    fn lookup_existing_matches_on_url_or_source_id_and_provider() {
        let mut index = ExistingIndex::default();
        index.by_ref.insert(
            (
                "bangumi",
                normalize_external_ref("https://bgm.tv/subject/123"),
            ),
            ExistingEntityRef {
                id: "anime:Foo".to_string(),
                title: "Foo".to_string(),
            },
        );
        // A hand-edited bare id stored for another entity.
        index.by_ref.insert(
            ("igdb", normalize_external_ref("456")),
            ExistingEntityRef {
                id: "game:Bar".to_string(),
                title: "Bar".to_string(),
            },
        );

        // URL match (scheme-insensitive), correct provider — refs match cross-type.
        let hit = lookup_existing(
            &index,
            &candidate("bangumi", "123", "http://bgm.tv/subject/123"),
            "anime",
        );
        assert_eq!(hit.unwrap().id, "anime:Foo");

        // source-id match against a bare-id ref.
        let hit = lookup_existing(
            &index,
            &candidate("igdb", "456", "https://igdb.com/games/bar"),
            "game",
        );
        assert_eq!(hit.unwrap().id, "game:Bar");

        // Right value, wrong provider → no match (cross-provider false positives).
        assert!(lookup_existing(
            &index,
            &candidate("mal", "123", "http://bgm.tv/subject/123"),
            "anime",
        )
        .is_none());

        // Unknown candidate.
        assert!(lookup_existing(
            &index,
            &candidate("bangumi", "999", "http://bgm.tv/subject/999"),
            "anime",
        )
        .is_none());
    }

    #[test]
    fn lookup_existing_matches_filename_against_any_candidate_title_within_type() {
        let mut index = ExistingIndex::default();
        // An entity whose filename carries the full-width form of the title.
        index.by_title.insert(
            ("anime".to_string(), normalize_title("Fate／stay night")),
            ExistingEntityRef {
                id: "anime:Fate".to_string(),
                title: "Fate".to_string(),
            },
        );
        let cand = titled_candidate("Fate/stay night", None, &[("ja", "フェイト")]);
        assert_eq!(
            lookup_existing(&index, &cand, "anime").unwrap().id,
            "anime:Fate"
        );
        // Type-scoped: the same title under a different type is not "in library".
        assert!(lookup_existing(&index, &cand, "manga").is_none());
    }

    #[test]
    fn lookup_existing_matches_same_language_title_only() {
        let mut index = ExistingIndex::default();
        index.by_lang_title.insert(
            (
                "anime".to_string(),
                "ja".to_string(),
                normalize_title("鋼の錬金術師"),
            ),
            ExistingEntityRef {
                id: "anime:FMA".to_string(),
                title: "FMA".to_string(),
            },
        );
        // Same-language (ja) title matches.
        let cand = titled_candidate(
            "Fullmetal Alchemist",
            None,
            &[("ja", "鋼の錬金術師"), ("en", "Fullmetal Alchemist")],
        );
        assert_eq!(
            lookup_existing(&index, &cand, "anime").unwrap().id,
            "anime:FMA"
        );
        // The same string under a *different* language must not match.
        let cross = titled_candidate("x", None, &[("en", "鋼の錬金術師")]);
        assert!(lookup_existing(&index, &cross, "anime").is_none());
    }
}
