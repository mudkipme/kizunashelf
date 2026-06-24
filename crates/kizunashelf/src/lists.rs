//! Lists: user-curated collections stored as **plain Markdown** in
//! `KizunaShelf/Lists/`, "files over apps" style.
//!
//! The shape of a list file mirrors how someone would hand-author it in Obsidian:
//!
//! ```markdown
//! Some prose describing the list.   <- description (everything before the list)
//!
//! - [[A Movie]]                     <- the first ul/ol IS the items
//! - [[Another Movie]]
//!
//! More notes underneath.            <- trailing markdown (everything after)
//! ```
//!
//! This module is pure parsing/rendering plus wikilink helpers; it has no I/O and
//! no knowledge of the API contract. The handlers in `api/lists.rs` compose these
//! with the [`Vfs`](crate::vfs::Vfs) and the [`Library`](crate::types::Library).
//!
//! No Markdown crate is used — the same lightweight regex/line approach as the rest
//! of the core (`library/frontmatter.rs`, `daily_notes.rs`).

use crate::daily_notes::normalize_wikilink_target;
use crate::library::wikilink_regex;
use crate::types::EntityRecord;
use regex::Regex;
use std::collections::HashMap;
use std::sync::OnceLock;

/// Vault-relative directory that holds every list. Sits next to the schema config
/// (`KizunaShelf/config.yaml`) so lists travel inside the vault on sync.
pub const LISTS_DIR: &str = "KizunaShelf/Lists";

/// A list's body decomposed into its three editable regions plus the items of the
/// first Markdown list. The frontmatter (if any) is handled separately by the
/// caller so it round-trips verbatim.
#[derive(Clone, Debug, PartialEq)]
pub struct ParsedList {
    /// Everything before the first list line.
    pub description: String,
    /// One entry per top-level item, holding the raw content after the list marker
    /// (so annotations like `[[X]] — rewatch` and nested lines are preserved).
    pub items: Vec<String>,
    /// `true` when the first list uses an ordered marker (`1.`/`1)`).
    pub ordered: bool,
    /// Everything after the first list block.
    pub trailing: String,
    /// `true` when the body contained no Markdown list at all (so `description`
    /// holds the whole body and `items`/`trailing` are empty).
    pub has_list: bool,
}

/// Splits a raw list file into `(frontmatter_block, body)`. The frontmatter block
/// — the leading `---\n … \n---` fence, without its trailing newline — is returned
/// verbatim so re-composing never reorders or reformats hand-authored YAML. Lists
/// are plain Markdown and usually have no frontmatter, but any present is kept.
pub fn split_frontmatter(raw: &str) -> (Option<String>, String) {
    if !raw.starts_with("---\n") {
        return (None, raw.to_string());
    }
    let Some(end) = raw[4..].find("\n---").map(|index| index + 4) else {
        return (None, raw.to_string());
    };
    let prefix = raw[..end + 4].to_string();
    // `end + 4` lands right after the closing `---`; the body starts after the
    // newline that follows it (stripped so re-composition controls the separator).
    let body = raw[end + 4..].strip_prefix('\n').unwrap_or(&raw[end + 4..]);
    (Some(prefix), body.to_string())
}

/// Re-attaches a (possibly absent) frontmatter block to a rendered body.
pub fn compose_document(frontmatter: &Option<String>, body: &str) -> String {
    match frontmatter {
        Some(prefix) if body.is_empty() => format!("{prefix}\n"),
        Some(prefix) => format!("{prefix}\n{body}"),
        None => body.to_string(),
    }
}

fn list_item_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    // A Markdown list line: optional indent, an unordered (`-`/`*`/`+`) or ordered
    // (`N.`/`N)`) marker, at least one space, then the content.
    RE.get_or_init(|| Regex::new(r"^([ \t]*)([-*+]|\d+[.)])[ \t]+(.*)$").unwrap())
}

struct ItemLine<'a> {
    indent: usize,
    ordered: bool,
    content: &'a str,
}

fn parse_item_line(line: &str) -> Option<ItemLine<'_>> {
    let caps = list_item_regex().captures(line)?;
    let indent = caps.get(1).map_or(0, |m| m.as_str().chars().count());
    let marker = caps.get(2).map_or("", |m| m.as_str());
    let content = caps.get(3).map_or("", |m| m.as_str());
    Some(ItemLine {
        indent,
        ordered: marker.chars().next().is_some_and(|c| c.is_ascii_digit()),
        content,
    })
}

