import { useEffect, useMemo, useState } from "react";
import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { PlusIcon, SearchIcon } from "lucide-react";
import { Link, useNavigate, useSearchParams } from "react-router-dom";
import { toast } from "sonner";

import { errorMessage } from "@/api/client";
import { quickAddEntity, searchSources } from "@/api/entities";
import { useInvalidateEntityData } from "@/api/invalidate-entity-data";
import { configQuery } from "@/api/queries";
import { AppFrame } from "@/components/layout/app-frame";
import { Alert } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import { CONTENT_WRITES_DISABLED, useCapabilities } from "@/lib/capabilities";
import type { ExternalMatch } from "@/types/api";

const ALL = "all";
const MIN_QUERY_LENGTH = 2;

// A candidate's stable identity across a result set (a work can appear once per
// type in cross-type search, so the type is part of the key).
function matchKey(match: ExternalMatch): string {
  return `${match.candidate.provider}:${match.candidate.sourceId}:${match.entityType}`;
}

export function QuickCapturePage() {
  const navigate = useNavigate();
  const [searchParams] = useSearchParams();
  const requestedType = searchParams.get("type") ?? ALL;
  const invalidateEntityData = useInvalidateEntityData();
  const config = useQuery(configQuery());
  const capabilities = useCapabilities();
  const contentWritable = capabilities.contentWritable;

  const [rawQuery, setRawQuery] = useState("");
  const [query, setQuery] = useState("");
  const [typeId, setTypeId] = useState(requestedType);
  const [provider, setProvider] = useState(ALL);
  const [addingKey, setAddingKey] = useState<string>();

  // Debounce typing so a cross-type "search anything" doesn't fan out to every
  // provider on each keystroke.
  useEffect(() => {
    const handle = window.setTimeout(() => setQuery(rawQuery.trim()), 400);
    return () => window.clearTimeout(handle);
  }, [rawQuery]);

  const typeLabels = useMemo(() => {
    const labels = new Map<string, string>();
    for (const type of config.data?.types ?? []) labels.set(type.id, type.label);
    return labels;
  }, [config.data]);

  // Probe available providers with an empty query (no network to any provider):
  // it returns the per-provider `enabled` summaries used to populate the provider
  // filter and to decide whether Quick Capture is usable at all.
  const probe = useQuery({
    queryKey: ["externalSearch", "probe"],
    queryFn: ({ signal }) => searchSources({ type: ALL, q: "" }, { signal }),
    staleTime: 60_000,
  });
  const enabledProviders = useMemo(
    () => (probe.data?.providers ?? []).filter((item) => item.enabled),
    [probe.data],
  );

  // No usable provider → Quick Capture has nothing to search; fall back to the
  // manual add page (which stays available in read-only mode too).
  useEffect(() => {
    if (probe.isSuccess && enabledProviders.length === 0) {
      navigate(`/entities/new/manual${requestedType !== ALL ? `?type=${encodeURIComponent(requestedType)}` : ""}`, {
        replace: true,
      });
    }
  }, [probe.isSuccess, enabledProviders.length, navigate, requestedType]);

  const searchEnabled = query.length >= MIN_QUERY_LENGTH;
  const results = useQuery({
    queryKey: ["externalSearch", "results", { query, typeId, provider }],
    queryFn: ({ signal }) =>
      searchSources({ type: typeId, provider, q: query, pageSize: 15 }, { signal }),
    enabled: searchEnabled,
    placeholderData: keepPreviousData,
  });

  const providerErrors = useMemo(
    () => (results.data?.providers ?? []).filter((item) => item.error),
    [results.data],
  );

  async function add(match: ExternalMatch) {
    if (!contentWritable) return;
    if (match.existing) {
      navigate(`/entities/${encodeURIComponent(match.existing.id)}`);
      return;
    }
    const key = matchKey(match);
    setAddingKey(key);
    try {
      const result = await quickAddEntity({ type: match.entityType, candidate: match.candidate });
      const failedCovers = (result.cover ?? []).filter((item) => item.status === "failed").length;
      if (failedCovers > 0) {
        toast.warning(`Added, but ${failedCovers} cover${failedCovers > 1 ? "s" : ""} couldn't be downloaded (URL kept).`);
      }
      if (result.episodes?.error) {
        toast.warning(`Added, but episode import failed: ${result.episodes.error}`);
      } else if (result.episodes && result.episodes.imported > 0) {
        toast.success(`Imported ${result.episodes.imported} episode${result.episodes.imported > 1 ? "s" : ""}.`);
      }
      if (result.basenameAdjusted) {
        toast.info(`A file named for this title already existed, so it was saved as “${result.entity.basename}”.`);
      }
      await invalidateEntityData();
      navigate(`/entities/${encodeURIComponent(result.entity.id)}`);
    } catch (error) {
      toast.error(errorMessage(error));
      setAddingKey(undefined);
    }
  }

  const queryError = config.error ?? capabilities.error ?? probe.error;
  const manualHref = `/entities/new/manual${
    typeId !== ALL || query ? `?${new URLSearchParams({ ...(typeId !== ALL ? { type: typeId } : {}), ...(query ? { title: query } : {}) })}` : ""
  }`;

  return (
    <AppFrame error={queryError ? errorMessage(queryError) : undefined}>
      <div className="mx-auto flex w-full max-w-5xl flex-col gap-4 p-4">
        <header className="flex flex-wrap items-start justify-between gap-3">
          <div className="min-w-0">
            <h1 className="truncate text-base font-semibold">Quick Capture</h1>
            <p className="mt-1 truncate text-xs text-muted-foreground">
              Search external sources, or add manually
            </p>
          </div>
          <Button asChild variant="outline">
            <Link to={manualHref}>
              <PlusIcon data-icon="inline-start" />
              Add manually
            </Link>
          </Button>
        </header>

        {!contentWritable ? <Alert>{CONTENT_WRITES_DISABLED} You can still open items already in your library.</Alert> : null}

        <section className="rounded-md border p-4">
          <div className="grid gap-3 md:grid-cols-[minmax(0,1fr)_180px_180px]">
            <label className="flex flex-col gap-1 text-sm font-medium">
              Search
              <div className="relative">
                <SearchIcon
                  className="pointer-events-none absolute left-2 top-1/2 size-4 -translate-y-1/2 text-muted-foreground"
                  aria-hidden
                />
                <Input
                  value={rawQuery}
                  onChange={(event) => setRawQuery(event.target.value)}
                  onKeyDown={(event) => {
                    if (event.key === "Enter") setQuery(rawQuery.trim());
                  }}
                  placeholder="Title, or paste a provider URL"
                  className="pl-8"
                  autoFocus
                />
              </div>
            </label>
            <label className="flex flex-col gap-1 text-sm font-medium">
              Type
              <Select value={typeId} onChange={(event) => setTypeId(event.target.value)}>
                <option value={ALL}>All types</option>
                {config.data?.types.map((type) => (
                  <option key={type.id} value={type.id}>
                    {type.label}
                  </option>
                ))}
              </Select>
            </label>
            <label className="flex flex-col gap-1 text-sm font-medium">
              Provider
              <Select value={provider} onChange={(event) => setProvider(event.target.value)}>
                <option value={ALL}>All providers</option>
                {enabledProviders.map((item) => (
                  <option key={item.id} value={item.id}>
                    {item.label}
                  </option>
                ))}
              </Select>
            </label>
          </div>
        </section>

        {providerErrors.length > 0 ? (
          <div className="rounded-md border border-amber-500/40 bg-amber-500/10 p-3 text-xs text-amber-700 dark:text-amber-400">
            {providerErrors.map((item) => `${item.label} search failed`).join(" · ")}
          </div>
        ) : null}

        <section className="flex flex-col gap-2">
          {!searchEnabled ? (
            <p className="rounded-md border border-dashed p-6 text-center text-sm text-muted-foreground">
              Type at least {MIN_QUERY_LENGTH} characters to search.
            </p>
          ) : results.isPending ? (
            <p className="rounded-md border border-dashed p-6 text-center text-sm text-muted-foreground">Searching…</p>
          ) : (results.data?.items.length ?? 0) === 0 ? (
            <p className="rounded-md border border-dashed p-6 text-center text-sm text-muted-foreground">
              No matches found.
            </p>
          ) : (
            <ul className="flex flex-col gap-2">
              {results.data?.items.map((match) => {
                const key = matchKey(match);
                const inLibrary = Boolean(match.existing);
                return (
                  <li key={key}>
                    <button
                      type="button"
                      onClick={() => void add(match)}
                      disabled={(!contentWritable && !inLibrary) || Boolean(addingKey)}
                      className="flex w-full items-start gap-3 rounded-md border p-3 text-left transition-colors hover:bg-accent disabled:pointer-events-none disabled:opacity-60"
                    >
                      {match.candidate.coverUrl ? (
                        <img
                          src={match.candidate.coverUrl}
                          alt=""
                          loading="lazy"
                          className="h-20 w-14 shrink-0 rounded object-cover"
                        />
                      ) : (
                        <div className="h-20 w-14 shrink-0 rounded bg-muted" />
                      )}
                      <div className="min-w-0 flex-1">
                        <div className="flex flex-wrap items-center gap-2">
                          <span className="truncate font-medium">{match.candidate.title}</span>
                          {inLibrary ? (
                            <span className="rounded-full bg-emerald-500/15 px-2 py-0.5 text-xs font-medium text-emerald-700 dark:text-emerald-400">
                              In library ✓
                            </span>
                          ) : null}
                        </div>
                        {match.candidate.originalTitle && match.candidate.originalTitle !== match.candidate.title ? (
                          <p className="truncate text-xs text-muted-foreground">{match.candidate.originalTitle}</p>
                        ) : null}
                        {match.candidate.brief ? (
                          <p className="mt-1 line-clamp-2 text-xs text-muted-foreground">{match.candidate.brief}</p>
                        ) : null}
                        <div className="mt-1.5 flex flex-wrap items-center gap-1.5 text-xs text-muted-foreground">
                          <span className="rounded border px-1.5 py-0.5">{match.candidate.provider}</span>
                          <span className="rounded border px-1.5 py-0.5">
                            {typeLabels.get(match.entityType) ?? match.entityType}
                          </span>
                          {addingKey === key ? <span>Adding…</span> : null}
                        </div>
                      </div>
                    </button>
                  </li>
                );
              })}
            </ul>
          )}

          {searchEnabled ? (
            <Link
              to={manualHref}
              className="rounded-md border border-dashed p-3 text-center text-sm text-muted-foreground transition-colors hover:bg-accent"
            >
              Create “{query}” manually →
            </Link>
          ) : null}
        </section>
      </div>
    </AppFrame>
  );
}
