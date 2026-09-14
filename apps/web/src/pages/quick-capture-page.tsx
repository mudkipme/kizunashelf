import { plural } from "@lingui/core/macro";
import { Trans, useLingui } from "@lingui/react/macro";
import { useQuery } from "@tanstack/react-query";
import { PlusIcon, SearchIcon } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { Link, useNavigate, useSearchParams } from "react-router-dom";
import { toast } from "sonner";

import { errorMessage } from "@/api/client";
import { quickAddEntity } from "@/api/entities";
import { useInvalidateEntityData } from "@/api/invalidate-entity-data";
import {
  configQuery,
  externalProvidersQuery,
  externalSearchQuery,
  providerCatalogQuery,
} from "@/api/queries";
import { ExternalSearchErrors } from "@/components/entities/external-search-errors";
import { AppFrame } from "@/components/layout/app-frame";
import { PageContainer } from "@/components/layout/page-container";
import { Alert } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import { useDebouncedCallback } from "@/hooks/use-debounce";
import { CONTENT_WRITES_DISABLED, useCapabilities } from "@/lib/capabilities";
import { useLanguagePreference } from "@/lib/language";
import { useQuickCaptureTypeStore } from "@/lib/quick-capture-preferences";
import { typeSupportsQuickCapture } from "@/lib/type-config";
import { cn } from "@/lib/utils";
import type { ExternalMatch } from "@/types/api";

const ALL = "all";
const MIN_QUERY_LENGTH = 2;

// A candidate's stable identity across a result set.
function matchKey(match: ExternalMatch): string {
  return `${match.candidate.provider}:${match.candidate.sourceId}`;
}

