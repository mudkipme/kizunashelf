use crate::dates::{is_in_month, parse_exact_date};
use crate::types::KizunaConfig;
use anyhow::Result;
use regex::Regex;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use tokio::fs;

#[derive(Clone)]
pub struct DailyNoteFile {
    pub absolute_path: PathBuf,
    pub relative_path: String,
    pub date: Option<String>,
    pub source_label: String,
}

pub async fn daily_note_files(
    config: &KizunaConfig,
    year: Option<i32>,
    month: Option<u32>,
    require_date: bool,
) -> Result<Vec<DailyNoteFile>> {
    let pattern = daily_note_date_pattern(config);
    let mut files = Vec::new();

    for path in daily_note_paths(config) {
        for absolute_path in walk_markdown_files(&Path::new(&config.vault_root).join(path)).await? {
            let relative_path = relative_path(Path::new(&config.vault_root), &absolute_path);
            let basename = absolute_path
                .file_name()
                .map(|item| item.to_string_lossy().to_string())
                .unwrap_or_default();
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
                absolute_path,
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
        .last()
        .unwrap_or(target)
        .trim()
        .to_lowercase()
}

fn relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
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

async fn walk_markdown_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(path) = stack.pop() {
        let mut entries = match fs::read_dir(&path).await {
            Ok(entries) => entries,
            Err(err) if err.kind() == ErrorKind::NotFound => continue,
            Err(err) => return Err(err.into()),
        };
        while let Some(entry) = entries.next_entry().await? {
            let file_type = entry.file_type().await?;
            let path = entry.path();
            if file_type.is_dir() {
                stack.push(path);
            } else if file_type.is_file()
                && path.extension().is_some_and(|extension| extension == "md")
            {
                files.push(path);
            }
        }
    }
    Ok(files)
}
