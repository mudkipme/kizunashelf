//! Minimal, fence-aware Markdown **heading-section** helpers shared by body-section
//! features (episodes today; external-metadata apply later). A "section" is the run
//! of body from a heading line up to the next heading of the same or higher level.
//!
//! This is the core-owned counterpart to the logic the web/iOS clients duplicate
//! (`findMarkdownHeadingSection`/`markdownHeadings`); behavior matches them so they
//! can eventually call the core instead. No Markdown crate — line + regex, like the
//! rest of the core.

use regex::Regex;
use std::sync::OnceLock;

/// One ATX heading found in a body, with byte offsets into the original string.
#[derive(Clone, Debug)]
pub struct Heading {
    /// Byte offset of the heading line's first character.
    pub start: usize,
    /// Byte offset just past the heading line (after its newline).
    pub end: usize,
    pub level: usize,
    /// The heading text, trailing `#`s stripped and trimmed.
    pub text: String,
}

/// A located heading section: the heading line plus its content up to the next
/// same-or-higher heading (or end of body).
#[derive(Clone, Debug)]
pub struct Section {
    /// Byte offset of the heading line start.
    pub start: usize,
    /// Byte offset where the content begins (just past the heading line).
    pub content_start: usize,
    /// Byte offset where the section ends (next same/higher heading, or body end).
    pub end: usize,
}

fn fence_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^ {0,3}(`{3,}|~{3,})").unwrap())
}

fn heading_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^ {0,3}(#{1,6})(?:[ \t]+|$)(.*)$").unwrap())
}

fn trailing_hashes_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"[ \t]+#+[ \t]*$").unwrap())
}

/// Every ATX heading in `body`, skipping headings inside fenced code blocks.
pub fn headings(body: &str) -> Vec<Heading> {
    let mut out = Vec::new();
    let mut offset = 0usize;
    // Open fence: (marker char, opening run length). A closing fence must use the
    // same marker and be at least as long.
    let mut fence: Option<(char, usize)> = None;
    for line in body.split_inclusive('\n') {
        let content = line.strip_suffix('\n').unwrap_or(line);
        let content = content.strip_suffix('\r').unwrap_or(content);
        if let Some(caps) = fence_regex().captures(content) {
            let run = caps.get(1).unwrap().as_str();
            let marker = run.chars().next().unwrap();
            let length = run.len();
            match fence {
                None => fence = Some((marker, length)),
                Some((open_marker, open_len)) if open_marker == marker && length >= open_len => {
                    fence = None;
                }
                _ => {}
            }
        } else if fence.is_none() {
            if let Some(caps) = heading_regex().captures(content) {
                let level = caps.get(1).unwrap().as_str().len();
                let raw = caps.get(2).map_or("", |m| m.as_str());
                let text = trailing_hashes_regex().replace(raw, "").trim().to_string();
                out.push(Heading {
                    start: offset,
                    end: offset + line.len(),
                    level,
                    text,
                });
            }
        }
        offset += line.len();
    }
    out
}

/// Normalizes heading text for case/space-insensitive matching (mirrors the
/// clients' `normalizeMarkdownHeadingText`).
pub fn normalize_heading(value: &str) -> String {
    let cleaned: String = value
        .chars()
        .map(|c| {
            if matches!(c, '\r' | '\n' | '#') {
                ' '
            } else {
                c
            }
        })
        .collect();
    cleaned
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// Sanitizes a heading for rendering: strips newlines/`#`, collapses whitespace.
fn sanitize_heading(value: &str) -> String {
    let cleaned: String = value
        .chars()
        .map(|c| {
            if matches!(c, '\r' | '\n' | '#') {
                ' '
            } else {
                c
            }
        })
        .collect();
    let collapsed = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        "Section".to_string()
    } else {
        collapsed
    }
}

/// Locates the section under the heading matching `heading` (case/space-insensitive),
/// or `None` if no such heading exists.
pub fn find_section(body: &str, heading: &str) -> Option<Section> {
    let target = normalize_heading(heading);
    if target.is_empty() {
        return None;
    }
    let headings = headings(body);
    let index = headings
        .iter()
        .position(|item| normalize_heading(&item.text) == target)?;
    let start = &headings[index];
    let end = headings[index + 1..]
        .iter()
        .find(|item| item.level <= start.level)
        .map(|item| item.start)
        .unwrap_or(body.len());
    Some(Section {
        start: start.start,
        content_start: start.end,
        end,
    })
}

