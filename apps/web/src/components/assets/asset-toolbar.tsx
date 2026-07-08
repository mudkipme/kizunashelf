import { useEffect, useRef, useState } from "react";
import { Trans, useLingui } from "@lingui/react/macro";
import { Grid2X2Icon, ListIcon, SlidersHorizontalIcon } from "lucide-react";

import { isAbortError } from "@/api/client";
import { Button } from "@/components/ui/button";
import { MultiValueCombobox } from "@/components/ui/multi-value-combobox";
import { Select } from "@/components/ui/select";
import { Separator } from "@/components/ui/separator";
import { allOptions, defaultDirection, defaultSort, relevanceSort } from "@/lib/constants";
import { cn } from "@/lib/utils";
import type { StatsResponse } from "@/types/api";

export type FieldFilterOption = {
  value: string;
  label?: string;
};

export type FieldFilter = {
  field: string;
  label: string;
  kind: "multi" | "bool" | "relation";
  options: FieldFilterOption[];
  values: string[];
  /** For `kind: "relation"`: loads suggestions dynamically as the user types. */
  loadOptions?: (query: string, signal: AbortSignal) => Promise<FieldFilterOption[]>;
};

type AssetToolbarProps = {
  className?: string;
  compact?: boolean;
  showLabel?: boolean;
  stats?: StatsResponse;
  sort: string;
  direction: string;
  view: string;
  fieldFilters?: FieldFilter[];
  /** Show a "Sort by relevance" option — only meaningful while searching. */
  showRelevanceSort?: boolean;
  dateFieldLabel?: (field: string) => string;
  onSortChange: (value: string) => void;
  onDirectionChange: (value: string) => void;
  onViewChange: (value: string) => void;
  onFieldFilterChange?: (field: string, values: string[]) => void;
};

export function AssetToolbar({
  className,
  compact = false,
  showLabel = true,
  stats,
  sort,
  direction,
  view,
  fieldFilters = [],
  showRelevanceSort = false,
  dateFieldLabel = (field) => field,
  onSortChange,
  onDirectionChange,
  onViewChange,
  onFieldFilterChange,
}: AssetToolbarProps) {
  const { t } = useLingui();
  const hasFieldFilters = fieldFilters.length > 0;

  return (
    <div
      className={cn(
        compact ? "grid grid-cols-2 gap-2" : "flex flex-wrap items-center gap-2 border-b px-3 py-2",
        className,
      )}
    >
      {showLabel ? (
        <div className="flex items-center gap-2 text-xs font-medium uppercase text-muted-foreground">
          <SlidersHorizontalIcon />
          <Trans>Filters</Trans>
        </div>
      ) : null}
      {fieldFilters.map((filter) => (
        <FieldFilterControl
          key={filter.field}
          filter={filter}
          compact={compact}
          onChange={(values) => onFieldFilterChange?.(filter.field, values)}
        />
      ))}
      {compact || !hasFieldFilters ? null : (
        <Separator orientation="vertical" className="mx-1 hidden h-6 sm:block" />
      )}
      <Select
        value={sort}
        onChange={(event) => onSortChange(event.target.value)}
        className={compact ? "min-w-0" : undefined}
        aria-label={t`Sort`}
      >
        {showRelevanceSort ? (
          <option value={relevanceSort}>{t`Sort by relevance`}</option>
        ) : null}
        <option value={defaultSort}>{t`Sort by title`}</option>
        <option value="recentlyUpdated">{t`Sort by update time`}</option>
        {stats?.dateFields.map((field) => (
          <option key={field} value={`date:${field}`}>
            {t`Sort by ${dateFieldLabel(field)}`}
          </option>
        ))}
        <option value="relationCount">{t`Sort by connections`}</option>
      </Select>
      <Select
        value={direction}
        onChange={(event) => onDirectionChange(event.target.value)}
        className={compact ? "min-w-0" : undefined}
        aria-label={t`Direction`}
      >
        <option value={defaultDirection}>{t`Ascending`}</option>
        <option value="desc">{t`Descending`}</option>
      </Select>
      <div className={cn(compact ? "col-span-2 grid grid-cols-2 gap-2" : "ml-auto flex items-center gap-1")}>
        <Button
          variant={view === "list" ? "secondary" : "ghost"}
          size="sm"
          className={compact ? "justify-center" : undefined}
          onClick={() => onViewChange("list")}
        >
          <ListIcon data-icon="inline-start" />
          <Trans>List</Trans>
        </Button>
        <Button
          variant={view === "grid" ? "secondary" : "ghost"}
          size="sm"
          className={compact ? "justify-center" : undefined}
          onClick={() => onViewChange("grid")}
        >
          <Grid2X2Icon data-icon="inline-start" />
          <Trans>Grid</Trans>
        </Button>
      </div>
    </div>
  );
}

