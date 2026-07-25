import type { SmartFilterGroup } from "@/types/api";

/// The URL parameter carrying a criteria group. Browsing the library *is* an
/// unsaved smart list, so its criteria have to survive a reload, a shared link,
/// and the back button — they ride in the URL as JSON rather than in a store.
export const criteriaParam = "criteria";

export const emptyCriteria: SmartFilterGroup = { conjunction: "all", rules: [], groups: [] };

/// Whether a group constrains anything at all. An empty group is left out of
/// the URL entirely, so `/library?type=anime` stays the clean "everything"
/// link that the sidebar and every external reference use.
export function hasCriteria(group: SmartFilterGroup | undefined): boolean {
  if (!group) return false;
  return (group.rules?.length ?? 0) > 0 || (group.groups?.length ?? 0) > 0;
}

/// Serializes a group for the URL, or `undefined` when it constrains nothing.
export function encodeCriteria(group: SmartFilterGroup | undefined): string | undefined {
  if (!hasCriteria(group)) return undefined;
  return JSON.stringify(group);
}

/// Reads a group back out of the URL. Anything malformed — a hand-edited link,
/// a param from an older build — degrades to "no criteria" rather than
/// breaking the page; the criteria are a lens over the library, not data.
export function decodeCriteria(value: string | null | undefined): SmartFilterGroup {
  if (!value) return emptyCriteria;
  try {
    const parsed = JSON.parse(value) as unknown;
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) return emptyCriteria;
    const group = parsed as SmartFilterGroup;
    return {
      conjunction:
        group.conjunction === "any" || group.conjunction === "none" ? group.conjunction : "all",
      rules: Array.isArray(group.rules) ? group.rules : [],
      groups: Array.isArray(group.groups) ? group.groups : [],
    };
  } catch {
    return emptyCriteria;
  }
}

/// How many editable pieces a group holds — the badge on the Filter button.
export function criteriaRuleCount(group: SmartFilterGroup): number {
  return (
    (group.rules?.length ?? 0) +
    (group.groups ?? []).reduce((sum, subgroup) => sum + (subgroup.rules?.length ?? 0), 0)
  );
}
