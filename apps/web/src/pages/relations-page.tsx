import { useEffect, useState } from "react";

import { fetchJson, errorMessage } from "@/api/client";
import { AppFrame } from "@/components/layout/app-frame";
import { RelationFieldCard } from "@/components/relations/relation-field-card";
import type { RelationGroupsResponse } from "@/types/api";

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
      const data = await fetchJson<RelationGroupsResponse>("/api/relation-groups");
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
                  ? `${state.data.fields.length} relation fields · updated ${state.data.generatedAt.slice(0, 10)}`
                  : "No relation data"}
            </p>
          </div>
        </header>

        {state.loading ? (
          <div className="rounded-md border p-8 text-center text-sm text-muted-foreground">
            Loading
          </div>
        ) : null}

        {!state.loading && state.data?.fields.length === 0 ? (
          <div className="rounded-md border p-8 text-center text-sm text-muted-foreground">
            No relations
          </div>
        ) : null}

        {state.data?.fields.length ? (
          <div className="grid grid-cols-1 gap-3 lg:grid-cols-2 xl:grid-cols-3">
            {state.data.fields.map((field) => (
              <RelationFieldCard key={field.field} field={field} />
            ))}
          </div>
        ) : null}
      </div>
    </AppFrame>
  );
}
