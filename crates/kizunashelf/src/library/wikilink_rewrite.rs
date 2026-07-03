//! Rewriting `[[wikilink]]` targets in raw Markdown when an entity is renamed.
//!
//! A wikilink resolves against the target file's *basename*, so renaming an
//! entity leaves every inbound `[[OldName]]` dangling until it is repointed.
//! We rewrite the **raw** file text rather than parsing and re-serializing the
//! Markdown document: a backlink file's frontmatter key order, formatting, and
//! comments must survive untouched, and only the matched link's basename should
//! change. Any `folder/` prefix, `#heading`, and `|alias` are preserved.

use std::collections::HashSet;
use unicode_normalization::UnicodeNormalization;

use super::frontmatter::wikilink_regex;
use crate::daily_notes::normalize_wikilink_target;

/// Case- and composition-insensitive form of a *full* wikilink target, keeping
/// any `folder/` prefix so `[[Anime/Beta]]` and `[[Manga/Beta]]` stay distinct.
/// (Unlike [`normalize_wikilink_target`], which reduces a target to its last
/// path segment for basename resolution.)
pub(crate) fn normalize_full_target(target: &str) -> String {
    target.trim().to_lowercase().nfc().collect()
}

/// Repoints every `[[...]]` whose *full* normalized target is in `old_targets`
/// so its basename becomes `new_basename`. For backlink files, `old_targets` is
/// the exact set of target texts the relation cache resolved to the renamed
/// entity — matching on the full target (prefix included) is what keeps a
/// sibling `[[Manga/Beta]]` pointing at a different entity untouched.
///
/// Returns the rewritten text and the number of links changed.
pub(crate) fn rewrite_backlink_wikilinks(
    text: &str,
    old_targets: &HashSet<String>,
    new_basename: &str,
) -> (String, usize) {
    rewrite_wikilinks(text, new_basename, |full, _basename| {
        old_targets.contains(full)
    })
}

/// Repoints every `[[...]]` whose *basename* matches `old_basename`. Used only on
/// the renamed file's own content, where any link to the old basename is a
/// self-reference (a namesake in another folder inside the very file being
/// renamed is implausible), so a basename match is both safe and able to catch
/// body self-links — which the relation graph deliberately drops.
pub(crate) fn rewrite_self_wikilinks(
    text: &str,
    old_basename: &str,
    new_basename: &str,
) -> (String, usize) {
    let old = normalize_wikilink_target(old_basename);
    rewrite_wikilinks(text, new_basename, |_full, basename| basename == old)
}

/// The shared scan: run `wikilink_regex` (the same matcher relation extraction
/// uses) over the text and rebuild every link whose **raw trimmed target** (as
/// written, `folder/` prefix included) the `matches` predicate accepts. Callers
/// that key off a normalized form go through [`rewrite_wikilinks`]; callers that
/// resolve the target against the entity set (e.g. list files, where a basename
/// is ambiguous) match on the raw target directly.
///
/// Returns the rewritten text and the number of links changed.
pub(crate) fn rewrite_wikilinks_matching(
    text: &str,
    new_basename: &str,
    matches: impl Fn(&str) -> bool,
) -> (String, usize) {
    let mut count = 0usize;
    let rewritten = wikilink_regex().replace_all(text, |captures: &regex::Captures| {
        let whole = &captures[0];
        let target = captures[1].trim();
        if !matches(target) {
            return whole.to_string();
        }
        count += 1;
        rebuild_wikilink(whole, new_basename)
    });
    (rewritten.into_owned(), count)
}

/// [`rewrite_wikilinks_matching`] with the predicate handed the target as a
/// normalized full target and a normalized basename, so basename/full-target
/// callers can key off whichever they need.
fn rewrite_wikilinks(
    text: &str,
    new_basename: &str,
    matches: impl Fn(&str, &str) -> bool,
) -> (String, usize) {
    rewrite_wikilinks_matching(text, new_basename, |target| {
        matches(
            &normalize_full_target(target),
            &normalize_wikilink_target(target),
        )
    })
}

