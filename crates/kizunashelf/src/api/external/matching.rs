//! Library duplicate matching shared by search, quick-add, and imports.
//! External references match across types; loose titles stay type- and
//! language-scoped. This module performs no network or vault I/O.

use super::provider_for_external_ref;
use crate::contract::{ExistingEntityRef, ExternalCandidate};
use crate::types::Library;
use std::collections::HashMap;

/// Normalizes an external-ref value (a stored URL/id, or a candidate's URL/id)
/// into a comparison key: scheme- and case-insensitive, no trailing slash, NFC.
/// Lets a candidate match a hand-edited ref regardless of `http`/`https` or a
/// bare-id vs full-URL storage style.
pub(super) fn normalize_external_ref(value: &str) -> String {
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
/// the full-width stand-ins [`derive_basename`](crate::api::mutations::derive_basename) writes) folded to
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
pub(in crate::api) struct ExistingIndex {
    /// `(provider, normalized-ref) → entity` — the candidate's URL or source id.
    by_ref: HashMap<(&'static str, String), ExistingEntityRef>,
    /// `(type, normalized-title) → entity` for each entity's filename and its
    /// canonical (original-role) title — matched against *any* candidate title.
    by_title: HashMap<(String, String), ExistingEntityRef>,
    /// `(type, language, normalized-title) → entity` for each per-language title —
    /// matched only against the candidate's title *in the same language*.
    by_lang_title: HashMap<(String, String, String), ExistingEntityRef>,
}

pub(in crate::api) fn build_existing_index(library: &Library) -> ExistingIndex {
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
pub(in crate::api) fn lookup_existing(
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Map;

    fn candidate(provider: &str, source_id: &str, url: &str) -> ExternalCandidate {
        ExternalCandidate {
            provider: provider.to_string(),
            source_id: source_id.to_string(),
            url: url.to_string(),
            ..titled_candidate("Some Title", None, &[])
        }
    }

    fn titled_candidate(
        title: &str,
        original: Option<&str>,
        titles: &[(&str, &str)],
    ) -> ExternalCandidate {
        ExternalCandidate {
            needs_detail: false,
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
