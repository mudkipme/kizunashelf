//! Obsidian-style **task list items** (`- [ ]` / `- [x]`) written anywhere in an
//! entity's Markdown body.
//!
//! The episodes/tracks list has its own structured model ([`crate::episodes`]);
//! this module covers every *other* checkbox a user typed into their notes — a
//! plain GFM task list, with no schema behind it. Toggling one rewrites exactly
//! that line and nothing else, stamping (or clearing) the Obsidian Tasks
//! `✅ YYYY-MM-DD` completion suffix so the file stays readable in Obsidian.
//!
//! It also owns the `📅`/`✅` date-suffix primitives the episodes list shares,
//! so both features agree on that syntax byte-for-byte.

use crate::markdown::{find_section, FenceState};
use crate::types::BodySection;
use regex::Regex;
use std::sync::OnceLock;

/// The Obsidian Tasks due (air/release) date emoji.
pub(crate) const DUE_EMOJI: &str = "📅";
/// The Obsidian Tasks completion date emoji.
pub(crate) const DONE_EMOJI: &str = "✅";

/// The Obsidian Tasks date suffixes we round-trip. We emit them at the end of a
/// line, but accept them anywhere in it.
pub(crate) fn due_date_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"📅\s*(\d{4}-\d{2}-\d{2})").unwrap())
}

pub(crate) fn done_date_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"✅\s*(\d{4}-\d{2}-\d{2})").unwrap())
}

/// Strips one emoji date suffix out of a line's content, returning
/// `(content_without_the_suffix, date)` so the date survives the round-trip as
/// structured data instead of leaking into the title.
pub(crate) fn strip_emoji_date(content: &str, regex: &Regex) -> (String, Option<String>) {
    let Some(captures) = regex.captures(content) else {
        return (content.to_string(), None);
    };
    let date = captures.get(1).map(|m| m.as_str().to_string());
    let whole = captures.get(0).unwrap();
    let mut without = String::with_capacity(content.len());
    without.push_str(content[..whole.start()].trim_end());
    let tail = content[whole.end()..].trim_start();
    if !tail.is_empty() {
        if !without.is_empty() {
            without.push(' ');
        }
        without.push_str(tail);
    }
    (without, date)
}

pub(crate) fn emoji_suffix(emoji: &str, date: Option<&str>) -> String {
    match date.map(str::trim) {
        Some(date) if !date.is_empty() => format!(" {emoji} {date}"),
        _ => String::new(),
    }
}

/// A GFM task-list item line: indent, list marker, `[ ]`/`[x]` checkbox, content.
/// Only ` `, `x` and `X` count as checkbox states — the same three remark-gfm
/// recognizes on the client, so what the UI draws as a checkbox is exactly what
/// this scanner can toggle.
fn task_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^([ \t]*)([-*+]|\d{1,9}[.)])([ \t]+)\[([ xX])\](?:[ \t]+(.*))?[ \t]*$")
            .unwrap()
    })
}

/// One task-list item line located in a body: its 1-based line number and the
/// byte range of the line itself (the newline excluded).
struct TaskLine {
    line: usize,
    start: usize,
    end: usize,
}

/// Every task-list item line in `text`, in document order, skipping fenced code
/// (a `- [ ]` inside a ``` block is a code sample, not a task).
fn task_lines(text: &str) -> Vec<TaskLine> {
    let mut fence = FenceState::default();
    let mut tasks = Vec::new();
    let mut offset = 0usize;
    for (index, line) in text.split('\n').enumerate() {
        let start = offset;
        offset += line.len() + 1;
        let content = line.strip_suffix('\r').unwrap_or(line);
        if fence.observe(content) || fence.in_fence() {
            continue;
        }
        if task_regex().is_match(content) {
            tasks.push(TaskLine {
                line: index + 1,
                start,
                end: start + content.len(),
            });
        }
    }
    tasks
}

