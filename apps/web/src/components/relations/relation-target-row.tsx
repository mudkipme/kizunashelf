import { ArrowRightIcon } from "lucide-react";
import { Link } from "react-router-dom";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { relationTargetHref } from "@/lib/relations";
import type { RelationTargetSummary } from "@/types/api";

export function RelationTargetRow({
  field,
  target,
}: {
  field: string;
  target: RelationTargetSummary;
}) {
  return (
    <div className="grid min-w-0 gap-2 border-b px-3 py-2 last:border-b-0 md:grid-cols-[minmax(0,1fr)_auto]">
      <div className="min-w-0">
        <div className="flex min-w-0 flex-wrap items-center gap-2">
          <Link
            to={relationTargetHref(field, target)}
            className="min-w-0 truncate text-sm font-medium hover:underline"
          >
            {target.targetTitle}
          </Link>
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
        {target.targetId ? (
          <Button asChild variant="ghost" size="sm">
            <Link to={`/entities/${encodeURIComponent(target.targetId)}`}>Entity</Link>
          </Button>
        ) : null}
        <Button asChild variant="outline" size="sm">
          <Link to={relationTargetHref(field, target)}>
            Open
            <ArrowRightIcon data-icon="inline-end" />
          </Link>
        </Button>
      </div>
    </div>
  );
}
