// Pure rule-model helpers, deliberately free of Lingui macro imports so
// non-UI modules (settings-model and its Vitest suite) can depend on them
// without pulling the macro runtime into a context that can't compile it.

import type { SmartFilterGroup, SmartFilterRule } from "@/types/api";

/// Whether a rule is filled in enough to evaluate. Rows the user is still
/// completing (an empty tag/value picker, a blank date) are kept in the draft
/// but excluded from preview and save requests — half-built criteria are an
/// editing state, not an error.
export function isCompleteRule(rule: SmartFilterRule): boolean {
  const hasValues = (rule.values ?? []).some((value) => value.trim() !== "");
  switch (rule.kind) {
    case "contains":
    case "startsWith":
    case "endsWith":
    case "hasTag":
    case "linksTo":
    case "inFolder":
      return hasValues;
    case "compare":
      if (rule.relative) return true;
      if (rule.number !== undefined && rule.number !== null) return true;
      if (rule.boolean !== undefined && rule.boolean !== null) return true;
      if (rule.date !== undefined && rule.date !== null) return rule.date.trim() !== "";
      if (rule.value !== undefined && rule.value !== null) return rule.value.trim() !== "";
      return false;
    default:
      return true;
  }
}

/// The savable/previewable projection of a criteria group: incomplete rules
/// dropped, emptied subgroups removed.
export function pruneIncompleteRules(group: SmartFilterGroup): SmartFilterGroup {
  return {
    ...group,
    rules: (group.rules ?? []).filter(isCompleteRule),
    groups: (group.groups ?? [])
      .map((subgroup) => ({
        ...subgroup,
        rules: (subgroup.rules ?? []).filter(isCompleteRule),
      }))
      .filter((subgroup) => (subgroup.rules?.length ?? 0) > 0),
  };
}
