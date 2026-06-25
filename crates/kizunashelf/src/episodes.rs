//! The built-in **episodes / tracks / chapters** list — a `BodySection` of
//! `kind = episodes`. It lives in the entity's Markdown body under a configurable
//! heading as an (optionally task-checked) list, optionally grouped by season/disc
//! sub-headings. Plain Markdown, Obsidian-editable, "files over apps".
//!
//! This module is the single owner of episode parsing/rendering, so web and iOS
//! render from structured data instead of re-implementing it. No Markdown crate —
//! it composes [`crate::markdown`] (heading sections) with a list-item regex.

use crate::contract::{EntityEpisodes, Episode, EpisodeGroup};
use crate::markdown::{find_section, headings, splice_section};
use crate::types::{
    BodySection, BodySectionKind, EntityTypeConfig, EpisodeProgress, EpisodeTracking,
};
use regex::Regex;
use std::sync::OnceLock;

/// The first episodes-kind body section declared by a type, if any.
pub fn episode_section(type_config: &EntityTypeConfig) -> Option<&BodySection> {
    type_config
        .body_sections
        .iter()
        .find(|section| section.kind == BodySectionKind::Episodes)
}

/// `tracking` resolved with its default (`checklist`).
fn resolved_tracking(section: &BodySection) -> EpisodeTracking {
    section.tracking.unwrap_or(EpisodeTracking::Checklist)
}

fn item_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    // indent, marker, optional task-list checkbox, content.
    RE.get_or_init(|| {
        Regex::new(r"^[ \t]*([-*+]|\d+[.)])[ \t]+(?:\[([ xX])\][ \t]+)?(.*)$").unwrap()
    })
}

/// The leading episode number in an item's content (e.g. `12.5`, `#5`, `E12`).
fn key_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^(?:#|[eE][pP]?[ \t]*)?(\d+(?:\.\d+)?)").unwrap())
}

struct ParsedItem<'a> {
    watched: bool,
    content: &'a str,
    /// The numeric part of an ordered-list marker (e.g. `12` from `12.`), used as a
    /// fallback key when the content carries no number.
    marker_number: Option<&'a str>,
}

fn parse_item(line: &str) -> Option<ParsedItem<'_>> {
    let caps = item_regex().captures(line)?;
    let marker = caps.get(1).map_or("", |m| m.as_str());
    let watched = caps.get(2).is_some_and(|m| matches!(m.as_str(), "x" | "X"));
    let content = caps.get(3).map_or("", |m| m.as_str());
    let marker_number = marker
        .chars()
        .next()
        .filter(|c| c.is_ascii_digit())
        .map(|_| marker.trim_end_matches(['.', ')']));
    Some(ParsedItem {
        watched,
        content,
        marker_number,
    })
}

/// Splits item content into `(key, title)`: a leading number in the content wins;
/// otherwise an ordered-list marker number is the key; otherwise no key.
fn split_key_title(content: &str, marker_number: Option<&str>) -> (String, String) {
    let trimmed = content.trim();
    if let Some(caps) = key_regex().captures(trimmed) {
        let key = caps.get(1).unwrap().as_str().to_string();
        let rest = &trimmed[caps.get(0).unwrap().end()..];
        return (key, strip_leading_separator(rest).to_string());
    }
    if let Some(marker_number) = marker_number {
        return (marker_number.to_string(), trimmed.to_string());
    }
    (String::new(), trimmed.to_string())
}

fn strip_leading_separator(value: &str) -> &str {
    value.trim_start_matches([' ', '\t', '·', '—', '–', '-', '.', ':', '|', '、'])
}

/// Parses the entity's episodes section into structured groups + a watched roll-up.
/// An absent heading yields an empty (but configured) result.
pub fn parse_episodes(body: &str, section: &BodySection) -> EntityEpisodes {
    let tracking = resolved_tracking(section);
    let groups = parse_groups(body, &section.heading);
    let total = groups.iter().map(|group| group.items.len()).sum();
    let watched = groups
        .iter()
        .flat_map(|group| &group.items)
        .filter(|episode| episode.watched)
        .count();
    EntityEpisodes {
        heading: section.heading.clone(),
        item_noun: section
            .item_noun
            .clone()
            .filter(|noun| !noun.trim().is_empty())
            .unwrap_or_else(|| "Episode".to_string()),
        tracking,
        groups,
        total,
        watched,
    }
}

/// The cheap resident roll-up for the entity summary — counts items/checked in the
/// section without building the full structure.
pub fn episode_progress(body: &str, section: &BodySection) -> EpisodeProgress {
    let Some(found) = find_section(body, &section.heading) else {
        return EpisodeProgress {
            watched: 0,
            total: 0,
        };
    };
    let content = &body[found.content_start..found.end];
    let mut total = 0;
    let mut watched = 0;
    for line in content.lines() {
        if let Some(item) = parse_item(line) {
            total += 1;
            if item.watched {
                watched += 1;
            }
        }
    }
    EpisodeProgress { watched, total }
}

