import { useEffect, useState } from "react";
import { ArrowLeftIcon, ExternalLinkIcon } from "lucide-react";
import { Link, useParams } from "react-router-dom";

import { fetchJson, errorMessage } from "@/api/client";
import { EntityListItem } from "@/components/assets/entity-list-item";
import { AppFrame } from "@/components/layout/app-frame";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import type { RelationTargetResponse } from "@/types/api";

type TargetState = {
  data?: RelationTargetResponse;
  loading: boolean;
  error?: string;
};

export function RelationTargetPage() {
  const { field = "", target = "" } = useParams();
  const [state, setState] = useState<TargetState>({ loading: true });

  useEffect(() => {
    if (!field || !target) return;
    void loadTarget();
  }, [field, target]);

  async function loadTarget() {
    setState({ loading: true });
    try {
      const data = await fetchJson<RelationTargetResponse>(
        `/api/relation-groups/${encodeURIComponent(field)}/${encodeURIComponent(target)}`,
      );
      setState({ data, loading: false });
    } catch (error) {
      setState({ loading: false, error: errorMessage(error) });
    }
  }

  const relationTarget = state.data?.target;

  return (
    <AppFrame error={state.error}>
      <div className="mx-auto flex w-full max-w-7xl flex-col gap-4 p-4">
        <header className="flex flex-wrap items-start gap-3">
          <Button asChild variant="ghost" size="sm">
            <Link to={`/relations/${encodeURIComponent(field)}`}>
              <ArrowLeftIcon data-icon="inline-start" />
              {field}
            </Link>
          </Button>
          <div className="min-w-0 flex-1">
            <div className="flex flex-wrap items-center gap-2">
              <Badge variant="secondary">{field}</Badge>
              {relationTarget?.targetTypeLabel ? (
                <Badge variant="outline">{relationTarget.targetTypeLabel}</Badge>
              ) : null}
              {relationTarget ? <Badge variant="secondary">{relationTarget.count} links</Badge> : null}
            </div>
            <h1 className="mt-2 break-words text-xl font-semibold leading-snug">
              {relationTarget?.targetTitle ?? target}
            </h1>
            {relationTarget?.sourceTypes.length ? (
              <div className="mt-2 flex flex-wrap gap-1">
                {relationTarget.sourceTypes.map((type) => (
                  <Badge key={type.name} variant="outline">
                    {type.name} {type.count}
                  </Badge>
                ))}
              </div>
            ) : null}
          </div>
          {relationTarget?.targetId ? (
            <Button asChild variant="outline" size="sm">
              <Link to={`/entities/${encodeURIComponent(relationTarget.targetId)}`}>
                Entity
                <ExternalLinkIcon data-icon="inline-end" />
              </Link>
            </Button>
          ) : null}
        </header>

        {state.loading ? (
          <div className="rounded-md border p-8 text-center text-sm text-muted-foreground">
            Loading
          </div>
        ) : null}

        {!state.loading && !state.data ? (
          <div className="rounded-md border p-8 text-center text-sm text-muted-foreground">
            Relation target not found
          </div>
        ) : null}

        {state.data?.groups.map((group) => (
          <section key={group.typeLabel} className="rounded-md border">
            <header className="flex items-center gap-2 border-b px-3 py-2">
              <h2 className="text-sm font-semibold">{group.typeLabel}</h2>
              <Badge variant="secondary">{group.count}</Badge>
            </header>
            <div>
              {group.items.map((entity) => (
                <EntityListItem key={entity.id} entity={entity} />
              ))}
            </div>
          </section>
        ))}
      </div>
    </AppFrame>
  );
}
