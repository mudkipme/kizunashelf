import { useMemo } from "react";
import { useLingui } from "@lingui/react/macro";

import { Select } from "@/components/ui/select";
import { defaultSort } from "@/lib/constants";
import { fieldDisplayLabel } from "@/lib/type-config";
import type { SmartSortSpec, TypeConfig } from "@/types/api";

/// Bases sorts by any property, so the offer is defined by exclusion: a field
/// is sortable unless its value has no single orderable key. List-valued fields
/// (and relations, which may hold a list) sort as absent for every entity;
/// id/image/external-ref fields hold opaque strings nobody browses by; and a
/// title field would only duplicate the built-in `file.name`, which already
/// sorts by the title resolved in the viewer's language. Schema-driven
/// throughout — a field sorts because of its declared type, never its name.
const unsortableFieldTypes = [
  "id",
  "title",
  "image",
  "imageList",
  "enumList",
  "textList",
  "relation",
  "externalRef",
];

/// The sort properties offered for one type: the two `file.*` built-ins plus
/// its orderable schema fields. `current` is kept in the list even when the
/// schema doesn't declare it, so a hand-written sort key stays visible instead
/// of silently switching to the title.
///
/// `typeConfig: undefined` is an unscoped list, and then only the built-ins are
/// offered — the same rule the criteria builder follows (`ruleFieldMetas`).
/// A field key doesn't mean one thing across types, and its label wouldn't
/// either: whichever type declared it first would name the menu entry.
function useSortOptions(typeConfig: TypeConfig | undefined, current?: string) {
  const { t } = useLingui();
  return useMemo(() => {
    const options: { value: string; label: string }[] = [
      { value: defaultSort, label: t`Title` },
      { value: "file.mtime", label: t`Update time` },
    ];
    const seen = new Set<string>();
    for (const field of typeConfig?.fields ?? []) {
      if (seen.has(field.field)) continue;
      if (unsortableFieldTypes.includes(field.fieldType)) continue;
      seen.add(field.field);
      options.push({ value: `note.${field.field}`, label: fieldDisplayLabel(field) });
    }
    if (current && !options.some((option) => option.value === current)) {
      options.push({ value: current, label: current });
    }
    return options;
  }, [typeConfig, current, t]);
}

/// Paired property + direction selects. Both pages render the same control;
/// only where the value lives (a draft view, or the browse URL) differs.
///
/// `sort: undefined` is "no sort key", which the server reads as relevance
/// while a search is active and as the title order otherwise. It's offered as a
/// choice only when `unsortedLabel` names it, and carries no direction — a
/// relevance ranking is always best-match-first.
export function SortPicker({
  typeConfig,
  sort,
  unsortedLabel,
  disabled = false,
  className,
  onChange,
}: {
  typeConfig: TypeConfig | undefined;
  sort: SmartSortSpec | undefined;
  unsortedLabel?: string;
  disabled?: boolean;
  className?: string;
  onChange: (sort: SmartSortSpec | undefined) => void;
}) {
  const { t } = useLingui();
  const direction = sort?.direction === "desc" ? "desc" : "asc";
  const options = useSortOptions(typeConfig, sort?.property);
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
