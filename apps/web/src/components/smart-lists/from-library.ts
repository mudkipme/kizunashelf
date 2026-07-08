import type { FieldFilter } from "@/components/assets/asset-toolbar";
import { allTypes } from "@/lib/constants";
import type {
  SmartFilterGroup,
  SmartFilterRule,
  SmartFilterSubgroup,
  SmartListView,
  SmartSortSpec,
  TypeConfig,
} from "@/types/api";

/// Converts the library page's current browse state (type, active filters,
/// sort) into a smart-list definition — the bridge from ad-hoc browsing to a
/// saved dynamic list. Multi-value enum/relation filters become an "any of"
/// subgroup, since one rule holds one comparison.
export function smartListFromLibraryState({
  selectedType,
  typeConfig,
  tagsField,
  activeFilters,
  sort,
  direction,
}: {
  selectedType: string;
  typeConfig: TypeConfig | undefined;
  tagsField: string;
  activeFilters: FieldFilter[];
  sort: string;
  direction: string;
}): { scope?: string; filters: SmartFilterGroup; views: SmartListView[] } {
  const rules: SmartFilterRule[] = [];
  const groups: SmartFilterSubgroup[] = [];

  for (const filter of activeFilters) {
    if (filter.values.length === 0) continue;
    if (filter.field === tagsField) {
      rules.push({ kind: "hasTag", negated: false, values: filter.values });
      continue;
    }
    if (filter.kind === "bool") {
      rules.push({
        kind: "compare",
        field: filter.field,
        op: "eq",
        boolean: filter.values[0] === "true",
        negated: false,
        values: [],
      });
      continue;
    }
    if (filter.kind === "relation") {
      const links = filter.values.map(
        (value): SmartFilterRule => ({ kind: "linksTo", negated: false, values: [value] }),
      );
      if (links.length === 1) rules.push(links[0]);
      else groups.push({ conjunction: "any", rules: links });
      continue;
    }
    // Enum vs enum-list matters: a list value matches by membership (one
    // `contains` rule), a scalar enum needs one equality per value.
    const fieldType = typeConfig?.fields?.find(
      (field) => field.field === filter.field,
    )?.fieldType;
    if (fieldType === "enumList" || fieldType === "textList") {
      rules.push({
        kind: "contains",
        field: filter.field,
        mode: "any",
        negated: false,
        values: filter.values,
      });
      continue;
    }
    const equals = filter.values.map(
      (value): SmartFilterRule => ({
        kind: "compare",
        field: filter.field,
        op: "eq",
        value,
        negated: false,
        values: [],
      }),
    );
    if (equals.length === 1) rules.push(equals[0]);
    else groups.push({ conjunction: "any", rules: equals });
  }

  const sortSpec: SmartSortSpec = {
    property: librarySortToProperty(sort),
    direction: direction === "desc" ? "desc" : "asc",
  };
  const views: SmartListView[] = [
    { name: "List", layout: "list", sort: [sortSpec] },
    { name: "Grid", layout: "grid", sort: [sortSpec] },
  ];

  return {
    scope: selectedType === allTypes ? undefined : selectedType,
    filters: { conjunction: "all", rules, groups },
    views,
  };
}

/// Library sort modes → Bases sort properties. `relevance` (query-bound) and
/// `relationCount` (no Bases equivalent) fall back to the title order.
function librarySortToProperty(sort: string): string {
  if (sort === "recentlyUpdated") return "file.mtime";
  const dateField = sort.startsWith("date:") ? sort.slice("date:".length) : undefined;
  if (dateField) return `note.${dateField}`;
  return "file.name";
}
