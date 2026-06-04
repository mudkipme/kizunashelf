use crate::library::wikilink_regex;
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
    let mut in_fence = false;

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
        let trimmed = line.trim();
        if fence_line_regex().is_match(trimmed) {
            flush_paragraph(&mut blocks, &mut paragraph, heading.clone());
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        if let Some(captures) = heading_regex().captures(trimmed) {
            flush_paragraph(&mut blocks, &mut paragraph, heading.clone());
            heading = captures
                .get(2)
                .map(|capture| clean_mention_snippet(capture.as_str(), 120));
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

fn fence_line_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^(```|~~~)").unwrap())
}

fn heading_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^(#{1,6})\s+(.+)$").unwrap())
}

fn list_or_quote_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^([-*+]|\d+\.)\s+|^>\s+").unwrap())
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
