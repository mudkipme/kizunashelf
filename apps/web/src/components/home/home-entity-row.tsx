import { Link } from "react-router-dom";

import { EntityCover } from "@/components/assets/entity-cover";
import { Badge } from "@/components/ui/badge";
import type { EntitySummary } from "@/types/api";

export function HomeEntityRow({ entity }: { entity: EntitySummary }) {
  return (
    <Link
      to={`/entities/${encodeURIComponent(entity.id)}`}
      className="grid grid-cols-[44px_1fr] gap-3 border-b px-3 py-2 transition-colors last:border-b-0 hover:bg-accent focus-visible:bg-accent focus-visible:outline-none"
    >
      <EntityCover entity={entity} />
      <span className="min-w-0">
        <span className="flex min-w-0 items-center gap-2">
          <span className="truncate text-sm font-medium leading-5">{entity.title}</span>
          {entity.status ? <Badge variant="outline">{entity.status}</Badge> : null}
        </span>
        <span className="mt-1 flex min-w-0 items-center gap-2 text-xs text-muted-foreground">
          {entity.subtitle ? <span className="truncate">{entity.subtitle}</span> : null}
          {entity.date ? <span className="shrink-0">{entity.date}</span> : null}
          <span className="ml-auto shrink-0">{entity.relationCount} links</span>
        </span>
      </span>
    </Link>
  );
}