/// Parses a list body into its description, first-list items, ordered flag, and
/// trailing markdown. When there is no list, everything lands in `description`.
pub fn parse_list(body: &str) -> ParsedList {
    let lines: Vec<&str> = body.split('\n').collect();

    let Some((start, base_indent, ordered)) = lines
        .iter()
        .enumerate()
        .find_map(|(i, line)| parse_item_line(line).map(|item| (i, item.indent, item.ordered)))
    else {
        return ParsedList {
            description: body.to_string(),
            items: Vec::new(),
            ordered: false,
            trailing: String::new(),
            has_list: false,
        };
    };

    let description = lines[..start].join("\n");
    let mut items: Vec<String> = Vec::new();
    // Index just past the last line consumed into the list block; anything from
    // here on (including a blank line that terminated the block) is trailing.
    let mut consumed_end = start;
    let mut i = start;
    while i < lines.len() {
        let line = lines[i];
        if let Some(item) = parse_item_line(line) {
            if item.indent == base_indent {
                items.push(item.content.to_string());
            } else if let Some(last) = items.last_mut() {
                // A more/less indented marker is a nested item — keep it attached to
                // the current top-level item so it survives a re-render.
                last.push('\n');
                last.push_str(line);
            } else {
                break;
            }
            consumed_end = i + 1;
            i += 1;
            continue;
        }
        if line.trim().is_empty() {
            // A blank line stays inside the block only if the list continues after
            // it (a "loose" list); otherwise it begins the trailing section.
            let next = lines[i + 1..]
                .iter()
                .position(|l| !l.trim().is_empty())
                .map(|offset| i + 1 + offset);
            match next {
                Some(j)
                    if parse_item_line(lines[j]).is_some_and(|item| item.indent == base_indent) =>
                {
                    i = j;
                    continue;
                }
                _ => break,
            }
        }
        if line.starts_with(' ') || line.starts_with('\t') {
            // Indented continuation paragraph under the current item.
            if let Some(last) = items.last_mut() {
                last.push('\n');
                last.push_str(line);
                consumed_end = i + 1;
                i += 1;
                continue;
            }
        }
        break;
    }

    let trailing = if consumed_end < lines.len() {
        lines[consumed_end..].join("\n")
    } else {
        String::new()
    };

    ParsedList {
        description,
        items,
        ordered,
        trailing,
        has_list: true,
    }
}

/// Renders a list body from its parts. Sections are separated by a single blank
/// line and items re-emitted with `- ` (unordered) or `1.`, `2.`, … (ordered)
/// markers, so the output is clean and idempotent under re-parsing.
pub fn render_list(description: &str, items: &[String], ordered: bool, trailing: &str) -> String {
    let mut sections: Vec<String> = Vec::new();
    let description = description.trim();
    if !description.is_empty() {
        sections.push(description.to_string());
    }
    if !items.is_empty() {
        let rendered: Vec<String> = items
            .iter()
            .enumerate()
            .map(|(index, item)| {
                if ordered {
                    format!("{}. {item}", index + 1)
                } else {
                    format!("- {item}")
                }
            })
            .collect();
        sections.push(rendered.join("\n"));
    }
    let trailing = trailing.trim();
    if !trailing.is_empty() {
        sections.push(trailing.to_string());
    }
    if sections.is_empty() {
        return String::new();
    }
    format!("{}\n", sections.join("\n\n"))
}

/// The first wikilink target inside an item's content, or `None` for a plain-text
/// item. Used to resolve an item to an entity for display.
pub fn item_target(content: &str) -> Option<String> {
    wikilink_regex()
        .captures(content)
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str().trim().to_string())
        .filter(|target| !target.is_empty())
}

/// Whether `basename` is shared by more than one entity (so a bare `[[basename]]`
/// would be ambiguous and the full path should be used instead).
pub fn basename_ambiguous(index: &HashMap<String, Vec<&EntityRecord>>, basename: &str) -> bool {
    index
        .get(&normalize_wikilink_target(basename))
        .is_some_and(|records| records.len() > 1)
}