export function QuickCapturePage() {
  const { t } = useLingui();
  const navigate = useNavigate();
  const [searchParams] = useSearchParams();
  const requestedType = searchParams.get("type") ?? undefined;
  const invalidateEntityData = useInvalidateEntityData();
  const config = useQuery(configQuery());
  const providerCatalog = useQuery(providerCatalogQuery());
  const capabilities = useCapabilities();
  const contentWritable = capabilities.contentWritable;
  // The full preference (possibly zh-Hans/zh-Hant): providers that distinguish
  // the scripts localize search results and quick-add episode titles with it.
  const language = useLanguagePreference();

  // Every search is scoped to one type (the core has no cross-type search). An
  // explicit `?type=` wins; otherwise start from the remembered last selection
  // and let the fallback effect below settle on a valid searchable type.
  const lastType = useQuickCaptureTypeStore((state) => state.lastType);
  const setLastType = useQuickCaptureTypeStore((state) => state.setLastType);

  const [rawQuery, setRawQuery] = useState("");
  const [query, setQuery] = useState("");
  const [typeId, setTypeId] = useState(requestedType ?? lastType);
  const [provider, setProvider] = useState(ALL);
  const [addingKey, setAddingKey] = useState<string>();

  const { schedule: scheduleQuery, cancel: cancelQuery } = useDebouncedCallback(
    (value: string) => setQuery(value),
    400,
  );
  useEffect(() => {
    scheduleQuery(rawQuery.trim());
    return cancelQuery;
  }, [rawQuery, scheduleQuery, cancelQuery]);

  const providerLabels = useMemo(() => {
    const labels = new Map<string, string>();
    for (const item of providerCatalog.data?.providers ?? []) labels.set(item.id, item.label);
    return labels;
  }, [providerCatalog.data]);

  // Only types with an external provider configured can be searched here, so the
  // rest are dropped from the Type picker. Until the provider catalog loads we
  // can't validate the refs, so show every type; once known, keep only those whose
  // `externalRef` fields / external body sections point at a real provider.
  const searchableTypes = useMemo(() => {
    const types = config.data?.types ?? [];
    const catalog = providerCatalog.data;
    if (!catalog) return types;
    const providerIds = new Set(catalog.providers.map((item) => item.id.toLowerCase()));
    return types.filter((type) => typeSupportsQuickCapture(type, providerIds));
  }, [config.data, providerCatalog.data]);

  // A later `?type=` navigation (the route doesn't remount) re-selects that type.
  useEffect(() => {
    if (requestedType) setTypeId(requestedType);
  }, [requestedType]);

  // Settle the selection once the searchable set is known: keep a valid choice,
  // otherwise fall back to the remembered last type, then the first searchable
  // one. No searchable type at all → Quick Capture has nothing to search; fall
  // back to the manual add page (which stays available in read-only mode too).
  useEffect(() => {
    if (!config.data || !providerCatalog.data) return;
    if (searchableTypes.length === 0) {
      navigate(
        `/entities/new/manual${requestedType ? `?type=${encodeURIComponent(requestedType)}` : ""}`,
        { replace: true },
      );
      return;
    }
    if (typeId && searchableTypes.some((type) => type.id === typeId)) return;
    const fallback = searchableTypes.find((type) => type.id === lastType) ?? searchableTypes[0];
    setTypeId(fallback.id);
  }, [
    config.data,
    providerCatalog.data,
    searchableTypes,
    typeId,
    lastType,
    navigate,
    requestedType,
  ]);

  // Remember every validated selection (picked here or arrived via `?type=`), so
  // type-less entry points (home, the all-types library view) reuse it.
  useEffect(() => {
    if (!providerCatalog.data || !typeId) return;
    if (searchableTypes.some((type) => type.id === typeId)) setLastType(typeId);
  }, [providerCatalog.data, typeId, searchableTypes, setLastType]);

  // Empty-query probes return per-provider `enabled` summaries without hitting
  // any provider network; scoped to the selected type, so the dropdown lists
  // only the providers that type's `externalRef` fields map to.
  const providerProbe = useQuery(externalProvidersQuery(typeId ?? ""));
  const enabledProviders = useMemo(
    () => (providerProbe.data?.providers ?? []).filter((item) => item.enabled),
    [providerProbe.data],
  );

  // A provider chosen for one type may not exist under the next; reset to All so
  // the search doesn't silently return nothing.
  useEffect(() => {
    if (
      provider !== ALL &&
      providerProbe.isSuccess &&
      !enabledProviders.some((item) => item.id === provider)
    ) {
      setProvider(ALL);
    }
  }, [provider, enabledProviders, providerProbe.isSuccess]);

  const hasQuery = rawQuery.trim().length >= MIN_QUERY_LENGTH && Boolean(typeId);
  const debouncing = query !== rawQuery.trim();
  const searchEnabled = hasQuery && !debouncing;
  const results = useQuery({
    ...externalSearchQuery({
      type: typeId ?? "",
      provider,
      q: searchEnabled ? query : "",
      pageSize: 15,
      language,
    }),
    enabled: searchEnabled,
  });

  async function add(match: ExternalMatch) {
    if (!contentWritable) return;
    if (match.existing) {
      navigate(`/entities/${encodeURIComponent(match.existing.id)}`);
      return;
    }
    const key = matchKey(match);
    setAddingKey(key);
    try {
      const result = await quickAddEntity({
        type: match.entityType,
        candidate: match.candidate,
        language,
      });
      const failedCovers = (result.cover ?? []).filter((item) => item.status === "failed").length;
      if (failedCovers > 0) {
        toast.warning(
          plural(failedCovers, {
            one: "Added, but # cover couldn't be downloaded (URL kept).",
            other: "Added, but # covers couldn't be downloaded (URL kept).",
          }),
        );
      }
      if (result.episodes?.error) {
        toast.warning(t`Added, but episode import failed: ${result.episodes.error}`);
      } else if (result.episodes && result.episodes.imported > 0) {
        toast.success(
          plural(result.episodes.imported, {
            one: "Imported # episode.",
            other: "Imported # episodes.",
          }),
        );
      }
      if (result.basenameAdjusted) {
        toast.info(
          t`A file named for this title already existed, so it was saved as “${result.entity.basename}”.`,
        );
      }
      await invalidateEntityData();
      navigate(`/entities/${encodeURIComponent(result.entity.id)}`);
    } catch (error) {
      toast.error(errorMessage(error));
      setAddingKey(undefined);
    }
  }

  const queryError = config.error ?? capabilities.error ?? providerProbe.error;
  const manualHref = `/entities/new/manual${
    typeId || query
      ? `?${new URLSearchParams({ ...(typeId ? { type: typeId } : {}), ...(query ? { title: query } : {}) })}`
      : ""
  }`;

  return (
    <AppFrame error={queryError ? errorMessage(queryError) : undefined}>
      <PageContainer>
        <header className="flex flex-wrap items-start justify-between gap-3">
          <div className="min-w-0">
            <h1 className="truncate text-base font-semibold">
              <Trans>Add</Trans>
            </h1>
          </div>
          <Button asChild variant="outline">
            <Link to={manualHref}>
              <PlusIcon data-icon="inline-start" />
              <Trans>Add manually</Trans>
            </Link>
          </Button>
        </header>

        {!contentWritable ? (
          <Alert>
            {CONTENT_WRITES_DISABLED}{" "}
            <Trans>You can still open entities already in your library.</Trans>
          </Alert>
        ) : null}

        <section className="flex flex-col gap-3 rounded-md border p-4">
          <div className="flex flex-wrap gap-1.5" role="group" aria-label={t`Type`}>
            {searchableTypes.map((type) => {
              const selected = type.id === typeId;
              return (
                <button
                  key={type.id}
                  type="button"
                  aria-pressed={selected}
                  onClick={() => setTypeId(type.id)}
                  className={cn(
                    "rounded-full border px-3 py-1 text-sm font-medium transition-colors",
                    selected
                      ? "border-primary bg-primary text-primary-foreground"
                      : "text-muted-foreground hover:bg-accent hover:text-foreground",
                  )}
                >
                  {type.label}
                </button>
              );
            })}
          </div>
          <div className="grid gap-3 md:grid-cols-[minmax(0,1fr)_180px]">
            <label className="flex flex-col gap-1 text-sm font-medium">
              <Trans>Search</Trans>
              <div className="relative">
                <SearchIcon
                  className="pointer-events-none absolute top-1/2 left-2 size-4 -translate-y-1/2 text-muted-foreground"
                  aria-hidden
                />
                <Input
                  value={rawQuery}
                  onChange={(event) => setRawQuery(event.target.value)}
                  onKeyDown={(event) => {
                    if (event.key === "Enter") {
                      cancelQuery();
                      setQuery(rawQuery.trim());
                    }
                  }}
                  placeholder={t`Title, or paste a provider URL`}
                  className="pl-8"
                  autoFocus
                />
              </div>
            </label>
            <label className="flex flex-col gap-1 text-sm font-medium">
              <Trans>Provider</Trans>
              <Select value={provider} onChange={(event) => setProvider(event.target.value)}>
                <option value={ALL}>{t`All providers`}</option>
                {enabledProviders.map((item) => (
                  <option key={item.id} value={item.id}>
                    {item.label}
                  </option>
                ))}
              </Select>
            </label>
          </div>
        </section>

        {providerProbe.isSuccess && enabledProviders.length === 0 ? (
          <Alert>
            <Trans>
              No search provider is enabled for this type — check its provider credentials in
              Settings, or add the entity manually.
            </Trans>
          </Alert>
        ) : null}

        <ExternalSearchErrors providers={results.data?.providers ?? []} />

        <section className="flex flex-col gap-2">
          {!hasQuery ? (
            <p className="rounded-md border border-dashed p-6 text-center text-sm text-muted-foreground">
              <Trans>Type at least {MIN_QUERY_LENGTH} characters to search.</Trans>
            </p>
          ) : debouncing || results.isPending ? (
            <p className="rounded-md border border-dashed p-6 text-center text-sm text-muted-foreground">
              <Trans>Searching…</Trans>
            </p>
          ) : results.error ? (
            <Alert>{errorMessage(results.error)}</Alert>
          ) : (results.data?.items.length ?? 0) === 0 ? (
            <p className="rounded-md border border-dashed p-6 text-center text-sm text-muted-foreground">
              <Trans>No matches found.</Trans>
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
                              <Trans>In library ✓</Trans>
                            </span>
                          ) : null}
                        </div>
                        {match.candidate.originalTitle &&
                        match.candidate.originalTitle !== match.candidate.title ? (
                          <p className="truncate text-xs text-muted-foreground">
                            {match.candidate.originalTitle}
                          </p>
                        ) : null}
                        {match.candidate.brief ? (
                          <p className="mt-1 line-clamp-2 text-xs text-muted-foreground">
                            {match.candidate.brief}
                          </p>
                        ) : null}
                        <div className="mt-1.5 flex flex-wrap items-center gap-1.5 text-xs text-muted-foreground">
                          <span className="rounded border px-1.5 py-0.5">
                            {providerLabels.get(match.candidate.provider) ??
                              match.candidate.provider}
                          </span>
                          {addingKey === key ? (
                            <span>
                              <Trans>Adding…</Trans>
                            </span>
                          ) : null}
                        </div>
                      </div>
                    </button>
                  </li>
                );
              })}
            </ul>
          )}

          {searchEnabled && (results.data?.items.length ?? 0) > 0 ? (
            <p className="text-center text-xs text-muted-foreground">
              <Trans>
                Search results come from third-party providers and are not affiliated with
                KizunaShelf.
              </Trans>
            </p>
          ) : null}

          {searchEnabled ? (
            <Link
              to={manualHref}
              className="rounded-md border border-dashed p-3 text-center text-sm text-muted-foreground transition-colors hover:bg-accent"
            >
              <Trans>Create “{query}” manually →</Trans>
            </Link>
          ) : null}
        </section>
      </PageContainer>
    </AppFrame>
  );
}
