import { useEffect, useMemo, useState } from "react";
import { ArrowLeftIcon } from "lucide-react";
import { useNavigate, useParams } from "react-router-dom";

import { fetchJson, errorMessage } from "@/api/client";
import { EntityDetail } from "@/components/assets/entity-detail";
import { AppFrame } from "@/components/layout/app-frame";
import { Button } from "@/components/ui/button";
import { groupRelations } from "@/lib/relations";
import type { EntityDatesResponse, EntityDetailResponse } from "@/types/api";

export function EntityPage() {
  const { id } = useParams();
  const navigate = useNavigate();
  const [state, setState] = useState<{
    detail?: EntityDetailResponse;
    dates?: EntityDatesResponse;
    loading: boolean;
    error?: string;
  }>({ loading: true });

  useEffect(() => {
    if (!id) return;
    setState({ loading: true });
    const encodedId = encodeURIComponent(id);
    void Promise.all([
      fetchJson<EntityDetailResponse>(`/api/entities/${encodedId}`),
      fetchJson<EntityDatesResponse>(`/api/entities/${encodedId}/dates`),
    ]).then(
      ([detail, dates]) => setState({ detail, dates, loading: false }),
      (error: unknown) => setState({ loading: false, error: errorMessage(error) }),
    );
  }, [id]);

  const entity = state.detail?.entity;
  const relationGroups = useMemo(
    () => groupRelations(state.detail?.relations ?? []),
    [state.detail],
  );

  return (
    <AppFrame error={state.error}>
      <div className="mx-auto flex w-full max-w-6xl flex-col gap-4 p-4">
        <div>
          <Button variant="ghost" size="sm" onClick={() => navigate(-1)}>
            <ArrowLeftIcon data-icon="inline-start" />
            Back
          </Button>
        </div>

        {state.loading ? (
          <div className="rounded-md border p-8 text-center text-sm text-muted-foreground">Loading</div>
        ) : entity ? (
          <EntityDetail
            entity={entity}
            relations={state.detail?.relations ?? []}
            relationGroups={relationGroups}
            dates={state.dates}
          />
        ) : (
          <div className="rounded-md border p-8 text-center text-sm text-muted-foreground">
            Entity not found
          </div>
        )}
      </div>
    </AppFrame>
  );
}
