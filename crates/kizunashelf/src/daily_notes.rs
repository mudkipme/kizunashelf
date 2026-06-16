use crate::dates::{is_in_month, parse_exact_date};
use crate::types::KizunaConfig;
use crate::vfs::{self, Vfs};
use anyhow::Result;
use regex::Regex;

#[derive(Clone)]
pub struct DailyNoteFile {
    /// Vault-relative path (forward-slash), e.g. `Daily Notes/2026-06-16.md`.
    pub relative_path: String,
    pub date: Option<String>,
    pub source_label: String,
}

pub async fn daily_note_files(
    config: &KizunaConfig,
    vfs: &dyn Vfs,
    year: Option<i32>,
    month: Option<u32>,
    require_date: bool,
) -> Result<Vec<DailyNoteFile>> {
    let pattern = daily_note_date_pattern(config);
    let mut files = Vec::new();

    for path in daily_note_paths(config) {
        for relative_path in vfs::walk_markdown_files(vfs, &path).await? {
            let basename = relative_path
                .rsplit('/')
                .next()
                .unwrap_or(&relative_path)
                .to_string();
            let date = daily_note_date(&relative_path, &pattern)
                .or_else(|| daily_note_date(&basename, &pattern));

            if require_date && date.is_none() {
                continue;
            }
            if year.zip(month).is_some_and(|(year, month)| {
                !date
                    .as_deref()
                    .is_some_and(|date| is_in_month(date, year, month))
            }) {
                continue;
            }

            let source_label = date.clone().unwrap_or_else(|| {
                basename
                    .strip_suffix(".md")
                    .unwrap_or(&basename)
                    .to_string()
            });
            files.push(DailyNoteFile {
                relative_path,
                date,
                source_label,
            });
        }
    }

    Ok(files)
}

pub fn strip_frontmatter(raw: &str) -> String {
    if !raw.starts_with("---\n") {
        return raw.to_string();
    }
    raw[4..]
        .find("\n---")
        .map(|end| raw[end + 8..].to_string())
        .unwrap_or_else(|| raw.to_string())
}

pub fn normalize_wikilink_target(target: &str) -> String {
    target
        .split('/')
        .next_back()
        .unwrap_or(target)
        .trim()
        .to_lowercase()
}

fn daily_note_paths(config: &KizunaConfig) -> Vec<String> {
    config
        .daily_notes
        .as_ref()
        .filter(|daily_notes| !daily_notes.paths.is_empty())
        .map(|daily_notes| daily_notes.paths.clone())
        .unwrap_or_else(|| vec!["Daily Notes".to_string()])
}

fn daily_note_date_pattern(config: &KizunaConfig) -> Regex {
    config
        .daily_notes
        .as_ref()
        .and_then(|daily_notes| daily_notes.date_pattern.as_ref())
        .and_then(|pattern| Regex::new(pattern).ok())
        .unwrap_or_else(|| Regex::new(r"^(?<date>\d{4}-\d{2}-\d{2})\.md$").unwrap())
}

fn daily_note_date(path: &str, pattern: &Regex) -> Option<String> {
    let captures = pattern.captures(path)?;
    let date = captures
        .name("date")
        .or_else(|| captures.get(1))
        .map(|capture| capture.as_str())?;
    parse_exact_date(Some(date))
}
