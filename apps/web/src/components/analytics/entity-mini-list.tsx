import { Link } from "react-router-dom";

import { Badge } from "@/components/ui/badge";
import type { EntitySummary } from "@/types/api";

export function EntityMiniList({ items }: { items: EntitySummary[] }) {
  return (
    <div className="flex flex-col gap-1">
      {items.map((entity) => (
        <Link
          key={entity.id}
          to={`/entities/${encodeURIComponent(entity.id)}`}
          className="flex min-w-0 items-center gap-2 rounded-md px-2 py-1 text-xs hover:bg-accent"
        >
          <Badge variant="outline">{entity.typeLabel}</Badge>
          <span className="min-w-0 truncate">{entity.title}</span>
          {entity.date ? <span className="shrink-0 text-muted-foreground">{entity.date}</span> : null}
        </Link>
      ))}
    </div>
  );
}
