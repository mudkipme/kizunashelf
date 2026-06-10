import { Grid2X2Icon, ListIcon, SlidersHorizontalIcon } from "lucide-react";

import { Button } from "@/components/ui/button";
import {
  Combobox,
  ComboboxChip,
  ComboboxChips,
  ComboboxChipsInput,
  ComboboxContent,
  ComboboxEmpty,
  ComboboxItem,
  ComboboxList,
  ComboboxValue,
} from "@/components/ui/combobox";
import { Select } from "@/components/ui/select";
import { Separator } from "@/components/ui/separator";
import {
  allOptions,
  defaultDirection,
  defaultSort,
  defaultTitleOptionId,
} from "@/lib/constants";
import { titleLanguageLabel } from "@/lib/title-language";
import { cn } from "@/lib/utils";
import type { StatsResponse } from "@/types/api";

export type EnumFieldFilter = {
  field: string;
  label: string;
  options: string[];
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
  titleLanguage: string;
  titleLanguages: string[];
  enumFilters?: EnumFieldFilter[];
  defaultTitleLabel?: string;
  dateFieldLabel?: (field: string) => string;
  onRefsChange: (value: string) => void;
  onCoverChange: (value: string) => void;
  onSortChange: (value: string) => void;
  onDirectionChange: (value: string) => void;
  onViewChange: (value: string) => void;
  onTitleLanguageChange: (value: string) => void;
  onEnumFilterChange?: (field: string, values: string[]) => void;
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
  titleLanguage,
  titleLanguages,
  enumFilters = [],
  defaultTitleLabel = "Default title",
  dateFieldLabel = (field) => field,
  onRefsChange,
  onCoverChange,
  onSortChange,
  onDirectionChange,
  onViewChange,
  onTitleLanguageChange,
  onEnumFilterChange,
}: AssetToolbarProps) {
  const hasFieldFilters = enumFilters.length > 0;

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
      {enumFilters.map((filter) => (
        <EnumFilterCombobox
          key={filter.field}
          filter={filter}
          compact={compact}
          onChange={(values) => onEnumFilterChange?.(filter.field, values)}
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
        <option value="relationCount">Sort by relation count</option>
        <option value="path">Sort by path</option>
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
      <Select
        value={titleLanguage}
        onChange={(event) => onTitleLanguageChange(event.target.value)}
        className={compact ? "min-w-0" : undefined}
        aria-label="Display title"
      >
        <option value={defaultTitleOptionId}>{defaultTitleLabel}</option>
        {titleLanguages.map((language) => (
          <option key={language} value={language}>
            {titleLanguageLabel(language)}
          </option>
        ))}
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

function EnumFilterCombobox({
  filter,
  compact,
  onChange,
}: {
  filter: EnumFieldFilter;
  compact: boolean;
  onChange: (values: string[]) => void;
}) {
  return (
    <label className={cn("w-56 text-xs text-muted-foreground", compact ? "col-span-2 w-full min-w-0" : "shrink-0")}>
      <span className="mb-1 block truncate">{filter.label}</span>
      <Combobox items={filter.options} multiple value={filter.values} onValueChange={onChange}>
        <ComboboxChips className="min-h-8 w-full px-2 py-1 text-xs">
          <ComboboxValue>
            {filter.values.map((item) => (
              <ComboboxChip key={item}>{item}</ComboboxChip>
            ))}
          </ComboboxValue>
          <ComboboxChipsInput placeholder={`Any ${filter.label}`} />
        </ComboboxChips>
        <ComboboxContent>
          <ComboboxEmpty>No options found.</ComboboxEmpty>
          <ComboboxList>
            {(item) => (
              <ComboboxItem key={item} value={item}>
                {item}
              </ComboboxItem>
            )}
          </ComboboxList>
        </ComboboxContent>
      </Combobox>
    </label>
  );
}