/// Rebuilds a matched `[[target#heading|alias]]`, swapping only the target's
/// last path segment for `new_basename` and preserving the prefix, heading, and
/// alias verbatim.
fn rebuild_wikilink(whole: &str, new_basename: &str) -> String {
    let inner = whole
        .strip_prefix("[[")
        .and_then(|rest| rest.strip_suffix("]]"))
        .unwrap_or(whole);
    let (link, alias) = match inner.split_once('|') {
        Some((link, alias)) => (link, Some(alias)),
        None => (inner, None),
    };
    let (target, heading) = match link.split_once('#') {
        Some((target, heading)) => (target, Some(heading)),
        None => (link, None),
    };
    let new_target = match target.trim().rsplit_once('/') {
        Some((prefix, _)) => format!("{prefix}/{new_basename}"),
        None => new_basename.to_string(),
    };
    let mut rebuilt = format!("[[{new_target}");
    if let Some(heading) = heading {
        rebuilt.push('#');
        rebuilt.push_str(heading);
    }
    if let Some(alias) = alias {
        rebuilt.push('|');
        rebuilt.push_str(alias);
    }
    rebuilt.push_str("]]");
    rebuilt
}

#[cfg(test)]
mod tests {
    use super::*;

    fn targets(items: &[&str]) -> HashSet<String> {
        items
            .iter()
            .map(|item| normalize_full_target(item))
            .collect()
    }

    #[test]
    fn rewrites_a_plain_link() {
        let (out, changed) =
            rewrite_backlink_wikilinks("see [[Beta]] here", &targets(&["Beta"]), "Beta2");
        assert_eq!(out, "see [[Beta2]] here");
        assert_eq!(changed, 1);
    }

    #[test]
    fn preserves_prefix_heading_and_alias() {
        let (out, changed) = rewrite_backlink_wikilinks(
            "[[Anime/Beta#Arc 1|the beta]]",
            &targets(&["Anime/Beta"]),
            "Beta2",
        );
        assert_eq!(out, "[[Anime/Beta2#Arc 1|the beta]]");
        assert_eq!(changed, 1);
    }

    #[test]
    fn matches_case_and_composition_insensitively() {
        let (out, changed) = rewrite_backlink_wikilinks("[[bEtA]]", &targets(&["Beta"]), "Beta2");
        assert_eq!(out, "[[Beta2]]");
        assert_eq!(changed, 1);
    }

    #[test]
    fn leaves_a_distinct_prefix_alone() {
        // Manga/Beta resolves to a different entity than the renamed Anime/Beta,
        // so only the Anime/Beta link is repointed.
        let (out, changed) = rewrite_backlink_wikilinks(
            "[[Anime/Beta]] and [[Manga/Beta]]",
            &targets(&["Anime/Beta"]),
            "Beta2",
        );
        assert_eq!(out, "[[Anime/Beta2]] and [[Manga/Beta]]");
        assert_eq!(changed, 1);
    }

    #[test]
    fn never_touches_bare_text() {
        let (out, changed) =
            rewrite_backlink_wikilinks("Beta without brackets", &targets(&["Beta"]), "Beta2");
        assert_eq!(out, "Beta without brackets");
        assert_eq!(changed, 0);
    }

    #[test]
    fn self_rewrite_matches_by_basename_including_path_form() {
        let (out, changed) = rewrite_self_wikilinks("[[Beta]] and [[Anime/Beta]]", "Beta", "Beta2");
        assert_eq!(out, "[[Beta2]] and [[Anime/Beta2]]");
        assert_eq!(changed, 2);
    }

    #[test]
    fn rewrites_every_matching_occurrence() {
        let (out, changed) =
            rewrite_backlink_wikilinks("[[Beta]] [[Beta]] [[Gamma]]", &targets(&["Beta"]), "Beta2");
        assert_eq!(out, "[[Beta2]] [[Beta2]] [[Gamma]]");
        assert_eq!(changed, 2);
    }

    #[test]
    fn matching_predicate_sees_the_raw_target() {
        // The generic form hands the predicate the target verbatim (prefix
        // included), so a caller can accept `Anime/Beta` while rejecting the
        // namesake `Manga/Beta` — the disambiguation list-file rewrites rely on.
        let (out, changed) =
            rewrite_wikilinks_matching("[[Anime/Beta]] and [[Manga/Beta]]", "Beta2", |target| {
                target == "Anime/Beta"
            });
        assert_eq!(out, "[[Anime/Beta2]] and [[Manga/Beta]]");
        assert_eq!(changed, 1);
    }
}