/// Builds the wikilink content for an entity, Obsidian-style: a bare `[[basename]]`
/// normally, but the full vault path (minus `.md`) when the basename is ambiguous.
pub fn entity_wikilink(basename: &str, path: &str, ambiguous: bool) -> String {
    let target = if ambiguous {
        path.strip_suffix(".md").unwrap_or(path)
    } else {
        basename
    };
    format!("[[{target}]]")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_description_items_and_trailing() {
        let body = "My favourites.\n\n- [[A]]\n- [[B]]\n\nSee also below.";
        let parsed = parse_list(body);
        assert!(parsed.has_list);
        assert!(!parsed.ordered);
        assert_eq!(parsed.description, "My favourites.\n");
        assert_eq!(parsed.items, vec!["[[A]]".to_string(), "[[B]]".to_string()]);
        assert_eq!(parsed.trailing, "\nSee also below.");
    }

    #[test]
    fn detects_ordered_lists() {
        let parsed = parse_list("1. [[A]]\n2. [[B]]");
        assert!(parsed.ordered);
        assert_eq!(parsed.items, vec!["[[A]]".to_string(), "[[B]]".to_string()]);
    }

    #[test]
    fn no_list_keeps_everything_as_description() {
        let parsed = parse_list("Just prose, no list here.");
        assert!(!parsed.has_list);
        assert_eq!(parsed.description, "Just prose, no list here.");
        assert!(parsed.items.is_empty());
    }

    #[test]
    fn render_round_trips_and_is_idempotent() {
        let body = "Desc.\n\n- [[A]]\n- [[B]] — note\n\nTrailing.";
        let parsed = parse_list(body);
        let rendered = render_list(
            &parsed.description,
            &parsed.items,
            parsed.ordered,
            &parsed.trailing,
        );
        let reparsed = parse_list(&rendered);
        assert_eq!(reparsed.items, parsed.items);
        assert_eq!(reparsed.description.trim(), "Desc.");
        assert_eq!(reparsed.trailing.trim(), "Trailing.");
        // Rendering the reparsed parts again yields the exact same string.
        let rerendered = render_list(
            &reparsed.description,
            &reparsed.items,
            reparsed.ordered,
            &reparsed.trailing,
        );
        assert_eq!(rendered, rerendered);
    }

    #[test]
    fn toggling_ordered_rewrites_markers() {
        let parsed = parse_list("- [[A]]\n- [[B]]");
        let rendered = render_list(&parsed.description, &parsed.items, true, &parsed.trailing);
        assert_eq!(rendered, "1. [[A]]\n2. [[B]]\n");
    }

    #[test]
    fn loose_list_with_blank_lines_between_items() {
        let parsed = parse_list("- [[A]]\n\n- [[B]]\n\nAfter.");
        assert_eq!(parsed.items, vec!["[[A]]".to_string(), "[[B]]".to_string()]);
        assert_eq!(parsed.trailing.trim(), "After.");
    }

    #[test]
    fn item_target_extracts_first_wikilink_or_none() {
        assert_eq!(item_target("[[Inception]]"), Some("Inception".to_string()));
        assert_eq!(
            item_target("[[Movies/Inception]] — rewatch"),
            Some("Movies/Inception".to_string())
        );
        assert_eq!(item_target("just text"), None);
    }

    #[test]
    fn entity_wikilink_uses_full_path_only_when_ambiguous() {
        assert_eq!(
            entity_wikilink("Inception", "Movies/Inception.md", false),
            "[[Inception]]"
        );
        assert_eq!(
            entity_wikilink("Inception", "Movies/Inception.md", true),
            "[[Movies/Inception]]"
        );
    }

    #[test]
    fn frontmatter_splits_and_recomposes_verbatim() {
        let raw = "---\ntags: [a]\n---\n- [[A]]\n";
        let (prefix, body) = split_frontmatter(raw);
        assert_eq!(prefix.as_deref(), Some("---\ntags: [a]\n---"));
        assert_eq!(body, "- [[A]]\n");
        assert_eq!(compose_document(&prefix, &body), raw);
    }

    #[test]
    fn frontmatter_absent_is_passthrough() {
        let (prefix, body) = split_frontmatter("- [[A]]\n");
        assert_eq!(prefix, None);
        assert_eq!(body, "- [[A]]\n");
        assert_eq!(compose_document(&prefix, &body), "- [[A]]\n");
    }
}
