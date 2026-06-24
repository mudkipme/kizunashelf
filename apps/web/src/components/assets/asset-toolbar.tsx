import { Grid2X2Icon, ListIcon, SlidersHorizontalIcon } from "lucide-react";

import { Button } from "@/components/ui/button";
import { MultiValueCombobox } from "@/components/ui/multi-value-combobox";
import { Select } from "@/components/ui/select";
import { Separator } from "@/components/ui/separator";
import { allOptions, defaultDirection, defaultSort } from "@/lib/constants";
import { cn } from "@/lib/utils";
import type { StatsResponse } from "@/types/api";

export type FieldFilterOption = {
  value: string;
  label?: string;
};

export type FieldFilter = {
  field: string;
  label: string;
  kind: "multi" | "bool";
  options: FieldFilterOption[];
  values: string[];
};

type AssetToolbarProps = {
  className?: string;
  compact?: boolean;
  showLabel?: boolean;
  stats?: StatsResponse;
  showRefsFilter?: boolean;
  showCoverFilter?: boolean;
  refs: string;
  cover: string;
  sort: string;
  direction: string;
  view: string;
  fieldFilters?: FieldFilter[];
  dateFieldLabel?: (field: string) => string;
  onRefsChange: (value: string) => void;
  onCoverChange: (value: string) => void;
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
  showRefsFilter = true,
  showCoverFilter = true,
  refs,
  cover,
  sort,
  direction,
  view,
  fieldFilters = [],
  dateFieldLabel = (field) => field,
  onRefsChange,
  onCoverChange,
  onSortChange,
  onDirectionChange,
  onViewChange,
  onFieldFilterChange,
}: AssetToolbarProps) {
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
          Filters
        </div>
      ) : null}
      {showRefsFilter ? (
        <Select
          value={refs}
          onChange={(event) => onRefsChange(event.target.value)}
          className={compact ? "min-w-0" : undefined}
          aria-label="Refs"
        >
          <option value={allOptions}>Any refs</option>
          <option value="with">With refs</option>
          <option value="without">Without refs</option>
        </Select>
      ) : null}
      {showCoverFilter ? (
        <Select
          value={cover}
          onChange={(event) => onCoverChange(event.target.value)}
          className={compact ? "min-w-0" : undefined}
          aria-label="Cover"
        >
          <option value={allOptions}>Any cover</option>
          <option value="with">With cover</option>
          <option value="without">Without cover</option>
        </Select>
      ) : null}
      {fieldFilters.map((filter) => (
        <FieldFilterControl
          key={filter.field}
          filter={filter}
          compact={compact}
          onChange={(values) => onFieldFilterChange?.(filter.field, values)}
        />
      ))}
      {compact || (!showRefsFilter && !showCoverFilter && !hasFieldFilters) ? null : (
        <Separator orientation="vertical" className="mx-1 hidden h-6 sm:block" />
      )}
      <Select
        value={sort}
        onChange={(event) => onSortChange(event.target.value)}
        className={compact ? "min-w-0" : undefined}
        aria-label="Sort"
      >
        <option value={defaultSort}>Sort by title</option>
        {stats?.dateFields.map((field) => (
          <option key={field} value={`date:${field}`}>
            Sort by {dateFieldLabel(field)}
          </option>
        ))}
        <option value="relationCount">Sort by connections</option>
        <option value="path">Sort by location</option>
      </Select>
      <Select
        value={direction}
        onChange={(event) => onDirectionChange(event.target.value)}
        className={compact ? "min-w-0" : undefined}
        aria-label="Direction"
      >
        <option value={defaultDirection}>Ascending</option>
        <option value="desc">Descending</option>
      </Select>
      <div className={cn(compact ? "col-span-2 grid grid-cols-2 gap-2" : "ml-auto flex items-center gap-1")}>
        <Button
          variant={view === "list" ? "secondary" : "ghost"}
          size="sm"
          className={compact ? "justify-center" : undefined}
          onClick={() => onViewChange("list")}
        >
          <ListIcon data-icon="inline-start" />
          List
        </Button>
        <Button
          variant={view === "grid" ? "secondary" : "ghost"}
          size="sm"
          className={compact ? "justify-center" : undefined}
          onClick={() => onViewChange("grid")}
        >
          <Grid2X2Icon data-icon="inline-start" />
          Grid
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
  if (filter.kind === "bool") {
    const value = filter.values[0] ?? allOptions;
    return (
      <Select
        value={value}
        onChange={(event) => onChange(event.target.value === allOptions ? [] : [event.target.value])}
        className={compact ? "col-span-2 w-full min-w-0" : "w-40 shrink-0"}
        aria-label={filter.label}
      >
        <option value={allOptions}>Any {filter.label}</option>
        {filter.options.map((option) => (
          <option key={option.value} value={option.value}>
            {filter.label} {option.label ?? option.value}
          </option>
        ))}
      </Select>
    );
  }

  return (
    <label className={cn("w-56 text-xs text-muted-foreground", compact ? "col-span-2 w-full min-w-0" : "shrink-0")}>
      <span className="mb-1 block truncate">{filter.label}</span>
      <MultiValueCombobox
        values={filter.values}
        options={filter.options}
        placeholder={`Any ${filter.label}`}
        ariaLabel={filter.label}
        className="min-h-8 px-2 py-1 text-xs"
        onChange={onChange}
      />
    </label>
  );
}
