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

/// Fenced-code-block tracker for a line-by-line scan. This is the one canonical
/// fence state machine the core scans with; every module that needs to skip
/// fenced code (`headings`, `strip_fenced_code`, calendar mentions, …) drives it
/// so they all agree on what a fence is — indent-tolerant (`{0,3}` leading
/// spaces), both ``` and ~~~ markers, and length-aware closing (a closing fence
/// must reuse the opening marker and be at least as long; a different or shorter
/// run inside a block is content, per CommonMark).
#[derive(Default)]
pub struct FenceState {
    /// Open fence: `(marker char, opening run length)`, or `None` when outside a
    /// fenced block.
    open: Option<(char, usize)>,
}

impl FenceState {
    /// Whether the scan is currently inside a fenced code block (i.e. lines fed so
    /// far have opened a fence that has not yet closed).
    pub fn in_fence(&self) -> bool {
        self.open.is_some()
    }

    /// Feeds one line (newline already stripped), updating the fence state, and
    /// returns whether this line is itself a fence delimiter (an opening or a
    /// matching closing fence). Lines that merely sit inside a block, or non-fence
    /// lines, return `false`.
    pub fn observe(&mut self, content: &str) -> bool {
        let Some(caps) = fence_regex().captures(content) else {
            return false;
        };
        let run = caps.get(1).unwrap().as_str();
        let marker = run.chars().next().unwrap();
        let length = run.len();
        match self.open {
            None => {
                self.open = Some((marker, length));
                true
            }
            Some((open_marker, open_len)) if open_marker == marker && length >= open_len => {
                self.open = None;
                true
            }
            // A different/shorter fence run inside an open block is content.
            Some(_) => false,
        }
    }
}

/// Returns `body` with fenced code blocks removed — the fence delimiter lines and
/// everything between them — so inline scanners (wikilink/summary extraction) do
/// not pick up constructs that only appear inside code. Every other line is kept
/// verbatim (newlines included). Fence detection is the canonical [`FenceState`],
/// so unlike a blanket `` ``` … ``` `` regex this also handles `~~~` fences and
/// only treats a run at the line start as a delimiter.
pub fn strip_fenced_code(body: &str) -> String {
    let mut out = String::with_capacity(body.len());
    let mut fence = FenceState::default();
    for line in body.split_inclusive('\n') {
        let content = line.strip_suffix('\n').unwrap_or(line);
        let content = content.strip_suffix('\r').unwrap_or(content);
        let inside_before = fence.in_fence();
        let is_delimiter = fence.observe(content);
        // Drop the opening delimiter, the closing delimiter, and every line that
        // was already inside the block.
        if is_delimiter || inside_before {
            continue;
        }
        out.push_str(line);
    }
    out
}

/// If `content` is an ATX heading line, returns `(level, heading text)` with the
/// leading `#`s consumed and any trailing `#`s stripped. Fence-awareness is the
/// caller's responsibility (drive a [`FenceState`] and skip lines inside a block).
pub fn parse_heading(content: &str) -> Option<(usize, String)> {
    let caps = heading_regex().captures(content)?;
    let level = caps.get(1).unwrap().as_str().len();
    let raw = caps.get(2).map_or("", |m| m.as_str());
    let text = trailing_hashes_regex().replace(raw, "").trim().to_string();
    Some((level, text))
}

/// Every ATX heading in `body`, skipping headings inside fenced code blocks.
pub fn headings(body: &str) -> Vec<Heading> {
    let mut out = Vec::new();
    let mut offset = 0usize;
    let mut fence = FenceState::default();
    for line in body.split_inclusive('\n') {
        let content = line.strip_suffix('\n').unwrap_or(line);
        let content = content.strip_suffix('\r').unwrap_or(content);
        if fence.observe(content) || fence.in_fence() {
            offset += line.len();
            continue;
        }
        if let Some((level, text)) = parse_heading(content) {
            out.push(Heading {
                start: offset,
                end: offset + line.len(),
                level,
                text,
            });
        }
        offset += line.len();
    }
    out
}

/// A document split at its leading YAML frontmatter fence.
pub struct FrontmatterSplit<'a> {
    /// The raw YAML text between the `---` fences (the fence lines themselves
    /// excluded), or `None` when the document has no frontmatter, or an
    /// unterminated one.
    pub frontmatter: Option<&'a str>,
    /// Everything after the closing fence's `---`. The whole input when there is
    /// no frontmatter (or it never closed). Taken byte-for-byte from the source —
    /// never trimmed — so writers round-trip the body unchanged; a caller wanting
    /// a trimmed body trims this itself.
    pub body: &'a str,
    /// True when a `---` fence opened the document but was never closed.
    pub unclosed: bool,
}

