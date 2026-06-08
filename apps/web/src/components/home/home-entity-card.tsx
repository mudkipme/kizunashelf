import { Link } from "react-router-dom";

import { EntityDateList } from "@/components/assets/entity-date-list";
import { Badge } from "@/components/ui/badge";
import type { EntitySummary } from "@/types/api";

export function HomeEntityCard({ entity }: { entity: EntitySummary }) {
  return (
    <Link
      to={`/entities/${encodeURIComponent(entity.id)}`}
      className="group flex min-h-56 min-w-0 flex-col overflow-hidden rounded-md border bg-background transition-colors hover:bg-accent focus-visible:bg-accent focus-visible:outline-none"
    >
      <div className="flex aspect-[4/5] items-center justify-center bg-muted">
        {entity.image ? (
          <img
            src={entity.image}
            alt=""
            className="size-full object-cover transition-transform group-hover:scale-[1.02]"
            loading="lazy"
          />
        ) : (
          <span className="px-3 text-center text-sm font-medium text-muted-foreground">
            {entity.typeLabel}
          </span>
        )}
      </div>
      <div className="flex min-h-0 flex-1 flex-col gap-2 p-2.5">
        <div className="line-clamp-2 text-sm font-medium leading-5">{entity.title}</div>
        <div className="flex min-w-0 items-center gap-2">
          <Badge variant="outline">{entity.typeLabel}</Badge>
        </div>
        <div className="mt-auto flex min-w-0 items-center gap-2 text-xs text-muted-foreground">
          <span className="min-w-0 truncate">
            {entity.dates.length > 0 ? <EntityDateList entity={entity} compact /> : entity.basename}
          </span>
          <span className="ml-auto shrink-0 tabular-nums">{entity.relationCount} links</span>
        </div>
      </div>
    </Link>
  );
}
