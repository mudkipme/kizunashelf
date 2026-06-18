//! The single source of truth for the title-language options offered in the
//! schema editor. These are the languages TheTVDB supports (matching the
//! provider that drives much of the metadata), exposed to every frontend through
//! `GET /api/languages` so web and iOS share one list.
//!
//! Title language on a field is stored as a free-form ISO 639-1 string (the core
//! never restricts it), so this list is a convenience/suggestion set — a vault
//! may still carry a code outside it.

use crate::contract::Language;

/// `(code, English label)`, common languages first then alphabetical. Mirrors
/// TheTVDB's supported set: <https://thetvdb-api.readthedocs.io/api/languages.html>.
const SUPPORTED_LANGUAGES: &[(&str, &str)] = &[
    ("en", "English"),
    ("zh", "Chinese"),
    ("ja", "Japanese"),
    ("ko", "Korean"),
    ("hr", "Croatian"),
    ("cs", "Czech"),
    ("da", "Danish"),
    ("nl", "Dutch"),
    ("fi", "Finnish"),
    ("fr", "French"),
    ("de", "German"),
    ("el", "Greek"),
    ("he", "Hebrew"),
    ("hu", "Hungarian"),
    ("it", "Italian"),
    ("no", "Norwegian"),
    ("pl", "Polish"),
    ("pt", "Portuguese"),
    ("ru", "Russian"),
    ("sl", "Slovenian"),
    ("es", "Spanish"),
    ("sv", "Swedish"),
    ("tr", "Turkish"),
];

/// The supported title languages as contract records.
pub fn supported_languages() -> Vec<Language> {
    SUPPORTED_LANGUAGES
        .iter()
        .map(|(code, label)| Language {
            code: (*code).to_string(),
            label: (*label).to_string(),
        })
        .collect()
}
