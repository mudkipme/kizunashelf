import { Grid2X2Icon, ListIcon, SlidersHorizontalIcon } from "lucide-react";

import { Button } from "@/components/ui/button";
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

type AssetToolbarProps = {
  className?: string;
  compact?: boolean;
  showLabel?: boolean;
  stats?: StatsResponse;
  refs: string;
  cover: string;
  sort: string;
  direction: string;
  view: string;
  titleLanguage: string;
  titleLanguages: string[];
  defaultTitleLabel?: string;
  onRefsChange: (value: string) => void;
  onCoverChange: (value: string) => void;
  onSortChange: (value: string) => void;
  onDirectionChange: (value: string) => void;
  onViewChange: (value: string) => void;
  onTitleLanguageChange: (value: string) => void;
};

export function AssetToolbar({
  className,
  compact = false,
  showLabel = true,
  stats,
  refs,
  cover,
  sort,
  direction,
  view,
  titleLanguage,
  titleLanguages,
  defaultTitleLabel = "Default title",
  onRefsChange,
  onCoverChange,
  onSortChange,
  onDirectionChange,
  onViewChange,
  onTitleLanguageChange,
}: AssetToolbarProps) {
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
      {compact ? null : <Separator orientation="vertical" className="mx-1 hidden h-6 sm:block" />}
      <Select
        value={sort}
        onChange={(event) => onSortChange(event.target.value)}
        className={compact ? "min-w-0" : undefined}
        aria-label="Sort"
      >
        <option value={defaultSort}>Sort by title</option>
        {stats?.dateFields.map((field) => (
          <option key={field} value={`date:${field}`}>
            Sort by {field}
          </option>
        ))}
        <option value="relations">Sort by links</option>
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
