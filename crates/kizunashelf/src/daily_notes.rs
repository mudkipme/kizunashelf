use crate::dates::is_in_month;
use crate::types::KizunaConfig;
use crate::vfs::{self, Vfs, VfsError};
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
    /// Directory-listing fingerprint (size, mtime) for the index cache, so an
    /// unchanged daily note's wikilinks can be reused without reading its body.
    pub(crate) len: u64,
    pub(crate) modified_unix_nanos: u128,
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
        for (relative_path, len, modified_unix_nanos) in
            vfs::walk_markdown_files_with_meta(vfs, &path).await?
        {
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
                len,
                modified_unix_nanos,
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
    crate::markdown::split_frontmatter(raw).body.to_string()
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

// --- Daily-note log writing ------------------------------------------------

/// The outcome of appending a log line to a daily note (or a dry run of it).
pub(crate) struct LogWriteOutcome {
    pub relative_path: String,
    pub note_created: bool,
    pub line_already_present: bool,
}

/// Why a log write could not proceed; the API layer maps these to HTTP status.
#[derive(Debug)]
pub(crate) enum LogWriteError {
    /// `date` isn't a valid `YYYY-MM-DD` or can't be formatted into a path.
    InvalidDate,
    /// The note changed on disk between our read and write (concurrent edit).
    Conflict,
    Vfs(anyhow::Error),
}

impl From<anyhow::Error> for LogWriteError {
    fn from(error: anyhow::Error) -> Self {
        LogWriteError::Vfs(error)
    }
}

/// Renders a log line: substitutes the tokens, then collapses whitespace runs to
/// single spaces and trims — so an empty `{note}` slot leaves no gap
/// (`- [[PRAGMATA]]  #Game` → `- [[PRAGMATA]] #Game`).
///
/// `{title}` always becomes a `[[wikilink]]` — that link is what ties the logged
/// line back to the entity (the daily-note relation parser keys off it), so it
/// must be present and is never the caller's responsibility to bracket. A config
/// that writes `[[{title}]]` by hand is handled first so the brackets aren't
/// doubled. There's no episode/progress token: logging is independent of the
/// episode list, so a caller who wants an episode number types it into `{note}`.
pub(crate) fn render_log_line(line_format: &str, title: &str, note: &str, date: &str) -> String {
    let wikilink = format!("[[{title}]]");
    let raw = line_format
        .replace("[[{title}]]", &wikilink)
        .replace("{title}", &wikilink)
        .replace("{note}", note)
        .replace("{date}", date);
    raw.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Appends `line` to the end of `section`'s block (creating an h2 `section` when
/// absent), or `None` when the **exact** line is already in the section — the
/// caller skips the write (idempotent). Whole-line match, so distinct lines for
/// several episodes the same day each get written.
fn append_log_line(body: &str, section: &str, line: &str) -> Option<String> {
    match crate::markdown::find_section(body, section) {
        Some(found) => {
            let existing = &body[found.content_start..found.end];
            if existing
                .lines()
                .any(|existing_line| existing_line.trim_end() == line.trim_end())
            {
                return None;
            }
            let trimmed = existing.trim_end();
            let content = if trimmed.is_empty() {
                line.to_string()
            } else {
                format!("{trimmed}\n{line}")
            };
            Some(crate::markdown::splice_section(body, section, &content))
        }
        None => Some(crate::markdown::splice_section(body, section, line)),
    }
}

/// The vault-relative path of an existing daily note for `date` (matched against
/// whatever filename format is on disk), else a freshly generated path. The bool
/// is whether the note already exists.
async fn resolve_log_note(
    config: &KizunaConfig,
    vfs: &dyn Vfs,
    date: &str,
) -> Result<Option<(String, bool)>> {
    for note in daily_note_candidates(config, vfs, None, None, true).await? {
        if note.date.as_deref() == Some(date) {
            return Ok(Some((note.relative_path, true)));
        }
    }
    Ok(generate_daily_note_path(config, date).map(|path| (path, false)))
}

/// Builds the vault-relative path for a new daily note dated `date`, from the
/// configured date format in the first daily-notes folder. `None` when `date`
/// isn't a valid `YYYY-MM-DD` or the format can't render it.
fn generate_daily_note_path(config: &KizunaConfig, date: &str) -> Option<String> {
    use std::fmt::Write as _;
    let parsed = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?;
    let chrono_format = daily_note_date_format(config);
    let mut relative = String::new();
    write!(relative, "{}", parsed.format(&chrono_format)).ok()?;
    let folder = daily_note_paths(config)
        .into_iter()
        .next()
        .unwrap_or_else(|| "Daily Notes".to_string());
    Some(format!("{folder}/{relative}.md"))
}

/// Starter content for a brand-new daily note: the configured template with
/// `{{date}}` / `{{title}}` substituted, or empty when no template is set or it
/// can't be read.
async fn new_note_content(config: &KizunaConfig, vfs: &dyn Vfs, date: &str) -> String {
    let Some(template_path) = config
        .daily_notes
        .as_ref()
        .and_then(|daily| daily.template.as_deref())
        .map(str::trim)
        .filter(|path| !path.is_empty())
    else {
        return String::new();
    };
    let Ok(bytes) = vfs.read(template_path).await else {
        return String::new();
    };
    let Ok(text) = String::from_utf8(bytes) else {
        return String::new();
    };
    text.replace("{{date}}", date).replace("{{title}}", date)
}

/// The outcome of removing a log line (or a dry run of it).
pub(crate) struct LogRemoveOutcome {
    pub relative_path: String,
    pub line_matched: bool,
}

/// Removes the **first exact** occurrence of `line` from `section` in the day's
/// note (whole-line match, never fuzzy) — the inverse of [`write_log_line`]'s
/// append. Leaves the section heading even if it becomes empty and never deletes
/// the note. No match (or no note) → `line_matched = false`, no write.
pub(crate) async fn remove_log_line(
    config: &KizunaConfig,
    vfs: &dyn Vfs,
    date: &str,
    section: &str,
    line: &str,
    dry_run: bool,
) -> Result<LogRemoveOutcome, LogWriteError> {
    let (relative_path, _) = resolve_log_note(config, vfs, date)
        .await?
        .ok_or(LogWriteError::InvalidDate)?;

    let existing = read_optional_text(vfs, &relative_path).await?;
    let removed = existing
        .as_deref()
        .and_then(|content| remove_exact_line_in_section(content, section, line));
    let line_matched = removed.is_some();

    if !dry_run {
        if let Some(new_content) = removed {
            // Re-read guard, as in `write_log_line`.
            if read_optional_text(vfs, &relative_path).await? != existing {
                return Err(LogWriteError::Conflict);
            }
            vfs.write_atomic(&relative_path, new_content.as_bytes())
                .await
                .map_err(|error| anyhow::anyhow!("failed to write {relative_path}: {error}"))?;
        }
    }

    Ok(LogRemoveOutcome {
        relative_path,
        line_matched,
    })
}

/// Removes the first line in `section`'s block whose trimmed text equals `line`
/// (the rest of the note preserved byte-for-byte). `None` when the section or the
/// line is absent.
fn remove_exact_line_in_section(content: &str, section: &str, line: &str) -> Option<String> {
    let found = crate::markdown::find_section(content, section)?;
    let target = line.trim_end();
    let mut offset = found.content_start;
    for raw_line in content[found.content_start..found.end].split_inclusive('\n') {
        let text = raw_line.strip_suffix('\n').unwrap_or(raw_line);
        if text.trim_end() == target {
            let mut out = String::with_capacity(content.len() - raw_line.len());
            out.push_str(&content[..offset]);
            out.push_str(&content[offset + raw_line.len()..]);
            return Some(out);
        }
        offset += raw_line.len();
    }
    None
}

/// Reads a vault file as text, returning `None` when it doesn't exist.
async fn read_optional_text(vfs: &dyn Vfs, path: &str) -> Result<Option<String>> {
    match vfs.read(path).await {
        Ok(bytes) => Ok(Some(String::from_utf8(bytes).map_err(|error| {
            anyhow::anyhow!("{path} is not valid UTF-8: {error}")
        })?)),
        Err(VfsError::NotFound) => Ok(None),
        Err(error) => Err(anyhow::anyhow!("failed to read {path}: {error}")),
    }
}

/// Appends a rendered log `line` to the day's daily note under `section`. Reads
/// the note (or seeds it from the template when absent), appends idempotently, and
/// writes atomically — re-reading right before the write to reject a concurrent
/// edit (409). This is side-effect #1 of logging only; it never touches the entity.
pub(crate) async fn write_log_line(
    config: &KizunaConfig,
    vfs: &dyn Vfs,
    date: &str,
    section: &str,
    line: &str,
    dry_run: bool,
) -> Result<LogWriteOutcome, LogWriteError> {
    // `resolve_log_note`'s existence flag is advisory (it comes from the directory
    // walk); the authoritative read below decides whether to seed from a template.
    let (relative_path, _) = resolve_log_note(config, vfs, date)
        .await?
        .ok_or(LogWriteError::InvalidDate)?;

    let existing = read_optional_text(vfs, &relative_path).await?;
    let base = match &existing {
        Some(content) => content.clone(),
        None => new_note_content(config, vfs, date).await,
    };

    let appended = append_log_line(&base, section, line);
    let line_already_present = appended.is_none();

    if !dry_run {
        if let Some(new_content) = appended {
            // Re-read guard: a concurrent edit between read and write → 409.
            if read_optional_text(vfs, &relative_path).await? != existing {
                return Err(LogWriteError::Conflict);
            }
            if let Some((parent, _)) = relative_path.rsplit_once('/') {
                vfs.create_dir_all(parent)
                    .await
                    .map_err(|error| anyhow::anyhow!("failed to create {parent}: {error}"))?;
            }
            vfs.write_atomic(&relative_path, new_content.as_bytes())
                .await
                .map_err(|error| anyhow::anyhow!("failed to write {relative_path}: {error}"))?;
        }
    }

    Ok(LogWriteOutcome {
        relative_path,
        note_created: existing.is_none(),
        line_already_present,
    })
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

#[cfg(test)]
mod log_write_tests {
    use super::*;
    use crate::types::{DailyNoteLogDefaults, DailyNotesConfig};
    use crate::vfs::InMemoryVfs;

    fn config(template: Option<&str>) -> KizunaConfig {
        KizunaConfig {
            vault_root: String::new(),
            taxonomy_root: "Taxonomy".to_string(),
            asset_root: None,
            content_writable: None,
            home: None,
            daily_notes: Some(DailyNotesConfig {
                paths: vec!["Journal".to_string()],
                date_format: None,
                template: template.map(str::to_string),
                log: Some(DailyNoteLogDefaults {
                    section: Some("Log".to_string()),
                    line_format: None,
                }),
            }),
            tags: None,
            types: Vec::new(),
        }
    }

    async fn read(vfs: &InMemoryVfs, path: &str) -> String {
        String::from_utf8(vfs.read(path).await.unwrap()).unwrap()
    }

    #[test]
    fn render_collapses_empty_slots() {
        // An empty `{note}` collapses with the surrounding whitespace.
        assert_eq!(
            render_log_line("- [[{title}]] {note} #Game", "PRAGMATA", "", "2024-08-20"),
            "- [[PRAGMATA]] #Game"
        );
        // Episode numbers (or any progress note) ride in `{note}` now.
        assert_eq!(
            render_log_line("- [[{title}]] {note} #Anime", "Show", "12", "2024-08-20"),
            "- [[Show]] 12 #Anime"
        );
        // Bare `{title}` is wrapped into a wikilink (no `[[ ]]` needed in config)…
        assert_eq!(
            render_log_line("- {title} {note} #Anime", "Show", "12", "2024-08-20"),
            "- [[Show]] 12 #Anime"
        );
        // …and a hand-written `[[{title}]]` is not double-bracketed.
        assert_eq!(
            render_log_line("- [[{title}]] #Movie", "Inception", "", "2024-08-20"),
            "- [[Inception]] #Movie"
        );
    }

    #[tokio::test]
    async fn creates_note_from_template_then_appends_under_section() {
        let vfs = InMemoryVfs::new();
        vfs.insert_file("Templates/Daily.md", "# {{date}}\n\n## Log\n");
        let config = config(Some("Templates/Daily.md"));

        let outcome = write_log_line(
            &config,
            &vfs,
            "2024-08-20",
            "Log",
            "- [[PRAGMATA]] #Game",
            false,
        )
        .await
        .unwrap();
        assert!(outcome.note_created);
        assert!(!outcome.line_already_present);
        assert_eq!(outcome.relative_path, "Journal/2024-08-20.md");

        let written = read(&vfs, "Journal/2024-08-20.md").await;
        assert!(written.contains("# 2024-08-20")); // template `{{date}}` substituted
        assert!(written.contains("## Log"));
        assert!(written.contains("- [[PRAGMATA]] #Game"));
    }

    #[tokio::test]
    async fn appends_under_existing_section_and_is_idempotent() {
        let vfs = InMemoryVfs::new();
        vfs.insert_file("Journal/2024-08-20.md", "## Log\n- [[A]] 1 #Anime\n");
        let config = config(None);
        let line = "- [[A]] 2 #Anime";

        let first = write_log_line(&config, &vfs, "2024-08-20", "Log", line, false)
            .await
            .unwrap();
        assert!(!first.note_created);
        assert!(!first.line_already_present);
        let after_first = read(&vfs, "Journal/2024-08-20.md").await;
        assert!(after_first.contains("- [[A]] 1 #Anime"));
        assert!(after_first.contains("- [[A]] 2 #Anime"));

        // The same line again is a no-op (no duplicate).
        let second = write_log_line(&config, &vfs, "2024-08-20", "Log", line, false)
            .await
            .unwrap();
        assert!(second.line_already_present);
        let after_second = read(&vfs, "Journal/2024-08-20.md").await;
        assert_eq!(after_first, after_second);
        assert_eq!(after_second.matches("- [[A]] 2 #Anime").count(), 1);
    }

    #[tokio::test]
    async fn creates_missing_section_when_note_exists() {
        let vfs = InMemoryVfs::new();
        vfs.insert_file("Journal/2024-08-20.md", "# Heading\n\nsome notes\n");
        let config = config(None);

        write_log_line(&config, &vfs, "2024-08-20", "Log", "- [[X]] #Anime", false)
            .await
            .unwrap();
        let written = read(&vfs, "Journal/2024-08-20.md").await;
        assert!(written.contains("## Log"));
        assert!(written.contains("- [[X]] #Anime"));
        assert!(written.contains("some notes")); // existing content preserved
    }

    #[tokio::test]
    async fn dry_run_reports_without_writing() {
        let vfs = InMemoryVfs::new();
        let config = config(None);

        let outcome = write_log_line(&config, &vfs, "2024-08-20", "Log", "- [[X]] #Anime", true)
            .await
            .unwrap();
        assert!(outcome.note_created); // would be created
        assert!(!outcome.line_already_present);
        assert!(vfs.read("Journal/2024-08-20.md").await.is_err()); // nothing on disk
    }

    #[tokio::test]
    async fn remove_strips_exact_line_and_keeps_heading() {
        let vfs = InMemoryVfs::new();
        vfs.insert_file(
            "Journal/2024-08-20.md",
            "## Log\n- [[A]] 1 #Anime\n- [[A]] 2 #Anime\n",
        );
        let config = config(None);

        let outcome = remove_log_line(
            &config,
            &vfs,
            "2024-08-20",
            "Log",
            "- [[A]] 1 #Anime",
            false,
        )
        .await
        .unwrap();
        assert!(outcome.line_matched);
        let after = read(&vfs, "Journal/2024-08-20.md").await;
        assert!(!after.contains("- [[A]] 1 #Anime"));
        assert!(after.contains("- [[A]] 2 #Anime")); // only the exact line went

        // Remove the last line too → the heading survives an empty section.
        remove_log_line(
            &config,
            &vfs,
            "2024-08-20",
            "Log",
            "- [[A]] 2 #Anime",
            false,
        )
        .await
        .unwrap();
        let empty = read(&vfs, "Journal/2024-08-20.md").await;
        assert!(empty.contains("## Log"));
        assert!(!empty.contains("[[A]]"));
    }

    #[tokio::test]
    async fn remove_is_noop_when_line_absent_or_hand_edited() {
        let vfs = InMemoryVfs::new();
        vfs.insert_file(
            "Journal/2024-08-20.md",
            "## Log\n- [[A]] watched twelve #Anime\n",
        );
        let config = config(None);

        // A non-exact (hand-edited) line is never fuzzy-matched; file unchanged.
        let before = read(&vfs, "Journal/2024-08-20.md").await;
        let outcome = remove_log_line(
            &config,
            &vfs,
            "2024-08-20",
            "Log",
            "- [[A]] 12 #Anime",
            false,
        )
        .await
        .unwrap();
        assert!(!outcome.line_matched);
        assert_eq!(read(&vfs, "Journal/2024-08-20.md").await, before);

        // A missing note matches nothing.
        let outcome = remove_log_line(
            &config,
            &vfs,
            "2024-09-01",
            "Log",
            "- [[A]] 1 #Anime",
            false,
        )
        .await
        .unwrap();
        assert!(!outcome.line_matched);
    }

    #[tokio::test]
    async fn add_then_remove_round_trips() {
        let vfs = InMemoryVfs::new();
        vfs.insert_file("Journal/2024-08-20.md", "## Log\n- [[A]] 1 #Anime\n");
        let config = config(None);
        let line = "- [[A]] 2 #Anime";

        write_log_line(&config, &vfs, "2024-08-20", "Log", line, false)
            .await
            .unwrap();
        assert!(read(&vfs, "Journal/2024-08-20.md")
            .await
            .contains("- [[A]] 2 #Anime"));

        let outcome = remove_log_line(&config, &vfs, "2024-08-20", "Log", line, false)
            .await
            .unwrap();
        assert!(outcome.line_matched);
        let after = read(&vfs, "Journal/2024-08-20.md").await;
        assert!(!after.contains("- [[A]] 2 #Anime"));
        assert!(after.contains("- [[A]] 1 #Anime")); // the original survives
    }
}
