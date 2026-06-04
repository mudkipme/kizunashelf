import { useEffect, useState } from "react";
import { getRelationGroup } from "@kizunashelf/api-contract";
import { ArrowLeftIcon, SearchIcon } from "lucide-react";
import { Link, useParams, useSearchParams } from "react-router-dom";

import { apiFetch, errorMessage } from "@/api/client";
import { PaginationBar } from "@/components/assets/pagination-bar";
import { AppFrame } from "@/components/layout/app-frame";
import { RelationTargetRow } from "@/components/relations/relation-target-row";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import type { RelationFieldResponse } from "@/types/api";

type FieldState = {
  data?: RelationFieldResponse;
  loading: boolean;
  error?: string;
};

const pageSize = 40;

export function RelationFieldPage() {
  const { field = "" } = useParams();
  const [searchParams, setSearchParams] = useSearchParams();
  const [state, setState] = useState<FieldState>({ loading: true });
  const query = searchParams.get("q") ?? "";
  const page = Math.max(1, Number(searchParams.get("page") ?? 1) || 1);
  const [queryInput, setQueryInput] = useState(query);

  useEffect(() => {
    setQueryInput(query);
  }, [query]);

  useEffect(() => {
    if (!field) return;
    void loadField();
  }, [field, query, page]);

  useEffect(() => {
    if (queryInput === query) return;
    const timeout = window.setTimeout(() => {
      const next = new URLSearchParams(searchParams);
      if (queryInput.trim()) next.set("q", queryInput.trim());
      else next.delete("q");
      next.set("page", "1");
      setSearchParams(next, { replace: true });
    }, 180);
    return () => window.clearTimeout(timeout);
  }, [queryInput]);

  async function loadField() {
    setState((current) => ({ ...current, loading: true, error: undefined }));
    try {
      const data = await getRelationGroup(
        field,
        {
          page,
          pageSize,
          ...(query.trim() ? { q: query.trim() } : {}),
        },
        undefined,
        apiFetch,
      );
      setState({ data, loading: false });
      if (data.page !== page) {
        const next = new URLSearchParams(searchParams);
        next.set("page", String(data.page));
        setSearchParams(next, { replace: true });
      }
    } catch (error) {
      setState((current) => ({ ...current, loading: false, error: errorMessage(error) }));
    }
  }

  function goToPage(nextPage: number) {
    const next = new URLSearchParams(searchParams);
    next.set("page", String(nextPage));
    setSearchParams(next);
  }

  return (
    <AppFrame error={state.error}>
      <div className="mx-auto flex min-h-[calc(100vh-3rem)] w-full max-w-7xl flex-col">
        <header className="flex flex-wrap items-center gap-3 border-b p-3">
          <Button asChild variant="ghost" size="sm">
            <Link to="/relations">
              <ArrowLeftIcon data-icon="inline-start" />
              Relations
            </Link>
          </Button>
          <div className="min-w-0 flex-1">
            <h1 className="truncate text-base font-semibold">{field}</h1>
            <p className="mt-1 text-xs text-muted-foreground">
              {state.loading
                ? "Loading"
                : state.data
                  ? `${state.data.uniqueTargets} targets · ${state.data.edgeCount} links`
                  : "No relation data"}
            </p>
          </div>
        </header>

        <div className="flex items-center gap-2 border-b p-3">
          <SearchIcon className="text-muted-foreground" />
          <Input
            value={queryInput}
            onChange={(event) => setQueryInput(event.target.value)}
            placeholder="Search relation targets"
          />
        </div>

        <div className="min-h-0 flex-1 overflow-auto">
          {state.loading ? (
            <div className="p-8 text-center text-sm text-muted-foreground">Loading</div>
          ) : null}
          {!state.loading && state.data?.targets.length === 0 ? (
            <div className="p-8 text-center text-sm text-muted-foreground">No targets</div>
          ) : null}
          {state.data?.targets.map((target) => <RelationTargetRow key={target.key} target={target} />)}
        </div>

        {state.data ? (
          <PaginationBar
            page={state.data.page}
            totalPages={state.data.totalPages}
            total={state.data.total}
            pageSize={state.data.pageSize}
            onPageChange={goToPage}
          />
        ) : null}
      </div>
    </AppFrame>
  );
}