/// Checks/unchecks the task item on 1-based `line` of the entity's **notes body**
/// — [`crate::episodes::notes_body`], the coordinate space both clients render —
/// stamping `today` as its `✅` completion date when checked and clearing it when
/// unchecked.
///
/// The line is resolved by position within the notes view, then mapped onto
/// `body` by ordinal: items inside the episodes section are excluded from both
/// scans, since that list has its own UI and its own write path. `text` is the
/// task line as the client saw it and is verified against the located line, so a
/// locator that has drifted fails loudly instead of toggling a *different* item.
///
/// `body` is the **raw document body** — the write path's
/// [`crate::library::split_markdown_document`], which keeps the newline that
/// follows the closing `---`. The client's line, though, was counted against
/// [`crate::types::Entity::body`], which the read path trims. Resolving the line
/// in the untrimmed body would shift every number by that leading whitespace, so
/// the notes view is rebuilt from the trimmed body to match what the client saw;
/// only the ordinal then crosses back into the raw body, where the write lands.
///
/// Returns the rewritten body, or `None` when the line isn't a task item, the two
/// scans don't line up, or `text` doesn't match what's there.
pub fn set_task_done(
    body: &str,
    section: Option<&BodySection>,
    line: usize,
    text: &str,
    done: bool,
    today: &str,
) -> Option<String> {
    let notes = crate::episodes::notes_body(body.trim(), section);
    let index = task_lines(&notes)
        .iter()
        .position(|task| task.line == line)?;
    let skipped = section.and_then(|section| find_section(body, &section.heading));
    let target = task_lines(body)
        .into_iter()
        .filter(|task| {
            skipped
                .as_ref()
                .is_none_or(|range| task.start < range.start || task.start >= range.end)
        })
        .nth(index)?;

    let source = &body[target.start..target.end];
    if source.trim_end() != text.trim_end() {
        return None;
    }
    let rewritten = set_line_done(source, done, today)?;
    Some(format!(
        "{}{rewritten}{}",
        &body[..target.start],
        &body[target.end..]
    ))
}