fn parse_groups(body: &str, heading: &str) -> Vec<EpisodeGroup> {
    let Some(section) = find_section(body, heading) else {
        return Vec::new();
    };
    let content = &body[section.content_start..section.end];
    // Sub-headings inside the section are group labels (find_section already stopped
    // at the next same/higher heading, so every heading here is deeper).
    let group_starts: std::collections::HashMap<usize, String> = headings(content)
        .into_iter()
        .map(|h| (h.start, h.text))
        .collect();

    let mut groups: Vec<EpisodeGroup> = Vec::new();
    let mut current = EpisodeGroup {
        label: String::new(),
        items: Vec::new(),
    };
    let mut offset = 0usize;
    for line in content.split_inclusive('\n') {
        let line_start = offset;
        offset += line.len();
        if let Some(label) = group_starts.get(&line_start) {
            if !current.label.is_empty() || !current.items.is_empty() {
                groups.push(std::mem::replace(
                    &mut current,
                    EpisodeGroup {
                        label: String::new(),
                        items: Vec::new(),
                    },
                ));
            }
            current.label = label.clone();
            continue;
        }
        let text = line.strip_suffix('\n').unwrap_or(line);
        if let Some(item) = parse_item(text) {
            let (key, title) = split_key_title(item.content, item.marker_number);
            current.items.push(Episode {
                key,
                title,
                watched: item.watched,
            });
        }
    }
    if !current.label.is_empty() || !current.items.is_empty() {
        groups.push(current);
    }
    groups
}

/// Renders the episodes section body (without the heading line) for `splice_section`.
fn render_groups(groups: &[EpisodeGroup], tracking: EpisodeTracking) -> String {
    let mut blocks: Vec<String> = Vec::new();
    for group in groups {
        let mut lines: Vec<String> = Vec::new();
        let label = group.label.trim();
        if !label.is_empty() {
            lines.push(format!("### {label}"));
            lines.push(String::new());
        }
        for episode in &group.items {
            lines.push(render_item(episode, tracking));
        }
        let block = lines.join("\n").trim_end().to_string();
        if !block.is_empty() {
            blocks.push(block);
        }
    }
    blocks.join("\n\n")
}

fn render_item(episode: &Episode, tracking: EpisodeTracking) -> String {
    let checkbox = match tracking {
        EpisodeTracking::Checklist if episode.watched => "[x] ",
        EpisodeTracking::Checklist => "[ ] ",
        _ => "",
    };
    let key = episode.key.trim();
    let title = episode.title.trim();
    let label = match (key.is_empty(), title.is_empty()) {
        (true, _) => title.to_string(),
        (false, true) => key.to_string(),
        (false, false) => format!("{key} · {title}"),
    };
    format!("- {checkbox}{label}")
}

/// Renders `groups` back into `body`'s episodes section (replacing only that
/// section; appending the heading if absent). The body stays the source of truth.
pub fn apply_episodes(body: &str, section: &BodySection, groups: &[EpisodeGroup]) -> String {
    let rendered = render_groups(groups, resolved_tracking(section));
    splice_section(body, &section.heading, &rendered)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn section() -> BodySection {
        BodySection {
            heading: "Episodes".to_string(),
            kind: BodySectionKind::Episodes,
            external_fields: Vec::new(),
            item_noun: Some("Episode".to_string()),
            tracking: Some(EpisodeTracking::Checklist),
        }
    }

    #[test]
    fn parses_groups_specials_and_watched_state() {
        let body = "Intro\n\n## Episodes\n### Season 1\n- [x] 1 · Pilot\n- [ ] 12.5 · Recap\n\n### Season 2\n- [ ] 1 New Dawn\n\n## Notes\nkeep\n";
        let parsed = parse_episodes(body, &section());
        assert_eq!(parsed.total, 3);
        assert_eq!(parsed.watched, 1);
        assert_eq!(parsed.groups.len(), 2);
        assert_eq!(parsed.groups[0].label, "Season 1");
        assert_eq!(parsed.groups[0].items[1].key, "12.5");
        assert_eq!(parsed.groups[0].items[1].title, "Recap");
        assert_eq!(parsed.groups[1].items[0].key, "1");
        assert_eq!(parsed.groups[1].items[0].title, "New Dawn");
    }

    #[test]
    fn ungrouped_items_land_in_one_unlabeled_group() {
        let body = "## Episodes\n- [ ] 0 · Prologue\n- [x] 1\n";
        let parsed = parse_episodes(body, &section());
        assert_eq!(parsed.groups.len(), 1);
        assert_eq!(parsed.groups[0].label, "");
        assert_eq!(parsed.groups[0].items[0].key, "0");
        assert_eq!(parsed.watched, 1);
    }

    #[test]
    fn ordered_marker_number_is_used_when_content_has_none() {
        let body = "## Episodes\n1. Pilot\n2. The Crossing\n";
        let parsed = parse_episodes(body, &section());
        assert_eq!(parsed.groups[0].items[0].key, "1");
        assert_eq!(parsed.groups[0].items[0].title, "Pilot");
    }

    #[test]
    fn apply_round_trips_and_preserves_other_sections() {
        let body = "## Summary\nhi\n\n## Episodes\n- [ ] 1 · A\n\n## Notes\nkeep\n";
        let mut parsed = parse_episodes(body, &section());
        parsed.groups[0].items[0].watched = true;
        let next = apply_episodes(body, &section(), &parsed.groups);
        assert!(next.contains("## Summary\nhi"));
        assert!(next.contains("- [x] 1 · A"));
        assert!(next.contains("## Notes\nkeep"));
        // Re-parsing reflects the toggle.
        assert_eq!(parse_episodes(&next, &section()).watched, 1);
    }

    #[test]
    fn progress_matches_full_parse_cheaply() {
        let body = "## Episodes\n### S1\n- [x] 1\n- [ ] 2\n- [x] 3\n";
        let progress = episode_progress(body, &section());
        assert_eq!(progress.total, 3);
        assert_eq!(progress.watched, 2);
    }
}
