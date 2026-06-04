import { ArrowRightIcon } from "lucide-react";
import { Link } from "react-router-dom";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import type { RelationTargetSummary } from "@/types/api";

export function RelationTargetRow({
  target,
}: {
  target: RelationTargetSummary;
}) {
  const entityHref = target.targetId ? `/entities/${encodeURIComponent(target.targetId)}` : undefined;
  return (
    <div className="grid min-w-0 gap-2 border-b px-3 py-2 last:border-b-0 md:grid-cols-[minmax(0,1fr)_auto]">
      <div className="min-w-0">
        <div className="flex min-w-0 flex-wrap items-center gap-2">
          {entityHref ? (
            <Link to={entityHref} className="min-w-0 truncate text-sm font-medium hover:underline">
              {target.targetTitle}
            </Link>
          ) : (
            <span className="min-w-0 truncate text-sm font-medium">{target.targetTitle}</span>
          )}
          {target.targetTypeLabel ? <Badge variant="outline">{target.targetTypeLabel}</Badge> : null}
          <Badge variant="secondary">{target.count}</Badge>
          {target.sourceTypes.map((type) => (
            <Badge key={type.name} variant="outline">
              {type.name} {type.count}
            </Badge>
          ))}
        </div>
        {target.examples.length > 0 ? (
          <div className="mt-1 line-clamp-1 text-xs leading-5 text-muted-foreground">
            {target.examples.map((entity) => entity.title).join(" · ")}
          </div>
        ) : null}
      </div>
      <div className="flex items-center justify-end gap-2">
        {entityHref ? (
          <Button asChild variant="ghost" size="sm">
            <Link to={entityHref}>Entity</Link>
          </Button>
        ) : null}
        {entityHref ? (
          <Button asChild variant="outline" size="sm">
            <Link to={entityHref}>
              Open
              <ArrowRightIcon data-icon="inline-end" />
            </Link>
          </Button>
        ) : null}
      </div>
    </div>
  );
}