/// Replaces the content under `heading` with `content` (keeping the heading line),
/// or appends a new `## heading` section at the end when the heading is absent.
/// Every other section of the body is preserved byte-for-byte.
pub fn splice_section(body: &str, heading: &str, content: &str) -> String {
    let content = content.trim();
    match find_section(body, heading) {
        Some(section) => {
            let before = &body[..section.start];
            let after = body[section.end..].trim_start_matches('\n');
            let heading_line = body[section.start..section.content_start].trim_end();
            let separator = if after.is_empty() { "\n" } else { "\n\n" };
            if content.is_empty() {
                format!("{before}{heading_line}\n{separator}{after}")
            } else {
                format!("{before}{heading_line}\n\n{content}{separator}{after}")
            }
        }
        None => {
            let trimmed = body.trim_end();
            let lead = if trimmed.is_empty() { "" } else { "\n\n" };
            format!(
                "{trimmed}{lead}## {}\n\n{content}\n",
                sanitize_heading(heading)
            )
        }
    }
}

/// Returns `body` with the entire section under `heading` removed — the heading
/// line *and* its content (down to the next same/higher heading) — collapsing the
/// surrounding blank lines so no double gap is left. Unchanged when the heading is
/// absent. Used to drop a section that has a dedicated UI (e.g. episodes) from the
/// generic body render without re-parsing Markdown in each client.
pub fn remove_section(body: &str, heading: &str) -> String {
    let Some(section) = find_section(body, heading) else {
        return body.to_string();
    };
    let before = body[..section.start].trim_end();
    let after = body[section.end..].trim_start_matches('\n').trim_end();
    match (before.is_empty(), after.is_empty()) {
        (true, _) => after.to_string(),
        (false, true) => before.to_string(),
        (false, false) => format!("{before}\n\n{after}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_section_and_respects_levels() {
        let body = "Intro\n\n## Episodes\n- [ ] 1\n\n## Notes\nhi\n";
        let section = find_section(body, "episodes").unwrap();
        assert_eq!(&body[section.content_start..section.end], "- [ ] 1\n\n");
    }

    #[test]
    fn ignores_headings_inside_code_fences() {
        let body = "## Real\n```\n## Fake\n```\ntext\n";
        let found: Vec<_> = headings(body).into_iter().map(|h| h.text).collect();
        assert_eq!(found, vec!["Real".to_string()]);
    }

    #[test]
    fn splice_replaces_only_the_target_section() {
        let body = "## Summary\nold\n\n## Episodes\n- [ ] 1\n\n## Notes\nkeep\n";
        let next = splice_section(body, "Episodes", "- [x] 1\n- [ ] 2");
        assert!(next.contains("## Summary\nold"));
        assert!(next.contains("## Episodes\n\n- [x] 1\n- [ ] 2"));
        assert!(next.contains("## Notes\nkeep"));
    }

    #[test]
    fn splice_appends_when_heading_absent() {
        let next = splice_section("Just a body.", "Episodes", "- [ ] 1");
        assert_eq!(next, "Just a body.\n\n## Episodes\n\n- [ ] 1\n");
    }

    #[test]
    fn remove_section_drops_heading_and_content_between_neighbours() {
        let body = "## Summary\nkeep\n\n## Episodes\n- [ ] 1\n- [ ] 2\n\n## Notes\nkeep too\n";
        let next = remove_section(body, "Episodes");
        assert_eq!(next, "## Summary\nkeep\n\n## Notes\nkeep too");
        assert!(!next.contains("Episodes"));
    }

    #[test]
    fn remove_section_handles_leading_and_absent() {
        // Section at the very start.
        assert_eq!(
            remove_section("## Episodes\n- 1\n\n## Notes\nx\n", "Episodes"),
            "## Notes\nx"
        );
        // Absent heading → unchanged.
        assert_eq!(remove_section("## Notes\nx\n", "Episodes"), "## Notes\nx\n");
    }
}
