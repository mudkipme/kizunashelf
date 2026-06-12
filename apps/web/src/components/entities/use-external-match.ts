import { useEffect, useMemo, useState } from "react";

import { errorMessage } from "@/api/client";
import { downloadAssets, searchSources } from "@/api/entities";
import { isRemoteAsset } from "@/lib/asset-src";
import {
  candidateBodyPatch,
  candidateBodyPreviewEntries,
  candidateMetadataEntries,
  candidateMetadataPatch,
  candidateMetadataPreviewEntries,
  externalProviderPriority,
} from "@/lib/external-metadata";
import type { ExternalCandidate, ExternalProviderCatalog, TypeConfig } from "@/types/api";

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
  onError,
}: {
  typeConfig?: TypeConfig;
  providerCatalog?: ExternalProviderCatalog;
  entityType?: string;
  defaultQuery?: string;
  externalRefs?: ExternalRefs;
  assetDownloadEnabled?: boolean;
  onError: (message: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [provider, setProvider] = useState("all");
  const [candidates, setCandidates] = useState<ExternalCandidate[]>([]);
  const [searching, setSearching] = useState(false);
  const [selectedCandidate, setSelectedCandidate] = useState<ExternalCandidate>();
  const [selectedFields, setSelectedFields] = useState<Set<string>>(new Set());
  const [selectedBodySections, setSelectedBodySections] = useState<Set<string>>(new Set());
  const [emptyMessage, setEmptyMessage] = useState("No candidates loaded");
  const [downloadAfterApply, setDownloadAfterApply] = useState(false);

  const providerOptions = useMemo(
    () => externalProviderPriority(providerCatalog, typeConfig),
    [providerCatalog, typeConfig],
  );
  const externalSearchEnabled = providerOptions.length > 0;
  const metadataEntries = useMemo(
    () => (selectedCandidate ? candidateMetadataPreviewEntries(selectedCandidate, typeConfig) : []),
    [selectedCandidate, typeConfig],
  );
  const bodyEntries = useMemo(
    () => (selectedCandidate ? candidateBodyPreviewEntries(selectedCandidate, typeConfig) : []),
    [selectedCandidate, typeConfig],
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
    const patch = candidateMetadataPatch(selectedCandidate, typeConfig, selectedFields) as Record<
      string,
      unknown
    >;
    return (typeConfig.fields ?? []).some(
      (field) =>
        (field.fieldType === "image" || field.fieldType === "imageList") &&
        patchValueHasRemote(patch[field.field]),
    );
  }, [assetDownloadEnabled, selectedCandidate, typeConfig, selectedFields]);

  // Downloads the entity's freshly applied remote cover when the user opted in.
  // Best-effort: a failure is surfaced but does not block the caller's flow.
  async function maybeDownloadCover(entity: { id: string; revision: string }) {
    if (!downloadAfterApply) return;
    try {
      await downloadAssets(entity.id, { revision: entity.revision });
    } catch (error) {
      onError(errorMessage(error));
    } finally {
      setDownloadAfterApply(false);
    }
  }

  function resetSelection() {
    setSelectedCandidate(undefined);
    setSelectedFields(new Set());
    setSelectedBodySections(new Set());
  }

  async function search(providerOverride?: string, queryOverride?: string) {
    const selectedProvider = providerOverride ?? provider;
    const selectedQuery = (queryOverride ?? query).trim() || defaultQuery?.trim() || "";
    if (!selectedQuery || !entityType || !externalSearchEnabled) return;
    if (selectedProvider !== "all" && !providerOptions.includes(selectedProvider)) return;

    setSearching(true);
    setEmptyMessage("No candidates loaded");
    resetSelection();
    try {
      const result = await searchSources({
        provider: selectedProvider,
        q: selectedQuery,
        type: entityType,
        pageSize: 8,
      });
      setCandidates(result.items);
      if (result.items.length === 0) setEmptyMessage("No external matches");
    } catch (error) {
      onError(errorMessage(error));
    } finally {
      setSearching(false);
    }
  }

  function refreshFromExternalRef(provider: string, value: string) {
    setOpen(true);
    setProvider(provider);
    setQuery(value);
    void search(provider, value);
  }

  function chooseCandidate(candidate: ExternalCandidate) {
    setSelectedCandidate(candidate);
    setSelectedFields(new Set(candidateMetadataEntries(candidate, typeConfig).map((entry) => entry.field)));
    setSelectedBodySections(
      new Set(
        candidateBodyPreviewEntries(candidate, typeConfig)
          .filter((entry) => entry.hasValue)
          .map((entry) => entry.key),
      ),
    );
  }

  function selectedPatch() {
    if (!selectedCandidate) return {};
    return candidateMetadataPatch(selectedCandidate, typeConfig, selectedFields);
  }

  function selectedBodyPatch() {
    if (!selectedCandidate) return [];
    return candidateBodyPatch(selectedCandidate, typeConfig, selectedBodySections);
  }

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
