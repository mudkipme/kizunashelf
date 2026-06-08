use regex::Regex;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ParsedEntityDate {
    pub year: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub month: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub day: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub season: Option<String>,
}

pub fn parse_entity_date(value: Option<&str>) -> Option<ParsedEntityDate> {
    let value = value?;
    if let Some((year, month, day)) = exact_date_parts(Some(value)) {
        return Some(ParsedEntityDate {
            year,
            month: Some(month),
            day: Some(day),
            season: Some(season_for_month(month).to_string()),
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

    Some(ParsedEntityDate {
        year,
        month,
        day: None,
        season,
    })
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
    match season.trim().to_ascii_lowercase().as_str() {
        "冬季" | "winter" => Some("winter".to_string()),
        "春季" | "spring" => Some("spring".to_string()),
        "夏季" | "summer" => Some("summer".to_string()),
        "秋季" | "autumn" | "fall" => Some("autumn".to_string()),
        _ => None,
    }
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
        Regex::new(r"(?i)年(春季|夏季|秋季|冬季)|\b(?:19|20)\d{2}\s*(Spring|Summer|Autumn|Fall|Winter)\b|\b(Spring|Summer|Autumn|Fall|Winter)\s+(?:19|20)\d{2}\b").unwrap()
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
}
