import { useLingui } from "@lingui/react/macro";
import { useQuery } from "@tanstack/react-query";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { toast } from "sonner";

import { errorMessage } from "@/api/client";
import { downloadAssets, reviewMatch } from "@/api/entities";
import { externalProvidersQuery, externalSearchQuery } from "@/api/queries";
import { isRemoteAsset } from "@/lib/asset-src";
import {
  externalProviderPriority,
  matchFieldPatch,
  matchFieldPreviewEntries,
} from "@/lib/external-metadata";
import { useLanguagePreference } from "@/lib/language";
import type {
  ExternalMatch,
  ExternalProviderCatalog,
  ExternalReviewResponse,
  TypeConfig,
} from "@/types/api";

type ExternalRefs = Record<string, string | undefined>;

/**
 * The chosen candidate and its review, as one state. The core's review supplies
 * the default selection and the locked fields, so the user can only choose what
 * to apply once it has landed (`ready`). Before then, a toggle would be
 * overwritten by the review's defaults, and Apply would skip the locks.
 */
export type CandidateSelection =
  | { status: "none" }
  | { status: "reviewing"; candidate: ExternalMatch }
  | { status: "failed"; candidate: ExternalMatch }
  | {
      status: "ready";
      candidate: ExternalMatch;
      review: ExternalReviewResponse;
      fields: ReadonlySet<string>;
      sections: ReadonlySet<string>;
    };

export type CandidateReviewStatus = CandidateSelection["status"];

