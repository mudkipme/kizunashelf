//! The built-in **episodes / tracks / chapters** list — a `BodySection` of
//! `kind = episodes`. It lives in the entity's Markdown body under a configurable
//! heading as an (optionally task-checked) list, optionally grouped by season/disc
//! sub-headings. Plain Markdown, Obsidian-editable, "files over apps".
//!
//! This module is the single owner of episode parsing/rendering, so web and iOS
//! render from structured data instead of re-implementing it. No Markdown crate —
//! it composes [`crate::markdown`] (heading sections) with a list-item regex.

use crate::contract::{EntityEpisodes, Episode, EpisodeGroup};
use crate::markdown::{find_section, headings, splice_section, FenceState};
use crate::types::{
    BodySection, BodySectionKind, EntityTypeConfig, EpisodeDate, EpisodeDateRole, EpisodeProgress,
    EpisodeTracking,
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
    // marker, optional task-list checkbox, content. Column-0 items only: unlike
    // headings/fences (absolute ≤3-space rule), list nesting is relative, and a
    // bullet 2 spaces under a column-0 item is already its child — so any indented
    // bullet is content nested *under* an episode (the item's tail), not an
    // episode. `render_item` always emits at column 0, so round-trips are exact.
    RE.get_or_init(|| Regex::new(r"^([-*+]|\d+[.)])[ \t]+(?:\[([ xX])\][ \t]+)?(.*)$").unwrap())
}

/// The leading episode number in an item's content (e.g. `12.5`, `#5`, `E12`).
fn key_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^(?:#|[eE][pP]?[ \t]*)?(\d+(?:\.\d+)?)").unwrap())
}

/// The Obsidian Tasks date suffixes we round-trip: `📅` (due/air date) and `✅`
/// (completion date). We emit them at the end, but accept them anywhere.
fn due_date_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"📅\s*(\d{4}-\d{2}-\d{2})").unwrap())
}

fn done_date_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"✅\s*(\d{4}-\d{2}-\d{2})").unwrap())
}

/// Strips the `📅` and `✅` date suffixes out of an item's content, returning
/// `(content_without_dates, due_date, done_date)` so the dates survive the
/// round-trip as structured data instead of leaking into the title.
fn extract_dates(content: &str) -> (String, Option<String>, Option<String>) {
    let (content, due) = strip_emoji_date(content, due_date_regex());
    let (content, done) = strip_emoji_date(&content, done_date_regex());
    (content, due, done)
}

fn strip_emoji_date(content: &str, regex: &Regex) -> (String, Option<String>) {
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
    let layout = parse_layout(body, &section.heading);
    let groups: Vec<EpisodeGroup> = layout
        .groups
        .iter()
        .map(|group| EpisodeGroup {
            label: group.label.clone(),
            items: group
                .items
                .iter()
                .map(|item| item.episode.clone())
                .collect(),
        })
        .collect();
    let total = groups.iter().map(|group| group.items.len()).sum();
    let watched = groups
        .iter()
        .flat_map(|group| &group.items)
        .filter(|episode| episode.watched)
        .count();
    EntityEpisodes {
        heading: section.heading.clone(),
        tracking,
        groups,
        total,
        watched,
        description: layout.description.trim().to_string(),
        trailing: layout.trailing.trim().to_string(),
    }
}

/// Flattens the episodes section into the dated items the calendar places: one
/// [`EpisodeDate`] per air (`📅`) date and one per completion (`✅`) date.
/// Undated items contribute nothing. Parsed once at index time and cached on the
/// record, so the calendar never re-reads bodies.
pub fn episode_calendar_dates(body: &str, section: &BodySection) -> Vec<EpisodeDate> {
    let mut dates = Vec::new();
    for group in parse_layout(body, &section.heading).groups {
        for item in group.items {
            let item = item.episode;
            if let Some(date) = non_empty_date(&item.date) {
                dates.push(EpisodeDate {
                    key: item.key.clone(),
                    title: item.title.clone(),
                    date,
                    role: EpisodeDateRole::Scheduled,
                });
            }
            if let Some(date) = non_empty_date(&item.done) {
                dates.push(EpisodeDate {
                    key: item.key.clone(),
                    title: item.title.clone(),
                    date,
                    role: EpisodeDateRole::Completed,
                });
            }
        }
    }
    dates
}

