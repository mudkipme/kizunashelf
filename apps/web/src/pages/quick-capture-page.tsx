import { useEffect, useMemo, useState } from "react";
import { plural } from "@lingui/core/macro";
import { Trans, useLingui } from "@lingui/react/macro";
import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { PlusIcon, SearchIcon } from "lucide-react";
import { Link, useNavigate, useSearchParams } from "react-router-dom";
import { toast } from "sonner";

import { errorMessage } from "@/api/client";
import { quickAddEntity, searchSources } from "@/api/entities";
import { useInvalidateEntityData } from "@/api/invalidate-entity-data";
import { configQuery, providerCatalogQuery } from "@/api/queries";
import { AppFrame } from "@/components/layout/app-frame";
import { PageContainer } from "@/components/layout/page-container";
import { Alert } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import { CONTENT_WRITES_DISABLED, useCapabilities } from "@/lib/capabilities";
import { useLanguagePreference } from "@/lib/language";
import { typeExternalRefs } from "@/lib/type-config";
import type { ExternalMatch } from "@/types/api";

const ALL = "all";
const MIN_QUERY_LENGTH = 2;

// A candidate's stable identity across a result set (a work can appear once per
// type in cross-type search, so the type is part of the key).
function matchKey(match: ExternalMatch): string {
  return `${match.candidate.provider}:${match.candidate.sourceId}:${match.entityType}`;
}

