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
    let parsed = parse_section(body, &section.heading);
    let total = parsed.groups.iter().map(|group| group.items.len()).sum();
    let watched = parsed
        .groups
        .iter()
        .flat_map(|group| &group.items)
        .filter(|episode| episode.watched)
        .count();
    EntityEpisodes {
        heading: section.heading.clone(),
        tracking,
        groups: parsed.groups,
        total,
        watched,
        description: parsed.description,
        trailing: parsed.trailing,
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

/// The parsed episodes section: the structured groups plus the free prose that
/// sits *above* the first list item/sub-heading (`description`) and *below* the
/// last list item (`trailing`). The prose is kept so the dedicated episodes UI can
/// display hand-written notes the user wraps around the list — the body's generic
/// render drops the whole section to avoid duplication.
struct ParsedSection {
    groups: Vec<EpisodeGroup>,
    description: String,
    trailing: String,
}

fn parse_section(body: &str, heading: &str) -> ParsedSection {
    let Some(section) = find_section(body, heading) else {
        return ParsedSection {
            groups: Vec::new(),
            description: String::new(),
            trailing: String::new(),
        };
    };
    let content = &body[section.content_start..section.end];
    // Byte offset of the first structural line (a group sub-heading or list item)
    // and the end of the last list item — the boundaries of the surrounding prose.
    let mut first_struct: Option<usize> = None;
    let mut last_item_end: Option<usize> = None;
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
            first_struct.get_or_insert(line_start);
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
            first_struct.get_or_insert(line_start);
            last_item_end = Some(offset);
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

    // Prose above the first structural line, and after the last list item. With no
    // structure at all, the whole section is treated as leading prose.
    let description = match first_struct {
        Some(start) => content[..start].trim(),
        None => content.trim(),
    }
    .to_string();
    let trailing = match last_item_end {
        Some(end) => content[end..].trim(),
        None => "",
    }
    .to_string();

    ParsedSection {
        groups,
        description,
        trailing,
    }
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

/// Merges provider-fetched `incoming` episode groups into the entity's `existing`
/// episodes, returning the merged groups (to be rendered + written). The merge is
/// deliberately non-destructive (the import UX promise):
/// - existing groups/items are the base — order and **watched** state are kept;
/// - a matched item (same group label + `key`) keeps its watched flag and gains a
///   title when its existing title is empty; when `overwrite` is set it also takes
///   the incoming title over a non-empty existing one (the user ticked it to update);
/// - new incoming items are appended to their group, new incoming groups appended;
/// - existing items/groups absent from `incoming` are kept (hand-added or
///   removed-upstream entries survive). Re-syncing unchanged data is a no-op.
pub fn merge_episodes(
    existing: &EntityEpisodes,
    incoming: &[EpisodeGroup],
    overwrite: bool,
) -> Vec<EpisodeGroup> {
    let mut result: Vec<EpisodeGroup> = existing.groups.clone();
    for incoming_group in incoming {
        match result
            .iter_mut()
            .find(|group| same_label(&group.label, &incoming_group.label))
        {
            Some(group) => {
                for item in &incoming_group.items {
                    match group.items.iter_mut().find(|existing_item| {
                        !item.key.trim().is_empty() && existing_item.key.trim() == item.key.trim()
                    }) {
                        Some(existing_item) => {
                            // Fill an empty title always; overwrite a non-empty one only
                            // when asked and the incoming title isn't itself empty.
                            if existing_item.title.trim().is_empty()
                                || (overwrite && !item.title.trim().is_empty())
                            {
                                existing_item.title = item.title.clone();
                            }
                        }
                        None => group.items.push(Episode {
                            key: item.key.clone(),
                            title: item.title.clone(),
                            watched: false,
                        }),
                    }
                }
            }
            None => result.push(EpisodeGroup {
                label: incoming_group.label.clone(),
                items: incoming_group
                    .items
                    .iter()
                    .map(|item| Episode {
                        key: item.key.clone(),
                        title: item.title.clone(),
                        watched: false,
                    })
                    .collect(),
            }),
        }
    }
    result
}

fn same_label(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn section() -> BodySection {
        BodySection {
            heading: "Episodes".to_string(),
            kind: BodySectionKind::Episodes,
            external_fields: Vec::new(),
            tracking: Some(EpisodeTracking::Checklist),
        }
    }

    fn episode(key: &str, title: &str, watched: bool) -> Episode {
        Episode {
            key: key.to_string(),
            title: title.to_string(),
            watched,
        }
    }

    fn unwatched(key: &str, title: &str) -> Episode {
        episode(key, title, false)
    }

    fn entity_episodes(groups: Vec<EpisodeGroup>) -> EntityEpisodes {
        EntityEpisodes {
            heading: "Episodes".to_string(),
            tracking: EpisodeTracking::Checklist,
            total: groups.iter().map(|g| g.items.len()).sum(),
            watched: groups
                .iter()
                .flat_map(|g| &g.items)
                .filter(|e| e.watched)
                .count(),
            groups,
            description: String::new(),
            trailing: String::new(),
        }
    }

    #[test]
    fn parse_captures_prose_above_and_below_the_list() {
        let body =
            "## Episodes\nWatch order notes.\n\n- 1 · Pilot\n- 2 · Dawn\n\nMore after the list.\n";
        let parsed = parse_episodes(body, &section());
        assert_eq!(parsed.description, "Watch order notes.");
        assert_eq!(parsed.trailing, "More after the list.");
        assert_eq!(parsed.total, 2);
    }

    #[test]
    fn parse_prose_with_groups_is_above_first_subheading() {
        let body = "## Episodes\nIntro.\n\n### Season 1\n- 1 · A\n\nOutro.\n";
        let parsed = parse_episodes(body, &section());
        assert_eq!(parsed.description, "Intro.");
        assert_eq!(parsed.trailing, "Outro.");
        assert_eq!(parsed.groups.len(), 1);
        assert_eq!(parsed.groups[0].label, "Season 1");
    }

    #[test]
    fn merge_keeps_watched_fills_empty_titles_and_keeps_extras() {
        let existing = entity_episodes(vec![EpisodeGroup {
            label: "Season 1".to_string(),
            items: vec![
                episode("1", "Pilot", true),     // watched + edited title
                episode("2", "", false),         // title to be filled
                episode("99", "My Extra", true), // local-only, must survive
            ],
        }]);
        let incoming = vec![EpisodeGroup {
            label: "season 1".to_string(), // label match is case/space-insensitive
            items: vec![
                unwatched("1", "Pilot (provider)"),
                unwatched("2", "Journey"),
                unwatched("3", "Dawn"),
            ],
        }];

        let merged = merge_episodes(&existing, &incoming, false);
        assert_eq!(merged.len(), 1);
        let items = &merged[0].items;
        assert_eq!(items[0].key, "1");
        assert_eq!(items[0].title, "Pilot"); // edited title NOT overwritten
        assert!(items[0].watched); // watched preserved
        assert_eq!(items[1].title, "Journey"); // empty title filled
        assert!(items.iter().any(|e| e.key == "99" && e.watched)); // extra kept
        assert!(items.iter().any(|e| e.key == "3")); // new appended

        // Re-syncing the same incoming is a no-op.
        let again = merge_episodes(&entity_episodes(merged.clone()), &incoming, false);
        assert_eq!(again, merged);
    }

    #[test]
    fn merge_overwrite_replaces_titles_but_keeps_watched_and_empty_incoming() {
        let existing = entity_episodes(vec![EpisodeGroup {
            label: "Season 1".to_string(),
            items: vec![
                episode("1", "Pilot", true), // watched + edited title
                episode("2", "Old name", false),
            ],
        }]);
        let incoming = vec![EpisodeGroup {
            label: "Season 1".to_string(),
            items: vec![
                unwatched("1", "Pilot (provider)"),
                unwatched("2", ""), // empty incoming title must not blank the existing one
            ],
        }];

        let merged = merge_episodes(&existing, &incoming, true);
        let items = &merged[0].items;
        assert_eq!(items[0].title, "Pilot (provider)"); // overwritten on request
        assert!(items[0].watched); // watched still preserved
        assert_eq!(items[1].title, "Old name"); // empty incoming leaves the title alone
    }

    #[test]
    fn merge_appends_new_groups_and_flat_into_ungrouped() {
        // New season group is appended.
        let existing = entity_episodes(vec![EpisodeGroup {
            label: "Season 1".to_string(),
            items: vec![episode("1", "A", true)],
        }]);
        let incoming = vec![EpisodeGroup {
            label: "Season 2".to_string(),
            items: vec![unwatched("1", "B")],
        }];
        let merged = merge_episodes(&existing, &incoming, false);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[1].label, "Season 2");

        // Flat import merges into the existing ungrouped group.
        let flat_existing = entity_episodes(vec![EpisodeGroup {
            label: String::new(),
            items: vec![episode("1", "A", true)],
        }]);
        let flat_incoming = vec![EpisodeGroup {
            label: String::new(),
            items: vec![unwatched("1", "A"), unwatched("2", "B")],
        }];
        let flat_merged = merge_episodes(&flat_existing, &flat_incoming, false);
        assert_eq!(flat_merged.len(), 1);
        assert_eq!(flat_merged[0].items.len(), 2);
        assert!(flat_merged[0].items[0].watched); // ep 1 stays watched
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