const NO_SELECTION: CandidateSelection = { status: "none" };
const EMPTY_SET: ReadonlySet<string> = new Set();

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
  entityId,
  entityType,
  defaultQuery,
  externalRefs,
  assetDownloadEnabled = false,
}: {
  typeConfig?: TypeConfig;
  providerCatalog?: ExternalProviderCatalog;
  // The entity a chosen candidate is reviewed against. The core computes the
  // default selection (empty/ref on, same/existing off) and locked flags via
  // `reviewExternalCandidate`, so the hook no longer needs the entity's
  // frontmatter/body.
  entityId?: string;
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
  const [request, setRequest] = useState<{
    entityId?: string;
    params: Parameters<typeof externalSearchQuery>[0];
  }>();
  const activeRequest =
    open &&
    request?.entityId === entityId &&
    request?.params.type === entityType &&
    request?.params.language === language
      ? request
      : undefined;
  const results = useQuery({
    ...externalSearchQuery(activeRequest?.params ?? { type: "", q: "" }),
    enabled: Boolean(activeRequest),
  });
  const { refetch: refetchResults } = results;
  const candidates = results.data?.items ?? [];
  const searching = Boolean(activeRequest) && results.isFetching;
  const emptyMessage = searching
    ? t`Searching…`
    : activeRequest
      ? t`No external matches`
      : t`Search for a match`;
  useEffect(() => {
    if (results.error) toast.error(errorMessage(results.error));
  }, [results.error]);
  const [selection, setSelection] = useState<CandidateSelection>(NO_SELECTION);
  // Bumped whenever the selection is replaced or dropped, so a review resolving
  // for an earlier choice (or after close/entity change) is ignored.
  const reviewToken = useRef(0);
  const selectedCandidate = selection.status === "none" ? undefined : selection.candidate;
  // The core's per-candidate review (current values, default selection, locked
  // flags, replace-vs-append), present only once it has landed.
  const review = selection.status === "ready" ? selection.review : undefined;
  const selectedFields = selection.status === "ready" ? selection.fields : EMPTY_SET;
  const selectedBodySections = selection.status === "ready" ? selection.sections : EMPTY_SET;
  const [downloadAfterApply, setDownloadAfterApply] = useState(false);

  // Providers configured for this type by the schema (credential-independent).
  const schemaProviderOptions = useMemo(
    () => externalProviderPriority(providerCatalog, typeConfig),
    [providerCatalog, typeConfig],
  );
  const availability = useQuery({
    ...externalProvidersQuery(entityType ?? ""),
    enabled: open && Boolean(entityType) && schemaProviderOptions.length > 0,
  });
  const providerOptions = useMemo(() => {
    const summaries = availability.data?.providers;
    if (!summaries) return schemaProviderOptions;
    return schemaProviderOptions.filter((id) =>
      summaries.some((item) => item.id === id && item.enabled),
    );
  }, [schemaProviderOptions, availability.data]);
  // Whether the type has any external source configured at all — gates whether
  // the match feature is offered (independent of credentials).
  const externalSearchEnabled = schemaProviderOptions.length > 0;
  // The review's fields/sections carry the enriched candidate's values (search
  // results are deliberately thin; the core resolves provider detail during
  // review), so once it lands they supersede the search-time mapping. Until
  // then the thin mapping keeps the panel from flashing empty.
  const effectiveFields = review?.fields ?? selectedCandidate?.fields;
  // Apply sends the review's enriched candidate (its enrichment marker already
  // consumed server-side) so provider detail is fetched once per selection.
  // Nothing is applicable until the review lands.
  const candidateForApply = review?.candidate;
  const metadataEntries = useMemo(
    () => (selectedCandidate ? matchFieldPreviewEntries(effectiveFields, typeConfig) : []),
    [selectedCandidate, effectiveFields, typeConfig],
  );
  const bodyEntries = useMemo(
    () => (selectedCandidate ? (review?.sections ?? selectedCandidate.bodySections ?? []) : []),
    [selectedCandidate, review],
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
    const patch = matchFieldPatch(effectiveFields, selectedFields);
    return (typeConfig.fields ?? []).some(
      (field) =>
        (field.fieldType === "image" || field.fieldType === "imageList") &&
        patchValueHasRemote(patch[field.field]),
    );
  }, [assetDownloadEnabled, selectedCandidate, typeConfig, selectedFields, effectiveFields]);

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
    reviewToken.current += 1;
    setSelection(NO_SELECTION);
  }, []);

  // Dropping the active query detaches its observer and aborts the request.
  // Review tokens also expire on close, entity changes, and unmount.
  useEffect(() => {
    setRequest(undefined);
    resetSelection();
    return () => {
      reviewToken.current += 1;
    };
  }, [entityId, entityType, language, resetSelection]);

  const changeOpen = useCallback(
    (next: boolean) => {
      setOpen(next);
      if (!next) {
        setRequest(undefined);
        resetSelection();
      }
    },
    [resetSelection],
  );

  const changeQuery = useCallback(
    (next: string) => {
      setQuery(next);
      setRequest(undefined);
      resetSelection();
    },
    [resetSelection],
  );

  const changeProvider = useCallback(
    (next: string) => {
      setProvider(next);
      setRequest(undefined);
      resetSelection();
    },
    [resetSelection],
  );

  const search = useCallback(
    (providerOverride?: string, queryOverride?: string) => {
      const selectedProvider = providerOverride ?? provider;
      const selectedQuery = (queryOverride ?? query).trim() || defaultQuery?.trim() || "";
      if (!selectedQuery || !entityType || !externalSearchEnabled) return;
      if (selectedProvider !== "all" && !providerOptions.includes(selectedProvider)) return;
      resetSelection();
      if (
        activeRequest?.params.provider === selectedProvider &&
        activeRequest.params.q === selectedQuery
      ) {
        void refetchResults();
      } else {
        setRequest({
          entityId,
          params: {
            provider: selectedProvider,
            q: selectedQuery,
            type: entityType,
            pageSize: 8,
            language,
          },
        });
      }
    },
    [
      provider,
      query,
      defaultQuery,
      entityId,
      entityType,
      externalSearchEnabled,
      providerOptions,
      resetSelection,
      language,
      activeRequest,
      refetchResults,
    ],
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

  // Choosing a candidate asks the core to review it against the entity; the
  // response seeds the default selection. A late-resolving review for a
  // previously chosen candidate is dropped via the token.
  const chooseCandidate = useCallback(
    (match: ExternalMatch) => {
      if (!entityId) return;
      const token = ++reviewToken.current;
      setSelection({ status: "reviewing", candidate: match });
      void (async () => {
        try {
          const result = await reviewMatch(entityId, { candidate: match.candidate, language });
          if (token !== reviewToken.current) return;
          setSelection({
            status: "ready",
            candidate: match,
            review: result,
            fields: new Set(
              result.fields.filter((field) => field.selected).map((field) => field.field),
            ),
            sections: new Set(
              result.sections.filter((section) => section.selected).map((section) => section.key),
            ),
          });
        } catch (error) {
          if (token !== reviewToken.current) return;
          setSelection({ status: "failed", candidate: match });
          toast.error(errorMessage(error));
        }
      })();
    },
    [entityId, language],
  );

  const retryReview = useCallback(() => {
    if (selection.status === "failed") chooseCandidate(selection.candidate);
  }, [selection, chooseCandidate]);

  // The user's choices only exist on a landed review; toggles before then have
  // nothing to apply to (the controls are disabled meanwhile).
  const setSelectedFields = useCallback((fields: Set<string>) => {
    setSelection((current) => (current.status === "ready" ? { ...current, fields } : current));
  }, []);
  const setSelectedBodySections = useCallback((sections: Set<string>) => {
    setSelection((current) => (current.status === "ready" ? { ...current, sections } : current));
  }, []);

  // Locked/replace-vs-append flags for the dialog, straight from the review.
  const fieldLocks = useMemo(
    () =>
      new Set((review?.fields ?? []).filter((field) => field.locked).map((field) => field.field)),
    [review],
  );
  const sectionLocks = useMemo(
    () =>
      new Set(
        (review?.sections ?? []).filter((section) => section.locked).map((section) => section.key),
      ),
    [review],
  );
  const sectionModes = useMemo(
    () =>
      review
        ? Object.fromEntries(
            review.sections.map(
              (section) => [section.key, section.exists ? "replace" : "append"] as const,
            ),
          )
        : undefined,
    [review],
  );

  return {
    open,
    setOpen: changeOpen,
    query,
    setQuery: changeQuery,
    provider,
    setProvider: changeProvider,
    candidates,
    providers: results.data?.providers ?? [],
    searching,
    selectedCandidate,
    reviewStatus: selection.status,
    retryReview,
    candidateForApply,
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
    fieldLocks,
    sectionLocks,
    sectionModes,
  };
}