fn non_empty_date(value: &Option<String>) -> Option<String> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|date| !date.is_empty())
        .map(str::to_string)
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
    let mut fence = FenceState::default();
    for line in content.lines() {
        let text = line.strip_suffix('\r').unwrap_or(line);
        if fence.observe(text) || fence.in_fence() {
            continue;
        }
        if let Some(item) = parse_item(text) {
            total += 1;
            if item.watched {
                watched += 1;
            }
        }
    }
    EpisodeProgress { watched, total }
}

/// The parsed episodes section with its full **layout**: the structured items plus
/// every non-item line captured verbatim, so a rewrite re-emits the user's own
/// text instead of dropping it — the prose above the first structural line
/// (`description`), each group's `lead` (lines between its sub-heading and first
/// item), each item's `tail` (nested bullets/continuation lines under it), and the
/// prose after the list (`trailing`). Description/trailing also surface on the
/// contract so the dedicated episodes UI can display them — the body's generic
/// render drops the whole section to avoid duplication.
struct SectionLayout {
    description: String,
    groups: Vec<GroupLayout>,
    trailing: String,
}

struct GroupLayout {
    label: String,
    /// Verbatim lines between the group sub-heading and its first item.
    lead: String,
    items: Vec<ItemLayout>,
}

struct ItemLayout {
    episode: Episode,
    /// Verbatim lines under the item (up to the next item/sub-heading), re-attached
    /// to the same episode on rewrite.
    tail: String,
}

fn parse_layout(body: &str, heading: &str) -> SectionLayout {
    let mut layout = SectionLayout {
        description: String::new(),
        groups: Vec::new(),
        trailing: String::new(),
    };
    let Some(section) = find_section(body, heading) else {
        return layout;
    };
    let content = &body[section.content_start..section.end];
    // Sub-headings inside the section are group labels (find_section already stopped
    // at the next same/higher heading, so every heading here is deeper).
    let group_starts: std::collections::HashMap<usize, String> = headings(content)
        .into_iter()
        .map(|h| (h.start, h.text))
        .collect();

    // `current` starts as `None` so everything before the first structural line
    // (sub-heading or item) accumulates as the description.
    let mut current: Option<GroupLayout> = None;
    let mut fence = FenceState::default();
    let mut offset = 0usize;
    for line in content.split_inclusive('\n') {
        let line_start = offset;
        offset += line.len();
        let text = line.strip_suffix('\n').unwrap_or(line);
        let text = text.strip_suffix('\r').unwrap_or(text);
        // Fence delimiters and lines inside a code block are never structural —
        // they flow into the surrounding description/lead/tail verbatim.
        let in_code = fence.observe(text) || fence.in_fence();
        if !in_code {
            if let Some(label) = group_starts.get(&line_start) {
                if let Some(group) = current.take() {
                    layout.groups.push(group);
                }
                current = Some(GroupLayout {
                    label: label.clone(),
                    lead: String::new(),
                    items: Vec::new(),
                });
                continue;
            }
            if let Some(item) = parse_item(text) {
                let (content, date, done) = extract_dates(item.content);
                let (key, title) = split_key_title(&content, item.marker_number);
                let group = current.get_or_insert_with(|| GroupLayout {
                    label: String::new(),
                    lead: String::new(),
                    items: Vec::new(),
                });
                group.items.push(ItemLayout {
                    episode: Episode {
                        key,
                        title,
                        watched: item.watched,
                        date,
                        done,
                    },
                    tail: String::new(),
                });
                continue;
            }
        }
        match current.as_mut() {
            Some(group) => match group.items.last_mut() {
                Some(item) => item.tail.push_str(line),
                None => group.lead.push_str(line),
            },
            None => layout.description.push_str(line),
        }
    }
    if let Some(group) = current.take() {
        layout.groups.push(group);
    }

    // The overall last item's tail ran to the section end, so it also swallowed the
    // trailing prose; split it back out at the first column-0 prose line.
    if let Some(item) = layout
        .groups
        .last_mut()
        .and_then(|group| group.items.last_mut())
    {
        let (tail, trailing) = split_trailing(&item.tail);
        item.tail = tail;
        layout.trailing = trailing;
    }
    layout
}

