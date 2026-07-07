import { useCallback, useEffect, useMemo, useState } from "react";
import { useLingui } from "@lingui/react/macro";
import { toast } from "sonner";

import { errorMessage } from "@/api/client";
import { downloadAssets, searchSources } from "@/api/entities";
import { isRemoteAsset } from "@/lib/asset-src";
import {
  externalProviderPriority,
  matchBodyPatch,
  matchBodyPreviewEntries,
  matchFieldPatch,
  matchFieldPreviewEntries,
  matchSelectableBodySections,
  matchSelectableFields,
} from "@/lib/external-metadata";
import { useLanguagePreference } from "@/lib/language";
import type { ExternalMatch, ExternalProviderCatalog, TypeConfig } from "@/types/api";

type ExternalRefs = Record<string, string | undefined>;

function patchValueHasRemote(value: unknown): boolean {
  if (typeof value === "string") return isRemoteAsset(value);
  if (Array.isArray(value)) {
    return value.some((item) => typeof item === "string" && isRemoteAsset(item));
  }
  return false;
}

export function useExternalMatch({
  typeConfig,
  providerCatalog,
  entityType,
  defaultQuery,
  externalRefs,
  assetDownloadEnabled = false,
}: {
  typeConfig?: TypeConfig;
  providerCatalog?: ExternalProviderCatalog;
  entityType?: string;
  defaultQuery?: string;
  externalRefs?: ExternalRefs;
  assetDownloadEnabled?: boolean;
}) {
  const { t } = useLingui();
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [provider, setProvider] = useState("all");
  // The full preference (possibly zh-Hans/zh-Hant): providers that distinguish
  // the scripts localize candidate metadata with it.
  const language = useLanguagePreference();
  const [candidates, setCandidates] = useState<ExternalMatch[]>([]);
  const [searching, setSearching] = useState(false);
  const [selectedCandidate, setSelectedCandidate] = useState<ExternalMatch>();
  const [selectedFields, setSelectedFields] = useState<Set<string>>(new Set());
  const [selectedBodySections, setSelectedBodySections] = useState<Set<string>>(new Set());
  const [emptyMessage, setEmptyMessage] = useState(t`No candidates loaded`);
  const [downloadAfterApply, setDownloadAfterApply] = useState(false);

  // Providers configured for this type by the schema (credential-independent).
  const schemaProviderOptions = useMemo(
    () => externalProviderPriority(providerCatalog, typeConfig),
    [providerCatalog, typeConfig],
  );
  // Per-provider credential availability from the search-response summaries
  // (empty until loaded). Mirrors iOS: providers whose credentials aren't
  // configured are hidden from the picker rather than offered and failing.
  const [providerEnabled, setProviderEnabled] = useState<Record<string, boolean>>({});
  const providerOptions = useMemo(() => {
    if (Object.keys(providerEnabled).length === 0) return schemaProviderOptions;
    return schemaProviderOptions.filter((id) => providerEnabled[id] !== false);
  }, [schemaProviderOptions, providerEnabled]);
  // Whether the type has any external source configured at all — gates whether
  // the match feature is offered (independent of credentials).
  const externalSearchEnabled = schemaProviderOptions.length > 0;
  const metadataEntries = useMemo(
    () => (selectedCandidate ? matchFieldPreviewEntries(selectedCandidate, typeConfig) : []),
    [selectedCandidate, typeConfig],
  );
  const bodyEntries = useMemo(
    () => (selectedCandidate ? matchBodyPreviewEntries(selectedCandidate) : []),
    [selectedCandidate],
  );
  const existingExternalRefs = useMemo(
    () =>
      typeConfig && externalRefs
        ? (typeConfig.fields ?? [])
            .filter((field) => field.fieldType === "externalRef" && field.externalRef)
            .map((field) => ({
              field: field.field,
              provider: field.externalRef ?? "",
              value: externalRefs[field.field],
            }))
            .filter((item): item is { field: string; provider: string; value: string } =>
              Boolean(item.provider && item.value && providerOptions.includes(item.provider)),
            )
        : [],
    [externalRefs, providerOptions, typeConfig],
  );

  // When the match UI opens, load provider availability. An empty query returns
  // the provider summaries (with `enabled`) without hitting any external API, so
  // the picker can hide providers whose credentials aren't configured.
  useEffect(() => {
    if (!open || !entityType || schemaProviderOptions.length === 0) return;
    let cancelled = false;
    void (async () => {
      try {
        const result = await searchSources({ provider: "all", q: "", type: entityType, pageSize: 1 });
        if (!cancelled) {
          setProviderEnabled(Object.fromEntries(result.providers.map((item) => [item.id, item.enabled])));
        }
      } catch {
        // Leave the picker unfiltered on failure rather than blocking matching.
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [open, entityType, schemaProviderOptions.length]);

  useEffect(() => {
    if (providerOptions.length === 0) {
      if (provider !== "all") setProvider("all");
      return;
    }
    if (providerOptions.length === 1) {
      if (provider !== providerOptions[0]) setProvider(providerOptions[0]);
      return;
    }
    if (provider !== "all" && !providerOptions.includes(provider)) {
      setProvider("all");
    }
  }, [provider, providerOptions]);

  const coverDownloadAvailable = useMemo(() => {
    if (!assetDownloadEnabled || !selectedCandidate || !typeConfig) return false;
    const patch = matchFieldPatch(selectedCandidate, selectedFields);
    return (typeConfig.fields ?? []).some(
      (field) =>
        (field.fieldType === "image" || field.fieldType === "imageList") &&
        patchValueHasRemote(patch[field.field]),
    );
  }, [assetDownloadEnabled, selectedCandidate, typeConfig, selectedFields]);

  // Downloads the entity's freshly applied remote cover when the user opted in.
  // Best-effort: a failure is surfaced but does not block the caller's flow.
  const maybeDownloadCover = useCallback(
    async (entity: { id: string; revision: string }) => {
      if (!downloadAfterApply) return;
      try {
        await downloadAssets(entity.id, { revision: entity.revision });
      } catch (error) {
        toast.error(errorMessage(error));
      } finally {
        setDownloadAfterApply(false);
      }
    },
    [downloadAfterApply],
  );

  const resetSelection = useCallback(() => {
    setSelectedCandidate(undefined);
    setSelectedFields(new Set());
    setSelectedBodySections(new Set());
  }, []);

  const search = useCallback(
    async (providerOverride?: string, queryOverride?: string) => {
      const selectedProvider = providerOverride ?? provider;
      const selectedQuery = (queryOverride ?? query).trim() || defaultQuery?.trim() || "";
      if (!selectedQuery || !entityType || !externalSearchEnabled) return;
      if (selectedProvider !== "all" && !providerOptions.includes(selectedProvider)) return;

      setSearching(true);
      setEmptyMessage(t`No candidates loaded`);
      resetSelection();
      try {
        const result = await searchSources({
          provider: selectedProvider,
          q: selectedQuery,
          type: entityType,
          pageSize: 8,
          language,
        });
        setProviderEnabled(Object.fromEntries(result.providers.map((item) => [item.id, item.enabled])));
        setCandidates(result.items);
        if (result.items.length === 0) setEmptyMessage(t`No external matches`);
      } catch (error) {
        toast.error(errorMessage(error));
      } finally {
        setSearching(false);
      }
    },
    [provider, query, defaultQuery, entityType, externalSearchEnabled, providerOptions, resetSelection, language, t],
  );

  const refreshFromExternalRef = useCallback(
    (nextProvider: string, value: string) => {
      setOpen(true);
      setProvider(nextProvider);
      setQuery(value);
      void search(nextProvider, value);
    },
    [search],
  );

  const chooseCandidate = useCallback((match: ExternalMatch) => {
    setSelectedCandidate(match);
    setSelectedFields(new Set(matchSelectableFields(match)));
    setSelectedBodySections(new Set(matchSelectableBodySections(match)));
  }, []);

  const selectedPatch = useCallback(() => {
    if (!selectedCandidate) return {};
    return matchFieldPatch(selectedCandidate, selectedFields);
  }, [selectedCandidate, selectedFields]);

  const selectedBodyPatch = useCallback(() => {
    if (!selectedCandidate) return [];
    return matchBodyPatch(selectedCandidate, selectedBodySections);
  }, [selectedCandidate, selectedBodySections]);

  return {
    open,
    setOpen,
    query,
    setQuery,
    provider,
    setProvider,
    candidates,
    searching,
    selectedCandidate,
    selectedFields,
    setSelectedFields,
    selectedBodySections,
    setSelectedBodySections,
    providerOptions,
    externalSearchEnabled,
    metadataEntries,
    bodyEntries,
    existingExternalRefs,
    emptyMessage,
    coverDownloadAvailable,
    downloadAfterApply,
    setDownloadAfterApply,
    maybeDownloadCover,
    search,
    refreshFromExternalRef,
    chooseCandidate,
    selectedPatch,
    selectedBodyPatch,
  };
}
