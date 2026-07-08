//! The single source of truth for the title-language options offered in the
//! schema editor, and for the user-language options offered in the clients'
//! language picker. Both are exposed to every frontend through
//! `GET /api/languages` so web and iOS share one list.
//!
//! Title language on a field is stored as a free-form ISO 639-1 string (the core
//! never restricts it), so this list is a convenience/suggestion set — a vault
//! may still carry a code outside it.
//!
//! The two lists differ only in how Chinese is presented. The user preference
//! distinguishes `zh-Hans` / `zh-Hant` (the UI must), but script subtags exist
//! in exactly two places: the UI locale and outbound requests to providers that
//! distinguish them. Everywhere data is stored, keyed, or matched — frontmatter
//! `titles`, schema `titleLanguage`, candidate title maps, the dedup index —
//! Chinese is always bare `zh` (see `docs/i18n-plan.md`).

use crate::contract::{Language, UserLanguage};

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

/// `(preference code, endonym label, title language)` — the options for the
/// clients' single language picker, one per entry in [`SUPPORTED_LANGUAGES`]
/// except Chinese, which splits into `zh-Hans`/`zh-Hant` (both mapping to the
/// bare `zh` title language). Labels are endonyms because a language picker must
/// be readable to someone stuck in the wrong language. Whether the UI is
/// *translated* into a given code is deliberately not here: that's a per-client
/// build fact (web and iOS ship different catalogs, and the set changes over
/// time), so each client layers it on from its own shipped-locale set.
const USER_LANGUAGES: &[(&str, &str, &str)] = &[
    ("en", "English", "en"),
    ("zh-Hans", "简体中文", "zh"),
    ("zh-Hant", "繁體中文", "zh"),
    ("ja", "日本語", "ja"),
    ("ko", "한국어", "ko"),
    ("hr", "Hrvatski", "hr"),
    ("cs", "Čeština", "cs"),
    ("da", "Dansk", "da"),
    ("nl", "Nederlands", "nl"),
    ("fi", "Suomi", "fi"),
    ("fr", "Français", "fr"),
    ("de", "Deutsch", "de"),
    ("el", "Ελληνικά", "el"),
    ("he", "עברית", "he"),
    ("hu", "Magyar", "hu"),
    ("it", "Italiano", "it"),
    ("no", "Norsk", "no"),
    ("pl", "Polski", "pl"),
    ("pt", "Português", "pt"),
    ("ru", "Русский", "ru"),
    ("sl", "Slovenščina", "sl"),
    ("es", "Español", "es"),
    ("sv", "Svenska", "sv"),
    ("tr", "Türkçe", "tr"),
];

/// The user-language preference options as contract records.
pub fn user_languages() -> Vec<UserLanguage> {
    USER_LANGUAGES
        .iter()
        .map(|(code, label, title_language)| UserLanguage {
            code: (*code).to_string(),
            label: (*label).to_string(),
            title_language: (*title_language).to_string(),
        })
        .collect()
}

/// The primary subtag of a BCP-47-ish language code, lowercased: the bare
/// ISO 639 title/content language a user preference maps to (`zh-Hans` → `zh`,
/// `en-US` → `en`). This is the one derivation that keeps script subtags out of
/// stored data — anything that keys or matches titles goes through it.
pub fn primary_language(code: &str) -> String {
    code.trim()
        .split(['-', '_'])
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase()
}

/// The TheTVDB ISO 639-2/T code for a title language (e.g. `zh` → `zho`), used
/// to request translated episode titles. Subtags are ignored (`zh-Hant` → `zho`
/// — TheTVDB has a single Chinese bucket). `None` for an unknown code.
pub fn thetvdb_language(code: &str) -> Option<&'static str> {
    let code = primary_language(code);
    SUPPORTED_LANGUAGES
        .iter()
        .find(|(iso, _, _)| *iso == code)
        .map(|(_, _, tvdb)| *tvdb)
}

/// The inverse of [`thetvdb_language`]: the ISO 639-1 title language for a
/// TheTVDB 3-letter code (e.g. `zho` → `zh`), used to tag the language-keyed
/// titles in a TheTVDB translations map. `None` for a code outside our set.
pub fn thetvdb_iso_language(tvdb_code: &str) -> Option<&'static str> {
    let tvdb_code = tvdb_code.trim().to_ascii_lowercase();
    SUPPORTED_LANGUAGES
        .iter()
        .find(|(_, _, tvdb)| *tvdb == tvdb_code)
        .map(|(iso, _, _)| *iso)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn primary_language_strips_subtags() {
        assert_eq!(primary_language("zh-Hans"), "zh");
        assert_eq!(primary_language("zh-Hant"), "zh");
        assert_eq!(primary_language("en-US"), "en");
        assert_eq!(primary_language(" JA "), "ja");
        assert_eq!(primary_language("pt_BR"), "pt");
        assert_eq!(primary_language(""), "");
    }

    #[test]
    fn thetvdb_language_ignores_subtags() {
        assert_eq!(thetvdb_language("zh"), Some("zho"));
        assert_eq!(thetvdb_language("zh-Hans"), Some("zho"));
        assert_eq!(thetvdb_language("zh-Hant"), Some("zho"));
        assert_eq!(thetvdb_language("ja"), Some("jpn"));
        assert_eq!(thetvdb_language("xx"), None);
    }

    #[test]
    fn thetvdb_iso_language_reverses_the_mapping() {
        assert_eq!(thetvdb_iso_language("zho"), Some("zh"));
        assert_eq!(thetvdb_iso_language("jpn"), Some("ja"));
        assert_eq!(thetvdb_iso_language("ENG"), Some("en"));
        assert_eq!(thetvdb_iso_language("xxx"), None);
        // Round-trips for every supported language.
        for (iso, _, tvdb) in SUPPORTED_LANGUAGES {
            assert_eq!(thetvdb_language(iso), Some(*tvdb));
            assert_eq!(thetvdb_iso_language(tvdb), Some(*iso));
        }
    }

    #[test]
    fn user_languages_split_chinese_and_cover_every_title_language() {
        let user = user_languages();
        assert!(user.iter().all(|entry| entry.code != "zh"));
        for code in ["zh-Hans", "zh-Hant"] {
            let entry = user.iter().find(|entry| entry.code == code).unwrap();
            assert_eq!(entry.title_language, "zh");
        }
        // The two tables must not drift: every schema title language is reachable
        // from some preference, and every preference maps to a schema language.
        let title_codes: BTreeSet<&str> = SUPPORTED_LANGUAGES
            .iter()
            .map(|(code, _, _)| *code)
            .collect();
        let mapped: BTreeSet<&str> = USER_LANGUAGES
            .iter()
            .map(|(_, _, title_language)| *title_language)
            .collect();
        assert_eq!(title_codes, mapped);
        // A preference's title language is always its own primary subtag.
        for (code, _, title_language) in USER_LANGUAGES {
            assert_eq!(primary_language(code), *title_language);
        }
    }
}
