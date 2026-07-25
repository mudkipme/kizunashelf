import type { ReactNode } from "react";

import { EntityGridItem } from "@/components/assets/entity-grid-item";
import { EntityListItem } from "@/components/assets/entity-list-item";
import { PaginationBar } from "@/components/assets/pagination-bar";
import { pageSize } from "@/lib/constants";
import { cn } from "@/lib/utils";
import type { EntitySummary, SmartViewLayout } from "@/types/api";

/// A page of entities in either layout, with its empty state and pagination —
/// the result surface shared by the library browser and the smart-list page
/// (the same list, saved or not).
export function EntityResults({
  entities,
  layout,
  labelsByType,
  showCover,
  showType,
  loading,
  page,
  totalPages,
  total,
  empty,
  className,
  onPageChange,
}: {
  entities: EntitySummary[];
  layout: SmartViewLayout;
  labelsByType?: ReadonlyMap<string, ReadonlyMap<string, string>>;
  showCover: boolean;
  showType: boolean;
  loading: boolean;
  page: number;
  totalPages: number;
  total: number;
  empty: ReactNode;
  className?: string;
  onPageChange: (page: number) => void;
}) {
  return (
    <div className={cn("flex min-h-0 flex-1 flex-col overflow-hidden", className)}>
      <div className="min-h-0 flex-1 overflow-auto">
        {layout === "grid" ? (
          <div className="grid grid-cols-[repeat(auto-fill,minmax(220px,1fr))] gap-3 p-3">
            {entities.map((entity) => (
              <EntityGridItem
                key={entity.id}
                entity={entity}
                labelsByType={labelsByType}
                showCover={showCover}
                showType={showType}
              />
            ))}
          </div>
        ) : (
          entities.map((entity) => (
            <EntityListItem
              key={entity.id}
              entity={entity}
              labelsByType={labelsByType}
              showCover={showCover}
              showType={showType}
            />
          ))
        )}
        {!loading && entities.length === 0 ? (
          <div className="p-8 text-center text-sm text-muted-foreground">{empty}</div>
        ) : null}
      </div>
      <PaginationBar
        page={page}
        totalPages={totalPages}
        total={total}
        pageSize={pageSize}
        onPageChange={onPageChange}
      />
    </div>
  );
}
