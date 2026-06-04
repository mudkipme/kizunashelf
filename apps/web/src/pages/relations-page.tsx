import { useEffect, useState } from "react";
import { getRelationGroups } from "@kizunashelf/api-contract";
import { ArrowRightIcon, ExternalLinkIcon } from "lucide-react";
import { Link } from "react-router-dom";

import { apiFetch, errorMessage } from "@/api/client";
import { AppFrame } from "@/components/layout/app-frame";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { relationFieldHref } from "@/lib/relations";
import type {
  RelationGroupsResponse,
  RelationFieldSummary,
  RelationTargetHubSummary,
  RelationTargetTypeSummary,
} from "@/types/api";

type RelationsState = {
  data?: RelationGroupsResponse;
  loading: boolean;
  error?: string;
};

export function RelationsPage() {
  const [state, setState] = useState<RelationsState>({ loading: true });

  useEffect(() => {
    void loadRelations();
  }, []);

  async function loadRelations() {
    setState({ loading: true });
    try {
      const data = await getRelationGroups(undefined, apiFetch);
      setState({ data, loading: false });
    } catch (error) {
      setState({ loading: false, error: errorMessage(error) });
    }
  }

  return (
    <AppFrame error={state.error}>
      <div className="mx-auto flex w-full max-w-7xl flex-col gap-4 p-4">
        <header className="flex flex-wrap items-end justify-between gap-3">
          <div className="min-w-0">
            <h1 className="truncate text-base font-semibold">Kizuna Map</h1>
            <p className="mt-1 text-xs text-muted-foreground">
              {state.loading
                ? "Loading"
                : state.data
                  ? `${state.data.targetTypes.length} target categories · ${state.data.fields.length} fields · updated ${state.data.generatedAt.slice(0, 10)}`
                  : "No relation data"}
            </p>
          </div>
        </header>

        {state.loading ? (
          <div className="rounded-md border p-8 text-center text-sm text-muted-foreground">
            Loading
          </div>
        ) : null}

        {!state.loading && state.data?.targetTypes.length === 0 ? (
          <div className="rounded-md border p-8 text-center text-sm text-muted-foreground">
            No relations
          </div>
        ) : null}

        {state.data?.targetTypes.length ? (
          <div className="grid grid-cols-1 gap-3 lg:grid-cols-2 xl:grid-cols-3">
            {state.data.targetTypes.map((targetType) => (
              <RelationTargetTypeCard key={targetType.type} targetType={targetType} />
            ))}
          </div>
        ) : null}

        {state.data?.fields.length ? <RelationFieldIndex fields={state.data.fields} /> : null}
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
          <Badge variant="secondary">{target.count}</Badge>
          {target.sourceTypes.map((type) => (
            <Badge key={type.name} variant="outline">
              {type.name} {type.count}
            </Badge>
          ))}
        </div>
        <div className="mt-1 flex flex-wrap gap-1">
          {target.fields.map((field) => (
            <Badge key={field.name} variant="outline">
              {field.name} {field.count}
            </Badge>
          ))}
        </div>
      </div>
      <div className="flex items-center justify-end gap-2">
        {entityHref ? (
          <Button asChild variant="ghost" size="sm">
            <Link to={entityHref}>
              Entity
              <ExternalLinkIcon data-icon="inline-end" />
            </Link>
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

function RelationFieldIndex({ fields }: { fields: RelationFieldSummary[] }) {
  return (
    <section className="rounded-md border px-3 py-3">
      <div className="flex flex-wrap items-center gap-2">
        <span className="text-xs font-medium text-muted-foreground">Field drill-down</span>
        {fields.map((field) => (
          <Button key={field.field} asChild variant="ghost" size="sm">
            <Link to={relationFieldHref(field.field)}>
              {field.field}
              <Badge variant="secondary">{field.edgeCount}</Badge>
            </Link>
          </Button>
        ))}
      </div>
    </section>
  );
}