/// Splits a last-item tail at the first non-blank column-0 line: indented and
/// blank lines stay nested under the item, everything from the first flush-left
/// prose line onward is the section's trailing block.
fn split_trailing(tail: &str) -> (String, String) {
    let mut offset = 0usize;
    for line in tail.split_inclusive('\n') {
        let text = line.trim_end_matches(['\n', '\r']);
        if !text.trim().is_empty() && !text.starts_with([' ', '\t']) {
            return (tail[..offset].to_string(), tail[offset..].to_string());
        }
        offset += line.len();
    }
    (tail.to_string(), String::new())
}

/// Renders the episodes section body (without the heading line) for
/// `splice_section`, re-attaching the old layout's prose: the description above
/// the list, each matched group's lead, each matched item's tail, and the
/// trailing prose — so a rewrite only canonicalizes the item lines themselves and
/// never drops hand-written text.
fn render_section(
    layout: SectionLayout,
    groups: &[EpisodeGroup],
    tracking: EpisodeTracking,
) -> String {
    let mut blocks: Vec<String> = Vec::new();
    let description = layout.description.trim();
    if !description.is_empty() {
        blocks.push(description.to_string());
    }
    let mut old_groups: Vec<Option<GroupLayout>> = layout.groups.into_iter().map(Some).collect();
    for group in groups {
        let old = take_group(&mut old_groups, &group.label);
        let mut lines: Vec<String> = Vec::new();
        let label = group.label.trim();
        if !label.is_empty() {
            lines.push(format!("### {label}"));
            lines.push(String::new());
        }
        if let Some(old) = &old {
            let lead = old.lead.trim();
            if !lead.is_empty() {
                lines.extend(lead.lines().map(str::to_string));
                lines.push(String::new());
            }
        }
        let tails = item_tails(old.as_ref().map_or(&[], |old| &old.items), &group.items);
        for (episode, tail) in group.items.iter().zip(&tails) {
            lines.push(render_item(episode, tracking));
            let tail = tail.trim_end();
            if !tail.is_empty() {
                lines.extend(tail.lines().map(str::to_string));
            }
        }
        let block = lines.join("\n").trim_end().to_string();
        if !block.is_empty() {
            blocks.push(block);
        }
    }
    let trailing = layout.trailing.trim();
    if !trailing.is_empty() {
        blocks.push(trailing.to_string());
    }
    blocks.join("\n\n")
}

/// Takes (at most once) the old layout group matching `label`, so duplicate labels
/// pair up in order.
fn take_group(groups: &mut [Option<GroupLayout>], label: &str) -> Option<GroupLayout> {
    let index = groups
        .iter()
        .position(|group| group.as_ref().is_some_and(|g| same_label(&g.label, label)))?;
    groups[index].take()
}

