use crate::library::wikilink_regex;
use crate::markdown::{parse_heading, FenceState};
use regex::Regex;
use std::sync::OnceLock;

pub(super) struct MarkdownMentionBlock {
    pub(super) text: String,
    pub(super) heading: Option<String>,
    pub(super) line: usize,
}

pub(super) fn mention_blocks(markdown: &str) -> Vec<MarkdownMentionBlock> {
    let mut blocks = Vec::new();
    let mut heading: Option<String> = None;
    let mut paragraph: Vec<(String, usize)> = Vec::new();
    let mut fence = FenceState::default();

    fn push_block(
        blocks: &mut Vec<MarkdownMentionBlock>,
        text: String,
        line: usize,
        heading: Option<String>,
    ) {
        if wikilink_regex().is_match(&text) {
            blocks.push(MarkdownMentionBlock {
                text,
                heading,
                line,
            });
        }
    }

    fn flush_paragraph(
        blocks: &mut Vec<MarkdownMentionBlock>,
        paragraph: &mut Vec<(String, usize)>,
        heading: Option<String>,
    ) {
        if paragraph.is_empty() {
            return;
        }
        let text = paragraph
            .iter()
            .map(|(text, _)| text.as_str())
            .collect::<Vec<_>>()
            .join(" ")
            .trim()
            .to_string();
        push_block(blocks, text, paragraph[0].1, heading);
        paragraph.clear();
    }

    for (index, line) in markdown.lines().enumerate() {
        let line_number = index + 1;
        let inside_fence = fence.in_fence();
        if fence.observe(line) || inside_fence {
            // A fence delimiter (opening or closing) ends the current paragraph;
            // lines inside a fenced block are code, not mentions.
            flush_paragraph(&mut blocks, &mut paragraph, heading.clone());
            continue;
        }
        let trimmed = line.trim();
        if let Some((_, text)) = parse_heading(line) {
            flush_paragraph(&mut blocks, &mut paragraph, heading.clone());
            heading = Some(clean_mention_snippet(&text, 120));
            push_block(
                &mut blocks,
                trimmed.to_string(),
                line_number,
                heading.clone(),
            );
            continue;
        }
        if trimmed.is_empty() {
            flush_paragraph(&mut blocks, &mut paragraph, heading.clone());
            continue;
        }
        if list_or_quote_regex().is_match(trimmed) || trimmed.contains('|') {
            flush_paragraph(&mut blocks, &mut paragraph, heading.clone());
            push_block(
                &mut blocks,
                trimmed.to_string(),
                line_number,
                heading.clone(),
            );
            continue;
        }
        paragraph.push((trimmed.to_string(), line_number));
    }
    flush_paragraph(&mut blocks, &mut paragraph, heading);
    blocks
}

pub(super) fn clean_mention_snippet(text: &str, max_length: usize) -> String {
    let mut cleaned = wikilink_with_alias_regex()
        .replace_all(text, |captures: &regex::Captures| {
            captures
                .get(2)
                .or_else(|| captures.get(1))
                .map(|capture| capture.as_str())
                .unwrap_or_default()
                .to_string()
        })
        .to_string();
    cleaned = image_markdown_regex().replace_all(&cleaned, "").to_string();
    cleaned = markdown_link_regex()
        .replace_all(&cleaned, "$1")
        .to_string();
    cleaned = heading_prefix_regex().replace_all(&cleaned, "").to_string();
    cleaned = quote_prefix_regex().replace_all(&cleaned, "").to_string();
    cleaned = list_prefix_regex().replace_all(&cleaned, "").to_string();
    cleaned = whitespace_regex()
        .replace_all(&cleaned, " ")
        .trim()
        .to_string();
    if cleaned.chars().count() <= max_length {
        cleaned
    } else {
        format!(
            "{}...",
            cleaned
                .chars()
                .take(max_length.saturating_sub(3))
                .collect::<String>()
                .trim()
        )
    }
}

fn list_or_quote_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    // Ordered markers accept both `.` and `)` delimiters (mirrors the list-item
    // parsing in `lists.rs`/`episodes.rs`).
    RE.get_or_init(|| Regex::new(r"^([-*+]|\d+[.)])\s+|^>\s+").unwrap())
}

fn wikilink_with_alias_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"!?\[\[([^\]|#]+)(?:#[^\]|]+)?(?:\|([^\]]+))?\]\]").unwrap())
}

fn image_markdown_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"!\[[^\]]*]\([^)]+\)").unwrap())
}

fn markdown_link_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\[([^\]]+)]\([^)]+\)").unwrap())
}

fn heading_prefix_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^#{1,6}\s+").unwrap())
}

fn quote_prefix_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^>\s+").unwrap())
}

fn list_prefix_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^([-*+]|\d+\.)\s+").unwrap())
}

fn whitespace_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\s+").unwrap())
}
