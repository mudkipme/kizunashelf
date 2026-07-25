import { useMemo } from "react";
import { useLingui } from "@lingui/react/macro";

import { Select } from "@/components/ui/select";
import { defaultSort } from "@/lib/constants";
import { fieldDisplayLabel } from "@/lib/type-config";
import type { SmartSortSpec, TypeConfig } from "@/types/api";

/// Which schema field types are meaningfully orderable. Deliberately schema-
/// driven — a field sorts because of its declared type, never its name.
const sortableFieldTypes = ["date", "rating", "progress", "totalProgress"];

/// The sort properties offered for a set of types: the two `file.*` built-ins
/// plus every orderable schema field across the scope. `current` is kept in the
/// list even when the schema doesn't declare it, so a hand-written sort key
/// stays visible instead of silently switching to the title.
function useSortOptions(typeConfigs: TypeConfig[], current?: string) {
  const { t } = useLingui();
  return useMemo(() => {
    const options: { value: string; label: string }[] = [
      { value: defaultSort, label: t`Title` },
      { value: "file.mtime", label: t`Update time` },
    ];
    const seen = new Set<string>();
    for (const typeConfig of typeConfigs) {
      for (const field of typeConfig.fields ?? []) {
        if (seen.has(field.field)) continue;
        if (!sortableFieldTypes.includes(field.fieldType)) continue;
        seen.add(field.field);
        options.push({ value: `note.${field.field}`, label: fieldDisplayLabel(field) });
      }
    }
    if (current && !options.some((option) => option.value === current)) {
      options.push({ value: current, label: current });
    }
    return options;
  }, [typeConfigs, current, t]);
}

/// Paired property + direction selects. Both pages render the same control;
/// only where the value lives (a draft view, or the browse URL) differs.
///
/// `sort: undefined` is "no sort key", which the server reads as relevance
/// while a search is active and as the title order otherwise. It's offered as a
/// choice only when `unsortedLabel` names it, and carries no direction — a
/// relevance ranking is always best-match-first.
export function SortPicker({
  typeConfigs,
  sort,
  unsortedLabel,
  disabled = false,
  className,
  onChange,
}: {
  typeConfigs: TypeConfig[];
  sort: SmartSortSpec | undefined;
  unsortedLabel?: string;
  disabled?: boolean;
  className?: string;
  onChange: (sort: SmartSortSpec | undefined) => void;
}) {
  const { t } = useLingui();
  const direction = sort?.direction === "desc" ? "desc" : "asc";
  const options = useSortOptions(typeConfigs, sort?.property);
  const value = sort ? sort.property : unsortedLabel ? "" : defaultSort;

  return (
    <>
      <Select
        value={value}
        disabled={disabled}
        aria-label={t`Sort`}
        className={className ?? "w-fit min-w-0"}
        onChange={(event) =>
          onChange(
            event.target.value ? { property: event.target.value, direction } : undefined,
          )
        }
      >
        {unsortedLabel ? <option value="">{unsortedLabel}</option> : null}
        {options.map((option) => (
          <option key={option.value} value={option.value}>
            {option.label}
          </option>
        ))}
      </Select>
      {sort ? (
        <Select
          value={direction}
          disabled={disabled}
          aria-label={t`Direction`}
          className={className ?? "w-fit min-w-0"}
          onChange={(event) =>
            onChange({
              property: sort.property,
              direction: event.target.value === "desc" ? "desc" : "asc",
            })
          }
        >
          <option value="asc">{t`Ascending`}</option>
          <option value="desc">{t`Descending`}</option>
        </Select>
      ) : null}
    </>
  );
}
