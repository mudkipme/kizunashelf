import { ArrowRightIcon } from "lucide-react";
import { Link } from "react-router-dom";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { relationFieldHref, relationTargetHref } from "@/lib/relations";
import type { RelationFieldSummary } from "@/types/api";

export function RelationFieldCard({ field }: { field: RelationFieldSummary }) {
  return (
    <section className="min-w-0 rounded-md border">
      <header className="flex items-start gap-3 border-b p-3">
        <div className="min-w-0 flex-1">
          <div className="flex min-w-0 items-center gap-2">
            <h2 className="truncate text-sm font-semibold">{field.field}</h2>
            <Badge variant="secondary">{field.edgeCount}</Badge>
          </div>
          <div className="mt-1 text-xs text-muted-foreground">
            {field.uniqueTargets} targets · {field.sourceCount} sources · {field.resolvedTargets}{" "}
            resolved
          </div>
        </div>
        <Button asChild variant="outline" size="sm">
          <Link to={relationFieldHref(field.field)}>
            Open
            <ArrowRightIcon data-icon="inline-end" />
          </Link>
        </Button>
      </header>
      <div className="flex flex-col gap-2 p-3">
        {field.topTargets.map((target) => (
          <Link
            key={target.key}
            to={relationTargetHref(field.field, target)}
            className="flex min-w-0 items-center gap-2 rounded-md border px-2 py-1 text-xs hover:bg-accent"
          >
            <span className="truncate">{target.targetTitle}</span>
            {target.targetTypeLabel ? (
              <span className="shrink-0 text-muted-foreground">{target.targetTypeLabel}</span>
            ) : null}
            <Badge variant="secondary">{target.count}</Badge>
          </Link>
        ))}
      </div>
    </section>
  );
}