/// Splits `raw` into YAML frontmatter and body at a leading `---`…`---` fence.
/// This is the single frontmatter splitter the core reads with (entity parse,
/// document round-trip, daily notes, lists). Compared with the naive
/// `starts_with("---\n")` + `find("\n---")` it replaced, it:
///
/// - tolerates a leading UTF-8 BOM some editors prepend;
/// - requires the opening `---` to be the document's very first line (Obsidian
///   only treats frontmatter written on line 1 as frontmatter);
/// - requires each fence to be a line of exactly `---` (trailing whitespace
///   allowed), so a `----` horizontal rule or a `---text` line inside the YAML
///   is not mistaken for the close;
/// - keeps the body byte-for-byte (the byte just past the closing `---` onward),
///   matching the offsets every reader used before so nothing round-trips
///   differently.
pub fn split_frontmatter(raw: &str) -> FrontmatterSplit<'_> {
    // Skip a leading UTF-8 BOM if present; offsets stay relative to `raw`.
    let bom = raw
        .strip_prefix('\u{FEFF}')
        .map_or(0, |rest| raw.len() - rest.len());
    let after_bom = &raw[bom..];

    // The opening fence must be the document's first line: exactly `---` (only
    // trailing whitespace before its newline).
    let Some(rest) = after_bom.strip_prefix("---") else {
        return FrontmatterSplit {
            frontmatter: None,
            body: raw,
            unclosed: false,
        };
    };
    let Some(newline) = rest.find('\n') else {
        return FrontmatterSplit {
            frontmatter: None,
            body: raw,
            unclosed: false,
        };
    };
    if !rest[..newline].trim().is_empty() {
        return FrontmatterSplit {
            frontmatter: None,
            body: raw,
            unclosed: false,
        };
    }

    // Byte offset (in `raw`) where the YAML text begins, just past `---\n`.
    let yaml_start = raw.len() - rest.len() + newline + 1;

    // Closing fence: the first later line that is exactly `---`. `find("\n---")`
    // anchors the run to a line start; the trailing check rejects `----`/`---x`.
    let mut cursor = yaml_start;
    while let Some(pos) = raw[cursor..].find("\n---") {
        let dash_start = cursor + pos + 1; // the first `-` of the candidate fence
        let after_dashes = dash_start + 3; // just past the three dashes
        let line_tail = raw[after_dashes..].split('\n').next().unwrap_or("");
        if line_tail.trim().is_empty() {
            return FrontmatterSplit {
                frontmatter: Some(&raw[yaml_start..dash_start - 1]),
                body: &raw[after_dashes..],
                unclosed: false,
            };
        }
        // Not a real fence — keep scanning past this candidate.
        cursor = after_dashes;
    }

    FrontmatterSplit {
        frontmatter: None,
        body: raw,
        unclosed: true,
    }
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
pub fn sanitize_heading(value: &str) -> String {
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
    fn headings_respect_tilde_fences_and_length() {
        // A `~~~` fence, and a shorter ``` run inside a longer one, are both code.
        let body = "# A\n~~~\n# tilde-fake\n~~~\n````\n```\n# nested-fake\n```\n````\n# B\n";
        let found: Vec<_> = headings(body).into_iter().map(|h| h.text).collect();
        assert_eq!(found, vec!["A".to_string(), "B".to_string()]);
    }

    #[test]
    fn strip_fenced_code_drops_both_fence_flavours() {
        // Wikilink-looking text inside ``` and ~~~ blocks must not survive.
        let body =
            "keep [[A]]\n```\ncode [[X]]\n```\nmid [[B]]\n~~~\ncode [[Y]]\n~~~\ntail [[C]]\n";
        let stripped = strip_fenced_code(body);
        assert!(stripped.contains("[[A]]"));
        assert!(stripped.contains("[[B]]"));
        assert!(stripped.contains("[[C]]"));
        assert!(!stripped.contains("[[X]]"));
        assert!(!stripped.contains("[[Y]]"));
        // The fence delimiter lines are gone too.
        assert!(!stripped.contains("```"));
        assert!(!stripped.contains("~~~"));
    }

    #[test]
    fn split_frontmatter_reads_yaml_and_keeps_body_verbatim() {
        let split = split_frontmatter("---\ntitle: X\n---\n\nBody.\n");
        assert_eq!(split.frontmatter, Some("title: X"));
        assert!(!split.unclosed);
        // Body is everything past the closing `---`, byte-for-byte (leading blank
        // line preserved), matching the pre-consolidation split offsets.
        assert_eq!(split.body, "\n\nBody.\n");
    }

    #[test]
    fn split_frontmatter_without_fence_returns_whole_input_as_body() {
        let split = split_frontmatter("Just a body.\n");
        assert_eq!(split.frontmatter, None);
        assert!(!split.unclosed);
        assert_eq!(split.body, "Just a body.\n");
    }

    #[test]
    fn split_frontmatter_reports_unclosed_fence() {
        let split = split_frontmatter("---\ntitle: X\n");
        assert_eq!(split.frontmatter, None);
        assert!(split.unclosed);
        assert_eq!(split.body, "---\ntitle: X\n");
    }

    #[test]
    fn split_frontmatter_tolerates_a_leading_bom() {
        let split = split_frontmatter("\u{FEFF}---\ntitle: X\n---\nBody\n");
        assert_eq!(split.frontmatter, Some("title: X"));
        assert_eq!(split.body, "\nBody\n");
    }

    #[test]
    fn split_frontmatter_does_not_close_on_a_rule_or_dashed_text() {
        // A `----` horizontal rule and a `---note` line inside the YAML must not be
        // mistaken for the closing fence; the real `---` line closes it.
        let split = split_frontmatter("---\na: 1\n----\nb: 2\n---\nBody\n");
        assert_eq!(split.frontmatter, Some("a: 1\n----\nb: 2"));
        assert_eq!(split.body, "\nBody\n");
    }

    #[test]
    fn split_frontmatter_requires_the_fence_on_the_first_line() {
        // A blank line before `---` means no frontmatter (Obsidian's rule).
        let split = split_frontmatter("\n---\ntitle: X\n---\nBody\n");
        assert_eq!(split.frontmatter, None);
        assert!(!split.unclosed);
    }

    #[test]
    fn parse_heading_reads_level_and_strips_trailing_hashes() {
        assert_eq!(parse_heading("## Title ##"), Some((2, "Title".to_string())));
        assert_eq!(
            parse_heading("   # Indented"),
            Some((1, "Indented".to_string()))
        );
        assert_eq!(parse_heading("not a heading"), None);
        // Four-space indent is a code line, not a heading.
        assert_eq!(parse_heading("    # Deep"), None);
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