/// Pairs each episode being rendered with the tail captured for the same item in
/// the old layout: a unique key match wins, then position — so toggles and merges
/// (which keep existing order) re-attach every tail, and newly appended items get
/// none.
fn item_tails(old: &[ItemLayout], new: &[Episode]) -> Vec<String> {
    let mut tails: Vec<Option<String>> = vec![None; new.len()];
    let mut used = vec![false; old.len()];
    for (index, episode) in new.iter().enumerate() {
        let key = episode.key.trim();
        if key.is_empty() {
            continue;
        }
        let mut matching = old
            .iter()
            .enumerate()
            .filter(|(_, item)| item.episode.key.trim() == key);
        let first = matching.next();
        if matching.next().is_some() {
            // Duplicate keys — fall through to the positional pass.
            continue;
        }
        if let Some((position, item)) = first {
            if !used[position] {
                used[position] = true;
                tails[index] = Some(item.tail.clone());
            }
        }
    }
    for (index, episode) in new.iter().enumerate() {
        if tails[index].is_some() {
            continue;
        }
        let Some(item) = old.get(index) else {
            continue;
        };
        if used[index] {
            continue;
        }
        let old_key = item.episode.key.trim();
        let new_key = episode.key.trim();
        if !old_key.is_empty() && !new_key.is_empty() && old_key != new_key {
            continue;
        }
        used[index] = true;
        tails[index] = Some(item.tail.clone());
    }
    tails.into_iter().map(Option::unwrap_or_default).collect()
}

fn render_item(episode: &Episode, tracking: EpisodeTracking) -> String {
    let checkbox = match tracking {
        EpisodeTracking::Checklist if episode.watched => "[x] ",
        EpisodeTracking::Checklist => "[ ] ",
        EpisodeTracking::None => "",
    };
    let key = episode.key.trim();
    let title = episode.title.trim();
    let label = match (key.is_empty(), title.is_empty()) {
        (true, _) => title.to_string(),
        (false, true) => key.to_string(),
        (false, false) => format!("{key} · {title}"),
    };
    // Obsidian Tasks date suffixes, in the canonical order: due (`📅`, the
    // air/release date) then done (`✅`, the completion date).
    format!(
        "- {checkbox}{label}{}{}",
        emoji_suffix("📅", episode.date.as_deref()),
        emoji_suffix("✅", episode.done.as_deref()),
    )
}

fn emoji_suffix(emoji: &str, date: Option<&str>) -> String {
    match date.map(str::trim) {
        Some(date) if !date.is_empty() => format!(" {emoji} {date}"),
        _ => String::new(),
    }
}

/// Renders `groups` back into `body`'s episodes section (replacing only that
/// section; appending the heading if absent). The body stays the source of truth:
/// the section's existing layout is re-parsed so the surrounding prose, group
/// leads, and per-item nested lines are re-emitted rather than dropped.
pub fn apply_episodes(body: &str, section: &BodySection, groups: &[EpisodeGroup]) -> String {
    let layout = parse_layout(body, &section.heading);
    let rendered = render_section(layout, groups, resolved_tracking(section));
    splice_section(body, &section.heading, &rendered)
}

/// Checks/unchecks a single episode in `body`, stamping `today` as its `✅`
/// completion date when checked and clearing it when unchecked. The item is
/// located within its group by `key` when that uniquely identifies one, else by
/// `index` (its position in the group) — so keyless or duplicate-keyed items
/// still resolve. Returns the rewritten body, or `None` if the item isn't found.
/// Only this one item changes — the rest of the section is re-rendered verbatim.
pub fn set_episode_watched(
    body: &str,
    section: &BodySection,
    group: &str,
    key: &str,
    index: usize,
    watched: bool,
    today: &str,
) -> Option<String> {
    let mut episodes = parse_episodes(body, section);
    let target = locate_episode(&mut episodes.groups, group, key, index)?;
    target.watched = watched;
    target.done = watched.then(|| today.to_string());
    Some(apply_episodes(body, section, &episodes.groups))
}

