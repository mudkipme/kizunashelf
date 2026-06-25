//! The single source of truth for the title-language options offered in the
//! schema editor. These are the languages TheTVDB supports (matching the
//! provider that drives much of the metadata), exposed to every frontend through
//! `GET /api/languages` so web and iOS share one list.
//!
//! Title language on a field is stored as a free-form ISO 639-1 string (the core
//! never restricts it), so this list is a convenience/suggestion set — a vault
//! may still carry a code outside it.

use crate::contract::Language;

/// `(ISO 639-1 code, English label, TheTVDB ISO 639-2/T code)`, common languages
/// first then alphabetical. Mirrors TheTVDB's supported set:
/// <https://thetvdb-api.readthedocs.io/api/languages.html>. The third column maps
/// our 2-letter code to the 3-letter code TheTVDB's v4 translation endpoints want.
const SUPPORTED_LANGUAGES: &[(&str, &str, &str)] = &[
    ("en", "English", "eng"),
    ("zh", "Chinese", "zho"),
    ("ja", "Japanese", "jpn"),
    ("ko", "Korean", "kor"),
    ("hr", "Croatian", "hrv"),
    ("cs", "Czech", "ces"),
    ("da", "Danish", "dan"),
    ("nl", "Dutch", "nld"),
    ("fi", "Finnish", "fin"),
    ("fr", "French", "fra"),
    ("de", "German", "deu"),
    ("el", "Greek", "ell"),
    ("he", "Hebrew", "heb"),
    ("hu", "Hungarian", "hun"),
    ("it", "Italian", "ita"),
    ("no", "Norwegian", "nor"),
    ("pl", "Polish", "pol"),
    ("pt", "Portuguese", "por"),
    ("ru", "Russian", "rus"),
    ("sl", "Slovenian", "slv"),
    ("es", "Spanish", "spa"),
    ("sv", "Swedish", "swe"),
    ("tr", "Turkish", "tur"),
];

/// The supported title languages as contract records.
pub fn supported_languages() -> Vec<Language> {
    SUPPORTED_LANGUAGES
        .iter()
        .map(|(code, label, _)| Language {
            code: (*code).to_string(),
            label: (*label).to_string(),
        })
        .collect()
}

/// The TheTVDB ISO 639-2/T code for an ISO 639-1 title language (e.g. `zh` → `zho`),
/// used to request translated episode titles. `None` for an unknown code.
pub fn thetvdb_language(code: &str) -> Option<&'static str> {
    let code = code.trim().to_ascii_lowercase();
    SUPPORTED_LANGUAGES
        .iter()
        .find(|(iso, _, _)| *iso == code)
        .map(|(_, _, tvdb)| *tvdb)
}
