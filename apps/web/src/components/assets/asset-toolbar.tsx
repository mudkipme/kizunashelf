import { Grid2X2Icon, ListIcon, SlidersHorizontalIcon } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Select } from "@/components/ui/select";
import { Separator } from "@/components/ui/separator";
import { allOptions, allStatuses, defaultDirection, defaultSort } from "@/lib/constants";
import type { StatsResponse } from "@/types/api";

export function AssetToolbar({
  stats,
  status,
  refs,
  cover,
  sort,
  direction,
  view,
  onStatusChange,
  onRefsChange,
  onCoverChange,
  onSortChange,
  onDirectionChange,
  onViewChange,
}: {
  stats?: StatsResponse;
  status: string;
  refs: string;
  cover: string;
  sort: string;
  direction: string;
  view: string;
  onStatusChange: (value: string) => void;
  onRefsChange: (value: string) => void;
  onCoverChange: (value: string) => void;
  onSortChange: (value: string) => void;
  onDirectionChange: (value: string) => void;
  onViewChange: (value: string) => void;
}) {
  return (
    <div className="flex flex-wrap items-center gap-2 border-b px-3 py-2">
      <div className="flex items-center gap-2 text-xs font-medium uppercase text-muted-foreground">
        <SlidersHorizontalIcon />
        Filters
      </div>
      <Select value={status} onChange={(event) => onStatusChange(event.target.value)}>
        <option value={allStatuses}>All statuses</option>
        {stats?.byStatus.map((item) => (
          <option key={item.name} value={item.name}>
            {item.name} ({item.count})
          </option>
        ))}
      </Select>
      <Select value={refs} onChange={(event) => onRefsChange(event.target.value)}>
        <option value={allOptions}>Any refs</option>
        <option value="with">With refs</option>
        <option value="without">Without refs</option>
      </Select>
      <Select value={cover} onChange={(event) => onCoverChange(event.target.value)}>
        <option value={allOptions}>Any cover</option>
        <option value="with">With cover</option>
        <option value="without">Without cover</option>
      </Select>
      <Separator orientation="vertical" className="mx-1 hidden h-6 sm:block" />
      <Select value={sort} onChange={(event) => onSortChange(event.target.value)}>
        <option value={defaultSort}>Sort by title</option>
        <option value="date">Sort by date</option>
        <option value="status">Sort by status</option>
        <option value="relations">Sort by links</option>
        <option value="path">Sort by path</option>
      </Select>
      <Select value={direction} onChange={(event) => onDirectionChange(event.target.value)}>
        <option value={defaultDirection}>Ascending</option>
        <option value="desc">Descending</option>
      </Select>
      <div className="ml-auto flex items-center gap-1">
        <Button
          variant={view === "list" ? "secondary" : "ghost"}
          size="sm"
          onClick={() => onViewChange("list")}
        >
          <ListIcon data-icon="inline-start" />
          List
        </Button>
        <Button
          variant={view === "grid" ? "secondary" : "ghost"}
          size="sm"
          onClick={() => onViewChange("grid")}
        >
          <Grid2X2Icon data-icon="inline-start" />
          Grid
        </Button>
      </div>
    </div>
  );
}
