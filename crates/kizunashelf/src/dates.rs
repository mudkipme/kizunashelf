use regex::Regex;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ParsedEntityDate {
    pub year: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub month: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub day: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub season: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub season_key: Option<String>,
}

pub fn parse_entity_date(value: Option<&str>) -> Option<ParsedEntityDate> {
    let value = value?;
    if let Some((year, month, day)) = exact_date_parts(Some(value)) {
        return Some(ParsedEntityDate {
            year,
            month: Some(month),
            day: Some(day),
            season: Some(season_for_month(month).to_string()),
            season_key: Some(season_key_for_month(month).to_string()),
        });
    }

    let year = year_regex()
        .find(value)
        .or_else(|| year_zh_regex().find(value))?
        .as_str()
        .chars()
        .take(4)
        .collect::<String>()
        .parse::<i32>()
        .ok()?;
    let month = year_month_regex()
        .captures(value)
        .and_then(|captures| captures.get(2))
        .and_then(|month| month.as_str().parse::<u32>().ok())
        .map(|month| clamp_number(month as f64, 1, 12) as u32);
    let season =
        parse_season_name(value).or_else(|| month.map(|month| season_for_month(month).to_string()));
    let season_key = season
        .as_deref()
        .and_then(normalize_season_name)
        .or_else(|| month.map(|month| season_key_for_month(month).to_string()));

    Some(ParsedEntityDate {
        year,
        month: month.or_else(|| season_key.as_deref().and_then(season_start_month)),
        day: None,
        season,
        season_key,
    })
}

pub fn parsed_date_sort_key(value: Option<&str>) -> Option<String> {
    let parsed = parse_entity_date(value)?;
    normalize_date(
        parsed.year,
        parsed.month.unwrap_or(1),
        parsed.day.unwrap_or(1),
    )
}

pub fn date_sort_key(value: Option<&str>) -> Option<String> {
    let value = value?;
    if let Some(exact) = parse_exact_date(Some(value)) {
        return Some(exact);
    }

    let parsed = parse_entity_date(Some(value))?;
    let season = parsed.season.as_deref()?;
    let (month, day) = season_end_date(season)?;
    normalize_date(parsed.year, month, day).or_else(|| Some(value.to_string()))
}

pub fn season_compare_value(season: &str) -> i32 {
    match normalize_season_name(season).as_deref() {
        Some("winter") => 0,
        Some("spring") => 1,
        Some("summer") => 2,
        Some("autumn") => 3,
        _ => i32::MIN,
    }
}

pub fn parse_exact_date(value: Option<&str>) -> Option<String> {
    let (year, month, day) = exact_date_parts(value)?;
    normalize_date(year, month, day)
}

/// Extracts the calendar date (`YYYY-MM-DD`) from the *start* of an ISO-8601 date
/// or date-time (e.g. `2023-04-16` or `2023-04-16T07:00:00Z`), validating it.
/// Returns `None` for an empty string or a non-date prefix. Used to normalize
/// provider air/release dates before they become an episode's `📅` suffix.
pub fn iso_date(value: &str) -> Option<String> {
    let captures = iso_date_regex().captures(value.trim())?;
    let year = captures.get(1)?.as_str().parse().ok()?;
    let month = captures.get(2)?.as_str().parse().ok()?;
    let day = captures.get(3)?.as_str().parse().ok()?;
    normalize_date(year, month, day)
}

pub fn exact_date_parts(value: Option<&str>) -> Option<(i32, u32, u32)> {
    let value = value?;
    let captures = exact_date_regex().captures(value)?;
    let year = captures
        .get(1)
        .or_else(|| captures.get(4))?
        .as_str()
        .parse()
        .ok()?;
    let month = captures
        .get(2)
        .or_else(|| captures.get(5))?
        .as_str()
        .parse()
        .ok()?;
    let day = captures
        .get(3)
        .or_else(|| captures.get(6))?
        .as_str()
        .parse()
        .ok()?;
    Some((year, month, day))
}

pub fn normalize_date(year: i32, month: u32, day: u32) -> Option<String> {
    chrono::NaiveDate::from_ymd_opt(year, month, day)
        .map(|_| format!("{year:04}-{month:02}-{day:02}"))
}

pub fn is_in_month(date: &str, year: i32, month: u32) -> bool {
    date.starts_with(&format!("{year:04}-{month:02}-"))
}

pub fn clamp_number(value: f64, min: i64, max: i64) -> i64 {
    if !value.is_finite() {
        return min;
    }
    (value.floor() as i64).clamp(min, max)
}

fn season_for_month(month: u32) -> &'static str {
    if month <= 3 {
        "冬季"
    } else if month <= 6 {
        "春季"
    } else if month <= 9 {
        "夏季"
    } else {
        "秋季"
    }
}

fn season_key_for_month(month: u32) -> &'static str {
    if month <= 3 {
        "winter"
    } else if month <= 6 {
        "spring"
    } else if month <= 9 {
        "summer"
    } else {
        "autumn"
    }
}