function FieldFilterControl({
  filter,
  compact,
  onChange,
}: {
  filter: FieldFilter;
  compact: boolean;
  onChange: (values: string[]) => void;
}) {
  const { t } = useLingui();
  if (filter.kind === "bool") {
    const value = filter.values[0] ?? allOptions;
    return (
      <Select
        value={value}
        onChange={(event) => onChange(event.target.value === allOptions ? [] : [event.target.value])}
        className={compact ? "col-span-2 w-full min-w-0" : "w-40 shrink-0"}
        aria-label={filter.label}
      >
        <option value={allOptions}>{t`Any ${filter.label}`}</option>
        {filter.options.map((option) => (
          <option key={option.value} value={option.value}>
            {filter.label} {option.label ?? option.value}
          </option>
        ))}
      </Select>
    );
  }

  if (filter.kind === "relation") {
    return <RelationFilterControl filter={filter} compact={compact} onChange={onChange} />;
  }

  return (
    <MultiValueCombobox
      values={filter.values}
      options={filter.options}
      placeholder={t`Any ${filter.label}`}
      ariaLabel={filter.label}
      className={cn("min-h-8 px-2 py-1 text-xs", compact ? "col-span-2 w-full min-w-0" : "w-56 shrink-0")}
      onChange={onChange}
    />
  );
}

/**
 * Relation-field filter: a multi-select whose suggestions are loaded on demand
 * (entities of the field's relation type), mirroring the metadata editor's
 * relation input. Selected values are entity basenames; a label cache keeps the
 * chips showing titles even after the search query changes.
 */
function RelationFilterControl({
  filter,
  compact,
  onChange,
}: {
  filter: FieldFilter;
  compact: boolean;
  onChange: (values: string[]) => void;
}) {
  const { t } = useLingui();
  const [inputValue, setInputValue] = useState("");
  const [open, setOpen] = useState(false);
  const [options, setOptions] = useState<FieldFilterOption[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string>();
  const labels = useRef(new Map<string, string>());
  for (const option of options) {
    if (option.label) labels.current.set(option.value, option.label);
  }

  const { loadOptions } = filter;
  useEffect(() => {
    if (!open || !loadOptions) return;
    const controller = new AbortController();
    const timer = window.setTimeout(() => {
      setLoading(true);
      setError(undefined);
      loadOptions(inputValue.trim(), controller.signal)
        .then((items) => {
          if (!controller.signal.aborted) setOptions(items);
        })
        .catch((caught) => {
          if (isAbortError(caught) || controller.signal.aborted) return;
          setOptions([]);
          setError(caught instanceof Error ? caught.message : t`Could not load options.`);
        })
        .finally(() => {
          if (!controller.signal.aborted) setLoading(false);
        });
    }, 200);
    return () => {
      controller.abort();
      window.clearTimeout(timer);
    };
  }, [open, inputValue, loadOptions, t]);

  return (
    <MultiValueCombobox
      values={filter.values}
      options={options}
      placeholder={t`Any ${filter.label}`}
      ariaLabel={filter.label}
      className={cn("min-h-8 px-2 py-1 text-xs", compact ? "col-span-2 w-full min-w-0" : "w-56 shrink-0")}
      inputValue={inputValue}
      onInputValueChange={setInputValue}
      loading={loading}
      error={error}
      formatChipLabel={(value) => labels.current.get(value) ?? value}
      onOpenChange={setOpen}
      onChange={onChange}
    />
  );
}
