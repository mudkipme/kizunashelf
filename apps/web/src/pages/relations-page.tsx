import { useQuery } from "@tanstack/react-query";
import { Link } from "react-router-dom";

import { errorMessage } from "@/api/client";
import { relationGroupsQuery } from "@/api/queries";
import { AppFrame } from "@/components/layout/app-frame";
import { Badge } from "@/components/ui/badge";
import { useTitleLanguage } from "@/lib/language";
import { entityTitle } from "@/lib/title-language";
import type {
  RelationTargetHubSummary,
  RelationTargetTypeSummary,
} from "@/types/api";

export function RelationsPage() {
  const relations = useQuery(relationGroupsQuery());

  return (
    <AppFrame error={relations.error ? errorMessage(relations.error) : undefined}>
      <div className="mx-auto flex w-full max-w-7xl flex-col gap-4 p-4">
        <header className="flex flex-wrap items-end justify-between gap-3">
          <div className="min-w-0">
            <h1 className="truncate text-base font-semibold">Kizuna Map</h1>
            <p className="mt-1 text-xs text-muted-foreground">
              {relations.isPending
                ? "Loading"
                : relations.data
                  ? `${relations.data.targetTypes.length} target categories · updated ${relations.data.generatedAt.slice(0, 10)}`
                  : "No relation data"}
            </p>
          </div>
        </header>

        {relations.isPending ? (
          <div className="rounded-md border p-8 text-center text-sm text-muted-foreground">
            Loading
          </div>
        ) : null}

        {!relations.isPending && relations.data?.targetTypes.length === 0 ? (
          <div className="rounded-md border p-8 text-center text-sm text-muted-foreground">
            No relations
          </div>
        ) : null}

        {relations.data?.targetTypes.length ? (
          <div className="grid grid-cols-1 gap-3 lg:grid-cols-2 xl:grid-cols-3">
            {relations.data.targetTypes.map((targetType) => (
              <RelationTargetTypeCard key={targetType.type} targetType={targetType} />
            ))}
          </div>
        ) : null}
      </div>
    </AppFrame>
  );
}

function RelationTargetTypeCard({ targetType }: { targetType: RelationTargetTypeSummary }) {
  return (
    <section className="min-w-0 rounded-md border">
      <header className="border-b p-3">
        <div className="flex min-w-0 items-center gap-2">
          <h2 className="truncate text-sm font-semibold">{targetType.typeLabel}</h2>
          <Badge variant="secondary">{targetType.edgeCount}</Badge>
        </div>
        <div className="mt-1 text-xs text-muted-foreground">
          {targetType.uniqueTargets} targets · {targetType.resolvedTargets} resolved
        </div>
      </header>
      <div>
        {targetType.topTargets.map((target) => (
          <RelationHubRow key={target.key} target={target} />
        ))}
      </div>
    </section>
  );
}

function RelationHubRow({ target }: { target: RelationTargetHubSummary }) {
  const entityHref = target.targetId ? `/entities/${encodeURIComponent(target.targetId)}` : undefined;
  const title = entityTitle({ title: target.targetTitle, titles: target.targetTitles }, useTitleLanguage());
  return (
    <div className="min-w-0 border-b px-3 py-2 last:border-b-0">
      <div className="min-w-0">
        <div className="flex min-w-0 flex-wrap items-center gap-2">
          {entityHref ? (
            <Link to={entityHref} className="min-w-0 truncate text-sm font-medium hover:underline">
              {title}
            </Link>
          ) : (
            <span className="min-w-0 truncate text-sm font-medium">{title}</span>
          )}
          <Badge variant="secondary">{target.count}</Badge>
          {target.sourceTypes.map((type) => (
            <Badge key={type.name} variant="outline">
              {type.name} {type.count}
            </Badge>
          ))}
        </div>
      </div>
    </div>
  );
}
