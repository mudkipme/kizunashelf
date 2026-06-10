import { useEffect, useMemo, useState } from "react";

import { errorMessage } from "@/api/client";
import { searchSources } from "@/api/entities";
import {
  candidateMetadataEntries,
  candidateMetadataPatch,
  candidateMetadataPreviewEntries,
  externalProviderPriority,
} from "@/lib/external-metadata";
import type { ExternalCandidate, ExternalProviderCatalog, TypeConfig } from "@/types/api";

type ExternalRefs = Record<string, string | undefined>;

export function useExternalMatch({
  typeConfig,
  providerCatalog,
  entityType,
  defaultQuery,
  externalRefs,
  onError,
}: {
  typeConfig?: TypeConfig;
  providerCatalog?: ExternalProviderCatalog;
  entityType?: string;
  defaultQuery?: string;
  externalRefs?: ExternalRefs;
  onError: (message: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [provider, setProvider] = useState("all");
  const [candidates, setCandidates] = useState<ExternalCandidate[]>([]);
  const [searching, setSearching] = useState(false);
  const [selectedCandidate, setSelectedCandidate] = useState<ExternalCandidate>();
  const [selectedFields, setSelectedFields] = useState<Set<string>>(new Set());
  const [emptyMessage, setEmptyMessage] = useState("No candidates loaded");

  const providerOptions = useMemo(
    () => externalProviderPriority(providerCatalog, typeConfig),
    [providerCatalog, typeConfig],
  );
  const externalSearchEnabled = providerOptions.length > 0;
  const metadataEntries = useMemo(
    () => (selectedCandidate ? candidateMetadataPreviewEntries(selectedCandidate, typeConfig) : []),
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

  function resetSelection() {
    setSelectedCandidate(undefined);
    setSelectedFields(new Set());
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
  }

  function selectedPatch() {
    if (!selectedCandidate) return {};
    return candidateMetadataPatch(selectedCandidate, typeConfig, selectedFields);
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
    providerOptions,
    externalSearchEnabled,
    metadataEntries,
    existingExternalRefs,
    emptyMessage,
    search,
    refreshFromExternalRef,
    chooseCandidate,
    selectedPatch,
  };
}