fn season_start_month(season: &str) -> Option<u32> {
    match season {
        "winter" => Some(1),
        "spring" => Some(4),
        "summer" => Some(7),
        "autumn" => Some(10),
        _ => None,
    }
}

fn season_end_date(season: &str) -> Option<(u32, u32)> {
    match normalize_season_name(season).as_deref() {
        Some("winter") => Some((3, 31)),
        Some("spring") => Some((6, 30)),
        Some("summer") => Some((9, 30)),
        Some("autumn") => Some((12, 31)),
        _ => None,
    }
}

fn parse_season_name(value: &str) -> Option<String> {
    season_regex()
        .captures(value)
        .and_then(|captures| {
            captures
                .get(1)
                .or_else(|| captures.get(2))
                .or_else(|| captures.get(3))
        })
        .map(|season| season.as_str().to_string())
}

fn normalize_season_name(season: &str) -> Option<String> {
    // Accepts Chinese (春季), Japanese short forms (春), and English. Japanese
    // seasons are written without the 季 suffix; Chinese keeps it.
    match season.trim().to_ascii_lowercase().as_str() {
        "冬季" | "冬" | "winter" => Some("winter".to_string()),
        "春季" | "春" | "spring" => Some("spring".to_string()),
        "夏季" | "夏" | "summer" => Some("summer".to_string()),
        "秋季" | "秋" | "autumn" | "fall" => Some("autumn".to_string()),
        _ => None,
    }
}

fn iso_date_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^(\d{4})-(\d{2})-(\d{2})").unwrap())
}

fn exact_date_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"\b((?:19|20)\d{2})[-/.](\d{1,2})[-/.](\d{1,2})\b|((?:19|20)\d{2})年(\d{1,2})月(\d{1,2})日").unwrap()
    })
}

fn year_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b(19|20)\d{2}\b").unwrap())
}

fn year_zh_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(19|20)\d{2}年").unwrap())
}

fn year_month_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b(19|20)\d{2}[-/.](\d{1,2})").unwrap())
}

fn season_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        // Longer Chinese forms (春季) are listed before the Japanese short forms
        // (春) so the alternation prefers them when the 季 suffix is present.
        Regex::new(r"(?i)年(春季|夏季|秋季|冬季|春|夏|秋|冬)|\b(?:19|20)\d{2}\s*(Spring|Summer|Autumn|Fall|Winter)\b|\b(Spring|Summer|Autumn|Fall|Winter)\s+(?:19|20)\d{2}\b").unwrap()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_chinese_and_english_season_dates() {
        assert_eq!(
            parse_entity_date(Some("2025年春季")).unwrap().season,
            Some("春季".to_string())
        );
        assert_eq!(
            parse_entity_date(Some("Spring 2025")).unwrap().season,
            Some("Spring".to_string())
        );
        assert_eq!(
            parse_entity_date(Some("2025 Fall")).unwrap().season,
            Some("Fall".to_string())
        );
    }

    #[test]
    fn parses_japanese_short_form_seasons() {
        // Japanese seasons omit the 季 suffix (春 vs 春季).
        assert_eq!(
            parse_entity_date(Some("2025年春")).unwrap().season_key,
            Some("spring".to_string())
        );
        assert_eq!(
            date_sort_key(Some("2025年夏")),
            Some("2025-09-30".to_string())
        );
        // Chinese long form still resolves correctly alongside the short forms.
        assert_eq!(
            parse_entity_date(Some("2025年春季")).unwrap().season_key,
            Some("spring".to_string())
        );
    }

    #[test]
    fn season_sort_keys_support_english_terms() {
        assert_eq!(
            date_sort_key(Some("Spring 2025")),
            Some("2025-06-30".to_string())
        );
        assert_eq!(
            date_sort_key(Some("2025 Winter")),
            Some("2025-03-31".to_string())
        );
        assert!(season_compare_value("Autumn") > season_compare_value("Summer"));
    }

    #[test]
    fn iso_date_truncates_datetimes_and_rejects_non_dates() {
        assert_eq!(iso_date("2024-01-15"), Some("2024-01-15".to_string()));
        assert_eq!(
            iso_date("2023-04-16T07:00:00Z"),
            Some("2023-04-16".to_string())
        );
        assert_eq!(
            iso_date("2009-04-09T00:00:00+00:00"),
            Some("2009-04-09".to_string())
        );
        assert_eq!(iso_date(""), None);
        assert_eq!(iso_date("2024-13-40"), None); // invalid month/day
        assert_eq!(iso_date("sometime"), None);
    }

    #[test]
    fn parsed_date_sort_keys_use_planning_start_dates() {
        assert_eq!(
            parsed_date_sort_key(Some("Spring 2025")),
            Some("2025-04-01".to_string())
        );
        assert_eq!(
            parsed_date_sort_key(Some("2025/4/5")),
            Some("2025-04-05".to_string())
        );
        assert_eq!(
            parsed_date_sort_key(Some("2025")),
            Some("2025-01-01".to_string())
        );
    }
}