export function QuickCapturePage() {
  const { t } = useLingui();
  const navigate = useNavigate();
  const [searchParams] = useSearchParams();
  const requestedType = searchParams.get("type") ?? ALL;
  const invalidateEntityData = useInvalidateEntityData();
  const config = useQuery(configQuery());
  const providerCatalog = useQuery(providerCatalogQuery());
  const capabilities = useCapabilities();
  const contentWritable = capabilities.contentWritable;
  // The full preference (possibly zh-Hans/zh-Hant): providers that distinguish
  // the scripts localize search results and quick-add episode titles with it.
  const language = useLanguagePreference();

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
    return types.filter((type) => typeExternalRefs(type).some((ref) => providerIds.has(ref)));
  }, [config.data, providerCatalog.data]);

  // If the selected type isn't searchable (e.g. arrived via `?type=`), fall back to
  // All so the picker stays valid rather than showing a hidden value.
  useEffect(() => {
    if (typeId !== ALL && providerCatalog.data && !searchableTypes.some((type) => type.id === typeId)) {
      setTypeId(ALL);
    }
  }, [typeId, providerCatalog.data, searchableTypes]);

  // Empty-query probes return per-provider `enabled` summaries without hitting any
  // provider network. The gate probe spans every type (is Quick Capture usable at
  // all?); the provider-list probe is scoped to the selected type, so the dropdown
  // lists only the providers that type's `externalRef` fields map to.
  const gateProbe = useQuery({
    queryKey: ["externalSearch", "probe", ALL],
    queryFn: ({ signal }) => searchSources({ type: ALL, q: "" }, { signal }),
    staleTime: 60_000,
  });
  const anyProviderEnabled = (gateProbe.data?.providers ?? []).some((item) => item.enabled);

  // Keyed by the selected type; when it is "all" this matches the gate probe's key
  // so React Query serves it from the same fetch.
  const providerProbe = useQuery({
    queryKey: ["externalSearch", "probe", typeId],
    queryFn: ({ signal }) => searchSources({ type: typeId, q: "" }, { signal }),
    staleTime: 60_000,
  });
  const enabledProviders = useMemo(
    () => (providerProbe.data?.providers ?? []).filter((item) => item.enabled),
    [providerProbe.data],
  );

  // No usable provider anywhere → Quick Capture has nothing to search; fall back to
  // the manual add page (which stays available in read-only mode too).
  useEffect(() => {
    if (gateProbe.isSuccess && !anyProviderEnabled) {
      navigate(`/entities/new/manual${requestedType !== ALL ? `?type=${encodeURIComponent(requestedType)}` : ""}`, {
        replace: true,
      });
    }
  }, [gateProbe.isSuccess, anyProviderEnabled, navigate, requestedType]);

  // A provider chosen for one type may not exist under the next; reset to All so
  // the search doesn't silently return nothing.
  useEffect(() => {
    if (provider !== ALL && providerProbe.isSuccess && !enabledProviders.some((item) => item.id === provider)) {
      setProvider(ALL);
    }
  }, [provider, enabledProviders, providerProbe.isSuccess]);

  const searchEnabled = query.length >= MIN_QUERY_LENGTH;
  const results = useQuery({
    queryKey: ["externalSearch", "results", { query, typeId, provider, language }],
    queryFn: ({ signal }) =>
      searchSources({ type: typeId, provider, q: query, pageSize: 15, language }, { signal }),
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
      const result = await quickAddEntity({ type: match.entityType, candidate: match.candidate, language });
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
        toast.info(t`A file named for this title already existed, so it was saved as “${result.entity.basename}”.`);
      }
      await invalidateEntityData();
      navigate(`/entities/${encodeURIComponent(result.entity.id)}`);
    } catch (error) {
      toast.error(errorMessage(error));
      setAddingKey(undefined);
    }
  }

  const queryError = config.error ?? capabilities.error ?? gateProbe.error ?? providerProbe.error;
  const manualHref = `/entities/new/manual${
    typeId !== ALL || query ? `?${new URLSearchParams({ ...(typeId !== ALL ? { type: typeId } : {}), ...(query ? { title: query } : {}) })}` : ""
  }`;

  return (
    <AppFrame error={queryError ? errorMessage(queryError) : undefined}>
      <PageContainer>
        <header className="flex flex-wrap items-start justify-between gap-3">
          <div className="min-w-0">
            <h1 className="truncate text-base font-semibold">
              <Trans>Quick Capture</Trans>
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
            {CONTENT_WRITES_DISABLED} <Trans>You can still open items already in your library.</Trans>
          </Alert>
        ) : null}

        <section className="rounded-md border p-4">
          <div className="grid gap-3 md:grid-cols-[minmax(0,1fr)_180px_180px]">
            <label className="flex flex-col gap-1 text-sm font-medium">
              <Trans>Search</Trans>
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
                  placeholder={t`Title, or paste a provider URL`}
                  className="pl-8"
                  autoFocus
                />
              </div>
            </label>
            <label className="flex flex-col gap-1 text-sm font-medium">
              <Trans>Type</Trans>
              <Select value={typeId} onChange={(event) => setTypeId(event.target.value)}>
                <option value={ALL}>{t`All types`}</option>
                {searchableTypes.map((type) => (
                  <option key={type.id} value={type.id}>
                    {type.label}
                  </option>
                ))}
              </Select>
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

        {providerErrors.length > 0 ? (
          <ul className="flex flex-col gap-1 rounded-md border border-amber-500/40 bg-amber-500/10 p-3 text-xs text-amber-700 dark:text-amber-400">
            {providerErrors.map((item) => (
              <li key={item.id}>
                {/* item.error is the provider's own (server) failure text — HTTP
                    status + body — surfaced verbatim, like other ApiError messages. */}
                <span className="font-medium">{item.label}</span>: {item.error}
              </li>
            ))}
          </ul>
        ) : null}

        <section className="flex flex-col gap-2">
          {!searchEnabled ? (
            <p className="rounded-md border border-dashed p-6 text-center text-sm text-muted-foreground">
              <Trans>Type at least {MIN_QUERY_LENGTH} characters to search.</Trans>
            </p>
          ) : results.isPending ? (
            <p className="rounded-md border border-dashed p-6 text-center text-sm text-muted-foreground">
              <Trans>Searching…</Trans>
            </p>
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
                        {match.candidate.originalTitle && match.candidate.originalTitle !== match.candidate.title ? (
                          <p className="truncate text-xs text-muted-foreground">{match.candidate.originalTitle}</p>
                        ) : null}
                        {match.candidate.brief ? (
                          <p className="mt-1 line-clamp-2 text-xs text-muted-foreground">{match.candidate.brief}</p>
                        ) : null}
                        <div className="mt-1.5 flex flex-wrap items-center gap-1.5 text-xs text-muted-foreground">
                          <span className="rounded border px-1.5 py-0.5">
                            {providerLabels.get(match.candidate.provider) ?? match.candidate.provider}
                          </span>
                          <span className="rounded border px-1.5 py-0.5">
                            {typeLabels.get(match.entityType) ?? match.entityType}
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
