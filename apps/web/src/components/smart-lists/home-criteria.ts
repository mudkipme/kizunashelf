import type { SmartFilterGroup } from "@/types/api";

/// How many editable pieces a criteria group holds — the settings row badge.
export function criteriaRuleCount(criteria: SmartFilterGroup): number {
  return (
    (criteria.rules?.length ?? 0) +
    (criteria.groups ?? []).reduce((sum, group) => sum + (group.rules?.length ?? 0), 0)
  );
}
