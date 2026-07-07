//! Import sources and their registry. Mirrors the external-provider registry
//! (`api/external.rs`): each source is a trait impl erased to fn pointers so the
//! orchestration iterates a `Vec<SourceEntry>` — adding a source is one line in
//! [`registry`].

mod bangumi;
mod yamtrack;

use super::model::ImportItem;
use crate::api::error::ApiError;
use crate::api::external::CredentialSpec;
use crate::api::state::AppState;
use crate::contract::{ImportInput, ImportInputKind};
use std::future::Future;
use std::pin::Pin;

pub(super) trait ImportSource {
    const ID: &'static str;
    const LABEL: &'static str;
    const INPUT: ImportInputKind;
    /// Human label for the input field ("Bangumi username", "Yamtrack CSV export").
    const INPUT_LABEL: &'static str;

    /// Credentials this source needs. Empty (the default) means none.
    fn credentials() -> &'static [CredentialSpec] {
        &[]
    }

    /// Provider ids this source resolves items to.
    fn providers() -> &'static [&'static str];

    fn available(_state: &AppState) -> bool {
        true
    }

    fn unavailable_reason(_state: &AppState) -> Option<String> {
        None
    }

    /// Fetches the user's library and normalizes it to [`ImportItem`]s. All
    /// source-specific vocabulary (status, score scale, dates) is translated here.
    fn fetch(
        state: &AppState,
        input: &ImportInput,
    ) -> impl Future<Output = Result<Vec<ImportItem>, ApiError>> + Send;
}

pub(super) type FetchFut<'a> =
    Pin<Box<dyn Future<Output = Result<Vec<ImportItem>, ApiError>> + Send + 'a>>;

/// The erased fetch fn pointer a [`SourceEntry`] holds; also the type the plan
/// worker takes so it can drive any source uniformly.
pub(super) type SourceFetchFn = for<'a> fn(&'a AppState, &'a ImportInput) -> FetchFut<'a>;

/// One source, erased to plain fn pointers so the registry is non-generic.
pub(super) struct SourceEntry {
    pub id: &'static str,
    pub label: &'static str,
    pub input: ImportInputKind,
    pub input_label: &'static str,
    pub credentials: &'static [CredentialSpec],
    pub providers: &'static [&'static str],
    pub available: fn(&AppState) -> bool,
    pub unavailable_reason: fn(&AppState) -> Option<String>,
    pub fetch: SourceFetchFn,
}

fn fetch_boxed<'a, S: ImportSource + 'static>(
    state: &'a AppState,
    input: &'a ImportInput,
) -> FetchFut<'a> {
    Box::pin(S::fetch(state, input))
}

fn entry<S: ImportSource + 'static>() -> SourceEntry {
    SourceEntry {
        id: S::ID,
        label: S::LABEL,
        input: S::INPUT,
        input_label: S::INPUT_LABEL,
        credentials: S::credentials(),
        providers: S::providers(),
        available: S::available,
        unavailable_reason: S::unavailable_reason,
        fetch: fetch_boxed::<S>,
    }
}

/// The import-source registry: the single source of truth for which sources
/// exist. Every other function derives from this.
pub(super) fn registry() -> Vec<SourceEntry> {
    vec![
        entry::<bangumi::BangumiSource>(),
        entry::<yamtrack::YamtrackSource>(),
    ]
}