/// Rewrites one task line's checkbox and `✅` suffix, keeping its indent, list
/// marker, spacing and remaining content (including a `📅` due date) intact.
fn set_line_done(line: &str, done: bool, today: &str) -> Option<String> {
    let captures = task_regex().captures(line)?;
    let indent = captures.get(1).map_or("", |m| m.as_str());
    let marker = captures.get(2).map_or("", |m| m.as_str());
    let spacing = captures.get(3).map_or(" ", |m| m.as_str());
    let content = captures.get(5).map_or("", |m| m.as_str());
    // Drop any existing completion date first, so re-checking an already-dated
    // item re-stamps it rather than appending a second `✅`.
    let (content, _) = strip_emoji_date(content, done_date_regex());
    let content = content.trim_end();

    let mut rewritten = format!(
        "{indent}{marker}{spacing}[{}]",
        if done { "x" } else { " " }
    );
    if !content.is_empty() {
        rewritten.push(' ');
        rewritten.push_str(content);
    }
    rewritten.push_str(&emoji_suffix(DONE_EMOJI, done.then_some(today)));
    Some(rewritten)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{BodySectionKind, EpisodeTracking};

    fn section() -> BodySection {
        BodySection {
            heading: "Episodes".to_string(),
            kind: BodySectionKind::Episodes,
            external_fields: Vec::new(),
            tracking: Some(EpisodeTracking::Checklist),
        }
    }

    /// Toggles the task on `line` of the notes view, taking `text` from that view
    /// the way a client does.
    fn toggle(
        body: &str,
        section: Option<&BodySection>,
        line: usize,
        done: bool,
    ) -> Option<String> {
        let notes = crate::episodes::notes_body(body.trim(), section);
        let text = notes.split('\n').nth(line - 1)?.to_string();
        set_task_done(body, section, line, &text, done, "2024-05-04")
    }

    #[test]
    fn checking_stamps_a_done_date_and_unchecking_clears_it() {
        let body = "## Notes\n\n- [ ] buy milk\n- [ ] call mum\n";
        let checked = toggle(body, None, 3, true).unwrap();
        assert_eq!(
            checked,
            "## Notes\n\n- [x] buy milk ✅ 2024-05-04\n- [ ] call mum\n"
        );

        // Unchecking restores the original line byte-for-byte.
        let notes = checked.clone();
        let text = notes.split('\n').nth(2).unwrap();
        let unchecked = set_task_done(&checked, None, 3, text, false, "2024-06-01").unwrap();
        assert_eq!(unchecked, body);
    }

    #[test]
    fn rechecking_replaces_the_existing_done_date() {
        let body = "- [x] ship it ✅ 2024-01-01\n";
        let restamped = toggle(body, None, 1, true).unwrap();
        assert_eq!(restamped, "- [x] ship it ✅ 2024-05-04\n");
    }

    #[test]
    fn a_due_date_stays_ahead_of_the_done_date() {
        let body = "- [ ] taxes 📅 2024-04-15\n";
        let checked = toggle(body, None, 1, true).unwrap();
        assert_eq!(checked, "- [x] taxes 📅 2024-04-15 ✅ 2024-05-04\n");
    }

    #[test]
    fn indent_and_marker_style_survive_the_rewrite() {
        let body = "1. [ ] first\n   * [ ] nested\n";
        assert_eq!(
            toggle(body, None, 2, true).unwrap(),
            "1. [ ] first\n   * [x] nested ✅ 2024-05-04\n"
        );
        assert_eq!(
            toggle(body, None, 1, true).unwrap(),
            "1. [x] first ✅ 2024-05-04\n   * [ ] nested\n"
        );
    }

    #[test]
    fn tasks_inside_code_fences_are_not_tasks() {
        let body = "```\n- [ ] sample\n```\n\n- [ ] real\n";
        // Line 5 of the notes view is the only task; the fenced one never counts,
        // so the ordinal lands on the real item rather than shifting by one.
        assert_eq!(
            toggle(body, None, 5, true).unwrap(),
            "```\n- [ ] sample\n```\n\n- [x] real ✅ 2024-05-04\n"
        );
        // Addressing the fenced line finds no task at all.
        assert!(toggle(body, None, 2, true).is_none());
    }

    #[test]
    fn notes_line_numbers_map_across_a_removed_episodes_section() {
        // The episodes list has its own UI and its own write path: its checkboxes
        // are absent from the notes view and must not be counted when the ordinal
        // is mapped back onto the full body.
        let body = "- [ ] before\n\n## Episodes\n\n- [ ] 1 · Pilot\n- [x] 2 · Dawn\n\n## Notes\n\n- [ ] after\n";
        let notes = crate::episodes::notes_body(body, Some(&section()));
        assert_eq!(notes, "- [ ] before\n\n## Notes\n\n- [ ] after");

        assert_eq!(
            toggle(body, Some(&section()), 5, true).unwrap(),
            "- [ ] before\n\n## Episodes\n\n- [ ] 1 · Pilot\n- [x] 2 · Dawn\n\n## Notes\n\n- [x] after ✅ 2024-05-04\n"
        );
        assert_eq!(
            toggle(body, Some(&section()), 1, true).unwrap(),
            "- [x] before ✅ 2024-05-04\n\n## Episodes\n\n- [ ] 1 · Pilot\n- [x] 2 · Dawn\n\n## Notes\n\n- [ ] after\n"
        );
    }

    /// The client counts lines against the *trimmed* body the detail response
    /// serves (`Entity::body`); the write path hands us the raw one, which still
    /// carries the newline after the closing `---`. Every line would be off by
    /// that much if the two weren't reconciled.
    #[test]
    fn a_raw_body_still_resolves_the_line_the_client_counted() {
        // What `split_markdown_document` yields: a leading newline (and here a
        // blank line after the frontmatter) that `parse_markdown` trims away.
        let raw = "\n\n## Notes\n\n- [ ] 第一章\n- [ ] 第二章\n";
        let served = raw.trim();
        assert_eq!(served, "## Notes\n\n- [ ] 第一章\n- [ ] 第二章");

        // The client sees 第二章 on line 4 of what it was served.
        let checked = set_task_done(raw, None, 4, "- [ ] 第二章", true, "2024-05-04").unwrap();
        assert_eq!(
            checked,
            "\n\n## Notes\n\n- [ ] 第一章\n- [x] 第二章 ✅ 2024-05-04\n"
        );
        // The leading whitespace is preserved, not silently normalized away.
        assert!(checked.starts_with("\n\n"));
    }

    #[test]
    fn a_raw_body_resolves_notes_tasks_written_above_the_episodes_section() {
        // Tasks *before* the section are where the ordinal mapping and the raw
        // body's leading newline compound: the section no longer sits at the top,
        // so `remove_section` doesn't trim that newline away on its own.
        let raw = "\n## Notes\n\n- [ ] 第一章\n\n## Episodes\n\n- [ ] 1 · Pilot\n";
        let notes = crate::episodes::notes_body(raw.trim(), Some(&section()));
        assert_eq!(notes, "## Notes\n\n- [ ] 第一章");

        let checked =
            set_task_done(raw, Some(&section()), 3, "- [ ] 第一章", true, "2024-05-04").unwrap();
        assert_eq!(
            checked,
            "\n## Notes\n\n- [x] 第一章 ✅ 2024-05-04\n\n## Episodes\n\n- [ ] 1 · Pilot\n"
        );
    }

    #[test]
    fn a_stale_locator_is_refused_rather_than_toggling_another_item() {
        let body = "- [ ] buy milk\n- [ ] call mum\n";
        // The client thinks line 2 says something else — the write must not land.
        assert!(set_task_done(body, None, 2, "- [ ] buy milk", true, "2024-05-04").is_none());
        assert!(set_task_done(body, None, 9, "- [ ] call mum", true, "2024-05-04").is_none());
    }

    #[test]
    fn a_non_task_line_is_not_toggleable() {
        let body = "- a plain bullet\n\nsome prose\n";
        assert!(toggle(body, None, 1, true).is_none());
        assert!(toggle(body, None, 3, true).is_none());
    }
}
