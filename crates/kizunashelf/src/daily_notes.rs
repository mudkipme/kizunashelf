use crate::dates::is_in_month;
use crate::types::KizunaConfig;
use crate::vfs::{self, Vfs};
use anyhow::Result;
use std::collections::HashMap;

#[derive(Clone)]
pub struct DailyNoteFile {
    /// Vault-relative path (forward-slash), e.g. `Daily Notes/2026-06-16.md`.
    pub relative_path: String,
    pub date: Option<String>,
    pub source_label: String,
    /// The file's contents, read in a single batched pass so callers don't make
    /// a per-file read round trip.
    pub contents: String,
}

/// A daily-note file that matched the date filter but whose contents have not
/// been read yet. Discovery (cheap directory walks) is separated from reading so
/// callers that scan the whole vault can read contents in bounded chunks rather
/// than loading every note into memory at once.
pub(crate) struct PendingDailyNote {
    pub(crate) relative_path: String,
    pub(crate) date: Option<String>,
    pub(crate) source_label: String,
}

pub async fn daily_note_files(
    config: &KizunaConfig,
    vfs: &dyn Vfs,
    year: Option<i32>,
    month: Option<u32>,
    require_date: bool,
) -> Result<Vec<DailyNoteFile>> {
    // 1. Discover matching files (no contents read yet).
    let pending = daily_note_candidates(config, vfs, year, month, require_date).await?;

    // 2. Read the surviving files' contents in one batched call.
    let paths: Vec<String> = pending
        .iter()
        .map(|note| note.relative_path.clone())
        .collect();
    let mut contents_by_path: HashMap<String, String> = read_daily_note_contents(vfs, &paths)
        .await?
        .into_iter()
        .collect();

    // 3. Assemble, dropping any file that could not be read or decoded.
    let files = pending
        .into_iter()
        .filter_map(|note| {
            contents_by_path
                .remove(&note.relative_path)
                .map(|contents| DailyNoteFile {
                    relative_path: note.relative_path,
                    date: note.date,
                    source_label: note.source_label,
                    contents,
                })
        })
        .collect();

    Ok(files)
}

/// Walks the configured daily-note directories and returns the files whose
/// path/filename matches the date filter — without reading any file contents.
/// One `read_dir` per directory; the surviving paths can then be read in chunks
/// via [`read_daily_note_contents`].
pub(crate) async fn daily_note_candidates(
    config: &KizunaConfig,
    vfs: &dyn Vfs,
    year: Option<i32>,
    month: Option<u32>,
    require_date: bool,
) -> Result<Vec<PendingDailyNote>> {
    let format = daily_note_date_format(config);

    let mut pending = Vec::new();
    for path in daily_note_paths(config) {
        for relative_path in vfs::walk_markdown_files(vfs, &path).await? {
            let basename = relative_path
                .rsplit('/')
                .next()
                .unwrap_or(&relative_path)
                .to_string();
            // Match against the path relative to the daily-notes folder first so
            // formats that include subfolders (e.g. `YYYY/MM-DD`) resolve, then
            // fall back to the bare basename.
            let relative_to_folder = relative_path
                .strip_prefix(&format!("{path}/"))
                .unwrap_or(&relative_path);
            let date = daily_note_date(relative_to_folder, &format)
                .or_else(|| daily_note_date(&basename, &format));

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
            pending.push(PendingDailyNote {
                relative_path,
                date,
                source_label,
            });
        }
    }

    Ok(pending)
}

/// Largest number of daily notes whose contents are held in memory at once when
/// scanning the whole vault (e.g. building daily-note relations). Bounds peak
/// memory so a vault with very many (or very large) daily notes cannot OOM the
/// relation pass.
pub(crate) const DAILY_NOTE_READ_CHUNK: usize = 64;

/// Reads a batch of daily-note files, returning `(relative_path, contents)` for
/// each that could be read and UTF-8 decoded. Files that fail either are dropped
/// (consistent with [`daily_note_files`]).
pub(crate) async fn read_daily_note_contents(
    vfs: &dyn Vfs,
    paths: &[String],
) -> Result<Vec<(String, String)>> {
    Ok(vfs
        .read_files(paths)
        .await?
        .into_iter()
        .filter_map(|(path, bytes)| String::from_utf8(bytes).ok().map(|text| (path, text)))
        .collect())
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
    use unicode_normalization::UnicodeNormalization;
    // Canonicalize to NFC so a target and an entity basename compare equal
    // regardless of Unicode composition. Filenames and file content can disagree:
    // Apple filesystems hand back directory entries in decomposed (NFD) form
    // (e.g. `ず` as `す` + U+3099 combining dakuten), while authors typically write
    // composed (NFC) content — and a vault may even mix the two. This function is
    // the single chokepoint applied to both the lookup key and the index, so
    // normalizing here makes relation/wikilink matching composition-insensitive.
    target
        .split('/')
        .next_back()
        .unwrap_or(target)
        .trim()
        .to_lowercase()
        .nfc()
        .collect()
}

fn daily_note_paths(config: &KizunaConfig) -> Vec<String> {
    config
        .daily_notes
        .as_ref()
        .filter(|daily_notes| !daily_notes.paths.is_empty())
        .map(|daily_notes| daily_notes.paths.clone())
        .unwrap_or_else(|| vec!["Daily Notes".to_string()])
}

/// Moment.js-style default, matching Obsidian's out-of-the-box Daily Notes
/// format and filenames like `2026-06-16.md`.
const DEFAULT_DATE_FORMAT: &str = "YYYY-MM-DD";

