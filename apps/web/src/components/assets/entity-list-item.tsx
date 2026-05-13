import { Link } from "react-router-dom";

import { EntityCover } from "@/components/assets/entity-cover";
import { Badge } from "@/components/ui/badge";
import type { EntitySummary } from "@/types/api";

export function EntityListItem({ entity }: { entity: EntitySummary }) {
  return (
    <Link
      to={`/entities/${encodeURIComponent(entity.id)}`}
      className="grid w-full grid-cols-[44px_1fr] gap-3 border-b px-3 py-2 text-left transition-colors hover:bg-accent focus-visible:bg-accent focus-visible:outline-none"
    >
      <EntityCover entity={entity} />
      <span className="min-w-0">
        <span className="flex items-center gap-2">
          <span className="truncate text-sm font-medium">{entity.title}</span>
          <Badge variant="outline">{entity.typeLabel}</Badge>
        </span>
        <span className="mt-1 flex min-w-0 items-center gap-2 text-xs text-muted-foreground">
          {entity.status ? <span className="shrink-0">{entity.status}</span> : null}
          {entity.date ? <span className="truncate">{entity.date}</span> : null}
          <span className="ml-auto shrink-0">{entity.relationCount} links</span>
        </span>
        {entity.summary ? (
          <span className="mt-1 line-clamp-2 text-xs text-muted-foreground">{entity.summary}</span>
        ) : null}
      </span>
    </Link>
  );
}