/// Resolves the episode to toggle: a unique `key` match within the group wins;
/// otherwise the item at `index`. `None` if the group or index doesn't exist.
fn locate_episode<'a>(
    groups: &'a mut [EpisodeGroup],
    group: &str,
    key: &str,
    index: usize,
) -> Option<&'a mut Episode> {
    let group = groups
        .iter_mut()
        .find(|candidate| same_label(&candidate.label, group))?;
    let key = key.trim();
    if !key.is_empty() {
        let mut unique: Option<usize> = None;
        let mut ambiguous = false;
        for (position, item) in group.items.iter().enumerate() {
            if item.key.trim() == key {
                if unique.is_some() {
                    ambiguous = true;
                    break;
                }
                unique = Some(position);
            }
        }
        if let (Some(position), false) = (unique, ambiguous) {
            return group.items.get_mut(position);
        }
    }
    group.items.get_mut(index)
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
                            // Dates follow the same rule: fill a missing one, and
                            // refresh it on overwrite when the provider has one.
                            if is_blank(&existing_item.date) || (overwrite && !is_blank(&item.date))
                            {
                                existing_item.date = item.date.clone();
                            }
                        }
                        None => group.items.push(Episode {
                            key: item.key.clone(),
                            title: item.title.clone(),
                            watched: false,
                            date: item.date.clone(),
                            done: item.done.clone(),
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
                        date: item.date.clone(),
                        done: item.done.clone(),
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

fn is_blank(value: &Option<String>) -> bool {
    value.as_deref().map(str::trim).unwrap_or("").is_empty()
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
            date: None,
            done: None,
        }
    }

    fn unwatched(key: &str, title: &str) -> Episode {
        episode(key, title, false)
    }

    fn dated(key: &str, title: &str, date: &str) -> Episode {
        Episode {
            date: Some(date.to_string()),
            ..episode(key, title, false)
        }
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
    fn nested_bullets_are_item_tails_not_episodes() {
        // A sub-bullet indented under an episode is the user's note, not an episode:
        // it must not count, and both parse paths must agree.
        let body = "## Episodes\n- [ ] 1 · Pilot\n  - [ ] extra sub-task\n- [ ] 2 · Dawn\n";
        let parsed = parse_episodes(body, &section());
        assert_eq!(parsed.total, 2);
        assert_eq!(parsed.groups[0].items[1].title, "Dawn");
        let progress = episode_progress(body, &section());
        assert_eq!(progress.total, 2);
    }

    #[test]
    fn toggle_round_trips_prose_and_nested_bullets_byte_for_byte() {
        // The full layout: description prose, a nested note under an item, and
        // trailing prose. Toggling one checkbox must touch only that item's line.
        let body = "## Episodes\n\nWatch order notes.\n\n- [ ] 1 · Pilot\n  - my nested note\n- [ ] 2 · Dawn\n\nMore after the list.\n";
        let checked =
            set_episode_watched(body, &section(), "", "1", 0, true, "2024-01-17").unwrap();
        assert_eq!(
            checked,
            "## Episodes\n\nWatch order notes.\n\n- [x] 1 · Pilot ✅ 2024-01-17\n  - my nested note\n- [ ] 2 · Dawn\n\nMore after the list.\n"
        );
        // The nested note is still not an episode after the rewrite.
        assert_eq!(parse_episodes(&checked, &section()).total, 2);

        // Unchecking restores the original body exactly.
        let unchecked =
            set_episode_watched(&checked, &section(), "", "1", 0, false, "2024-02-01").unwrap();
        assert_eq!(unchecked, body);
    }

    #[test]
    fn apply_with_unchanged_groups_is_the_identity() {
        let body = "## Episodes\n\nIntro.\n\n### Season 1\n\nGroup notes.\n\n- [x] 1 · Pilot\n  - note under pilot\n- [ ] 2 · Dawn\n\nOutro.\n";
        let parsed = parse_episodes(body, &section());
        assert_eq!(parsed.description, "Intro.");
        assert_eq!(parsed.trailing, "Outro.");
        let next = apply_episodes(body, &section(), &parsed.groups);
        assert_eq!(next, body);
    }

    #[test]
    fn merge_write_keeps_leads_tails_and_prose_around_appended_items() {
        // The import path: merge provider episodes into a section the user has
        // annotated. Every hand-written line must survive the rewrite.
        let body = "## Episodes\n\nIntro.\n\n### Season 1\n\nGroup notes.\n\n- [x] 1 · Pilot\n  - note under pilot\n\nOutro.\n";
        let existing = parse_episodes(body, &section());
        let incoming = vec![EpisodeGroup {
            label: "Season 1".to_string(),
            items: vec![unwatched("1", "Pilot"), unwatched("2", "Dawn")],
        }];
        let merged = merge_episodes(&existing, &incoming, false);
        let next = apply_episodes(body, &section(), &merged);
        assert_eq!(
            next,
            "## Episodes\n\nIntro.\n\n### Season 1\n\nGroup notes.\n\n- [x] 1 · Pilot\n  - note under pilot\n- [ ] 2 · Dawn\n\nOutro.\n"
        );
    }

    #[test]
    fn item_lines_inside_code_fences_are_content_not_episodes() {
        let body = "## Episodes\n\n- [ ] 1 · A\n\n```\n- [ ] fake item\n### fake group\n```\n";
        let parsed = parse_episodes(body, &section());
        assert_eq!(parsed.total, 1);
        assert_eq!(parsed.groups.len(), 1);
        assert_eq!(episode_progress(body, &section()).total, 1);
        // A toggle rewrite keeps the fenced block intact.
        let checked =
            set_episode_watched(body, &section(), "", "1", 0, true, "2024-01-17").unwrap();
        assert_eq!(
            checked,
            "## Episodes\n\n- [x] 1 · A ✅ 2024-01-17\n\n```\n- [ ] fake item\n### fake group\n```\n"
        );
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
    fn renders_and_round_trips_the_date_emoji() {
        let groups = vec![EpisodeGroup {
            label: String::new(),
            items: vec![dated("1", "Pilot", "2024-01-15"), unwatched("2", "No date")],
        }];
        let body = apply_episodes("## Episodes\n", &section(), &groups);
        assert!(body.contains("- [ ] 1 · Pilot 📅 2024-01-15"));
        // The undated item has no emoji appended.
        assert!(body.contains("- [ ] 2 · No date\n"));

        // Re-parsing recovers the date as structured data, not as part of the title.
        let parsed = parse_episodes(&body, &section());
        assert_eq!(parsed.groups[0].items[0].title, "Pilot");
        assert_eq!(
            parsed.groups[0].items[0].date.as_deref(),
            Some("2024-01-15")
        );
        assert_eq!(parsed.groups[0].items[1].date, None);
    }

    #[test]
    fn checking_stamps_a_done_date_and_unchecking_clears_it() {
        let body = "## Episodes\n- [ ] 1 · Pilot 📅 2024-01-15\n";
        // Check it: today's done date is appended after the air date.
        let checked =
            set_episode_watched(body, &section(), "", "1", 0, true, "2024-01-17").unwrap();
        assert!(checked.contains("- [x] 1 · Pilot 📅 2024-01-15 ✅ 2024-01-17"));

        // Re-parsing keeps both dates structured (not in the title).
        let parsed = parse_episodes(&checked, &section());
        assert_eq!(parsed.groups[0].items[0].title, "Pilot");
        assert_eq!(
            parsed.groups[0].items[0].date.as_deref(),
            Some("2024-01-15")
        );
        assert_eq!(
            parsed.groups[0].items[0].done.as_deref(),
            Some("2024-01-17")
        );

        // Unchecking removes the ✅ but keeps the 📅 air date.
        let unchecked =
            set_episode_watched(&checked, &section(), "", "1", 0, false, "2024-02-01").unwrap();
        assert!(unchecked.contains("- [ ] 1 · Pilot 📅 2024-01-15\n"));
        assert!(!unchecked.contains("✅"));

        // A missing episode (bad index, no key match) is a no-op signal.
        assert!(set_episode_watched(body, &section(), "", "999", 9, true, "2024-01-17").is_none());
    }

    #[test]
    fn episode_calendar_dates_flatten_air_and_completion_dates() {
        let body = "## Episodes\n### Season 1\n- [x] 1 · Pilot 📅 2024-01-15 ✅ 2024-01-17\n- [ ] 2 · Dawn 📅 2024-01-22\n- [ ] 3 · Undated\n";
        let dates = episode_calendar_dates(body, &section());
        // Pilot → scheduled + completed; Dawn → scheduled only; Undated → nothing.
        assert_eq!(dates.len(), 3);
        assert_eq!(dates[0].title, "Pilot");
        assert_eq!(dates[0].date, "2024-01-15");
        assert_eq!(dates[0].role, EpisodeDateRole::Scheduled);
        assert_eq!(dates[1].date, "2024-01-17");
        assert_eq!(dates[1].role, EpisodeDateRole::Completed);
        assert_eq!(dates[2].title, "Dawn");
        assert_eq!(dates[2].role, EpisodeDateRole::Scheduled);
    }

    #[test]
    fn toggle_falls_back_to_index_for_keyless_or_duplicate_items() {
        // Two keyless items with the same title — only the position disambiguates.
        let body = "## Episodes\n- [ ] Intro\n- [ ] Intro\n";
        let checked = set_episode_watched(body, &section(), "", "", 1, true, "2024-01-17").unwrap();
        let parsed = parse_episodes(&checked, &section());
        assert!(!parsed.groups[0].items[0].watched); // first Intro untouched
        assert!(parsed.groups[0].items[1].watched); // second Intro checked by index
        assert_eq!(
            parsed.groups[0].items[1].done.as_deref(),
            Some("2024-01-17")
        );

        // Duplicate keys also fall back to index rather than toggling the first.
        let dup = "## Episodes\n- [ ] 1 · A\n- [ ] 1 · B\n";
        let toggled = set_episode_watched(dup, &section(), "", "1", 1, true, "2024-01-17").unwrap();
        let parsed = parse_episodes(&toggled, &section());
        assert!(!parsed.groups[0].items[0].watched);
        assert!(parsed.groups[0].items[1].watched);
    }

    #[test]
    fn sync_overwrite_keeps_the_done_date_while_replacing_text_and_air_date() {
        let existing = entity_episodes(vec![EpisodeGroup {
            label: String::new(),
            items: vec![Episode {
                date: Some("2024-01-10".to_string()),
                done: Some("2024-01-17".to_string()),
                ..episode("1", "Old title", true)
            }],
        }]);
        let incoming = vec![EpisodeGroup {
            label: String::new(),
            items: vec![dated("1", "New title", "2024-01-15")],
        }];
        let merged = merge_episodes(&existing, &incoming, true);
        let item = &merged[0].items[0];
        assert_eq!(item.title, "New title"); // text overwritten
        assert_eq!(item.date.as_deref(), Some("2024-01-15")); // air date overwritten
        assert_eq!(item.done.as_deref(), Some("2024-01-17")); // completion date kept
        assert!(item.watched); // watched kept
    }

    #[test]
    fn merge_fills_missing_dates_and_overwrites_on_request() {
        let existing = entity_episodes(vec![EpisodeGroup {
            label: String::new(),
            items: vec![
                episode("1", "Pilot", true),      // no date yet, watched
                dated("2", "Dawn", "2024-01-22"), // already dated
            ],
        }]);
        let incoming = vec![EpisodeGroup {
            label: String::new(),
            items: vec![
                dated("1", "Pilot", "2024-01-15"),
                dated("2", "Dawn", "2099-12-31"),
            ],
        }];

        // Non-overwrite: a missing date is filled, an existing one is left alone.
        let merged = merge_episodes(&existing, &incoming, false);
        assert_eq!(merged[0].items[0].date.as_deref(), Some("2024-01-15"));
        assert!(merged[0].items[0].watched);
        assert_eq!(merged[0].items[1].date.as_deref(), Some("2024-01-22"));

        // Overwrite: an existing date is refreshed from the provider.
        let merged = merge_episodes(&existing, &incoming, true);
        assert_eq!(merged[0].items[1].date.as_deref(), Some("2099-12-31"));
    }

    #[test]
    fn progress_matches_full_parse_cheaply() {
        let body = "## Episodes\n### S1\n- [x] 1\n- [ ] 2\n- [x] 3\n";
        let progress = episode_progress(body, &section());
        assert_eq!(progress.total, 3);
        assert_eq!(progress.watched, 2);
    }
}