/// Resolve the configured Moment.js-style date format and translate it to a
/// chrono `strftime` format ready for parsing.
fn daily_note_date_format(config: &KizunaConfig) -> String {
    let moment = config
        .daily_notes
        .as_ref()
        .and_then(|daily_notes| daily_notes.date_format.as_deref())
        .map(str::trim)
        .filter(|format| !format.is_empty())
        .unwrap_or(DEFAULT_DATE_FORMAT);
    moment_format_to_chrono(moment)
}

/// Parse a daily-note filename candidate (with the `.md` extension stripped)
/// against the chrono format, returning a normalized `YYYY-MM-DD` string. The
/// whole candidate must match, which also rejects impossible dates (e.g.
/// `2025-02-30`).
fn daily_note_date(candidate: &str, chrono_format: &str) -> Option<String> {
    let candidate = candidate.strip_suffix(".md").unwrap_or(candidate);
    let date = chrono::NaiveDate::parse_from_str(candidate, chrono_format).ok()?;
    Some(date.format("%Y-%m-%d").to_string())
}

/// Convert a Moment.js-style date format (as used by Obsidian Daily Notes) into
/// a chrono `strftime` format. Only the tokens meaningful for a daily-note
/// filename are translated; bracketed text `[literal]` is emitted verbatim and
/// any other character passes through as a literal (with `%` escaped so it is
/// not mistaken for a chrono specifier).
fn moment_format_to_chrono(format: &str) -> String {
    // Longest tokens first so e.g. `YYYY` is matched before `YY`.
    const TOKENS: &[(&str, &str)] = &[
        ("YYYY", "%Y"),
        ("YY", "%y"),
        ("MMMM", "%B"),
        ("MMM", "%b"),
        ("MM", "%m"),
        ("M", "%m"),
        ("DD", "%d"),
        ("D", "%d"),
        ("dddd", "%A"),
        ("ddd", "%a"),
        ("HH", "%H"),
        ("H", "%H"),
        ("hh", "%I"),
        ("h", "%I"),
        ("mm", "%M"),
        ("ss", "%S"),
        ("A", "%p"),
    ];

    let mut out = String::with_capacity(format.len() + 8);
    let mut rest = format;
    while !rest.is_empty() {
        if let Some(inner) = rest.strip_prefix('[') {
            if let Some(end) = inner.find(']') {
                push_literal(&mut out, &inner[..end]);
                rest = &inner[end + 1..];
                continue;
            }
        }
        if let Some((token, repl)) = TOKENS.iter().find(|(token, _)| rest.starts_with(token)) {
            out.push_str(repl);
            rest = &rest[token.len()..];
            continue;
        }
        let ch = rest.chars().next().unwrap();
        push_literal_char(&mut out, ch);
        rest = &rest[ch.len_utf8()..];
    }
    out
}

fn push_literal(out: &mut String, literal: &str) {
    for ch in literal.chars() {
        push_literal_char(out, ch);
    }
}

fn push_literal_char(out: &mut String, ch: char) {
    if ch == '%' {
        out.push_str("%%");
    } else {
        out.push(ch);
    }
}

#[cfg(test)]
mod tests {
    use super::{daily_note_date, moment_format_to_chrono, normalize_wikilink_target};

    #[test]
    fn moment_tokens_map_to_chrono() {
        assert_eq!(moment_format_to_chrono("YYYY-MM-DD"), "%Y-%m-%d");
        assert_eq!(moment_format_to_chrono("YYYY/MM/DD"), "%Y/%m/%d");
        assert_eq!(moment_format_to_chrono("DD-MM-YYYY"), "%d-%m-%Y");
        assert_eq!(moment_format_to_chrono("YYYY-MMM-DD"), "%Y-%b-%d");
        // Bracketed text and unknown characters stay literal.
        assert_eq!(moment_format_to_chrono("[Daily] YYYY"), "Daily %Y");
        assert_eq!(moment_format_to_chrono("YYYY年MM月DD日"), "%Y年%m月%d日");
    }

    #[test]
    fn default_format_extracts_date_from_filename() {
        let format = moment_format_to_chrono("YYYY-MM-DD");
        assert_eq!(
            daily_note_date("2025-04-21.md", &format),
            Some("2025-04-21".to_string())
        );
        // Impossible dates are rejected.
        assert_eq!(daily_note_date("2025-02-30.md", &format), None);
        // A non-matching name yields nothing.
        assert_eq!(daily_note_date("notes.md", &format), None);
    }

    #[test]
    fn custom_formats_extract_and_normalize() {
        let slashed = moment_format_to_chrono("YYYY/MM-DD");
        assert_eq!(
            daily_note_date("2026/06-16", &slashed),
            Some("2026-06-16".to_string())
        );
        // Single-digit M/D tokens accept unpadded values and normalize output.
        let unpadded = moment_format_to_chrono("YYYY-M-D");
        assert_eq!(
            daily_note_date("2026-6-9.md", &unpadded),
            Some("2026-06-09".to_string())
        );
    }

    #[test]
    fn normalize_matches_across_nfc_and_nfd() {
        // `田所あずさ`: the `ず` is composed (NFC, U+305A) on one side and
        // decomposed (NFD, `す` U+3059 + U+3099 combining dakuten) on the other —
        // the shape Apple's filesystem produces for directory-entry names.
        let nfc = "田所あ\u{305A}さ";
        let nfd = "田所あ\u{3059}\u{3099}さ";
        assert_ne!(nfc, nfd, "inputs must differ byte-wise to be a real test");
        assert_eq!(
            normalize_wikilink_target(nfc),
            normalize_wikilink_target(nfd),
            "NFC and NFD forms of the same name must normalize equal"
        );
    }
}
