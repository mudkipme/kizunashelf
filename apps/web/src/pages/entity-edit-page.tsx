import { useCallback, useEffect, useMemo, useState } from "react";
import { getConfig, getEntities, getEntity } from "@kizunashelf/api-contract";
import { ArrowLeftIcon, SearchIcon } from "lucide-react";
import { useNavigate, useParams } from "react-router-dom";

import { apiFetch, errorMessage, isAbortError } from "@/api/client";
import { getAppCapabilities, saveEntity, searchSources } from "@/api/entities";
import { ExternalMatchDialog } from "@/components/entities/external-match-dialog";
import {
  type FrontmatterDraft,
  frontmatterPatch,
  MetadataEditor,
  normalizeFrontmatter,
} from "@/components/entities/metadata-editor";
import { AppFrame } from "@/components/layout/app-frame";
import { Button } from "@/components/ui/button";
import {
  candidateMetadataEntries,
  candidateMetadataPatch,
  candidateMetadataPreviewEntries,
  externalProviderPriority,
} from "@/lib/external-metadata";
import type {
  Capabilities,
  ConfigResponse,
  EntityDetailResponse,
  EntitySummary,
  ExternalCandidate,
} from "@/types/api";

type EditState = {
  detail?: EntityDetailResponse;
  config?: ConfigResponse;
  capabilities?: Capabilities;
  relationSuggestions: EntitySummary[];
  loading: boolean;
  error?: string;
};

export function EntityEditPage() {
  const { id } = useParams();
  const navigate = useNavigate();
  const [state, setState] = useState<EditState>({ loading: true, relationSuggestions: [] });
  const [frontmatter, setFrontmatter] = useState<FrontmatterDraft>({});
  const [body, setBody] = useState("");
  const [saving, setSaving] = useState(false);
  const [matchOpen, setMatchOpen] = useState(false);
  const [externalQuery, setExternalQuery] = useState("");
  const [externalProvider, setExternalProvider] = useState("all");
  const [externalCandidates, setExternalCandidates] = useState<ExternalCandidate[]>([]);
  const [externalSearching, setExternalSearching] = useState(false);
  const [selectedCandidate, setSelectedCandidate] = useState<ExternalCandidate>();
  const [selectedFields, setSelectedFields] = useState<Set<string>>(new Set());
  const entity = state.detail?.entity;
  const contentWritable = state.capabilities?.contentWritable !== false;
  const typeConfig = useMemo(
    () => state.config?.types.find((type) => type.id === entity?.type),
    [state.config, entity?.type],
  );
  const selectedCandidateEntries = useMemo(
    () => (selectedCandidate ? candidateMetadataPreviewEntries(selectedCandidate, typeConfig) : []),
    [selectedCandidate, typeConfig],
  );
  const providerOptions = useMemo(() => externalProviderPriority(typeConfig), [typeConfig]);
  const externalSearchEnabled = providerOptions.length > 0;
  const existingExternalRefs = useMemo(
    () =>
      typeConfig && entity
        ? (typeConfig.fields ?? [])
            .filter((field) => field.fieldType === "externalRef" && field.externalRef)
            .map((field) => ({
              field: field.field,
              provider: field.externalRef ?? "",
              value: entity.externalRefs[field.field],
            }))
            .filter((item): item is { field: string; provider: string; value: string } =>
              Boolean(item.provider && item.value && providerOptions.includes(item.provider)),
            )
        : [],
    [entity, providerOptions, typeConfig],
  );

  useEffect(() => {
    if (!id) return;
    const controller = new AbortController();
    void load(controller.signal);
    return () => controller.abort();
  }, [id]);

  useEffect(() => {
    if (!entity) return;
    setFrontmatter(normalizeFrontmatter(entity.frontmatter));
    setBody(entity.body);
    setExternalQuery(entity.title);
  }, [entity?.id, entity?.revision]);

  useEffect(() => {
    if (providerOptions.length === 0) {
      if (externalProvider !== "all") setExternalProvider("all");
      return;
    }
    if (providerOptions.length === 1) {
      if (externalProvider !== providerOptions[0]) setExternalProvider(providerOptions[0]);
      return;
    }
    if (externalProvider !== "all" && !providerOptions.includes(externalProvider)) {
      setExternalProvider("all");
    }
  }, [externalProvider, providerOptions]);

  async function load(signal: AbortSignal) {
    if (!id) return;
    setState({ loading: true, relationSuggestions: [] });
    try {
      const [detail, config, capabilities] = await Promise.all([
        getEntity(id, { signal }, apiFetch),
        getConfig({ signal }, apiFetch),
        getAppCapabilities({ signal }),
      ]);
      setState({
        detail,
        config,
        capabilities,
        relationSuggestions: [],
        loading: false,
      });
    } catch (error) {
      if (isAbortError(error)) return;
      setState({ loading: false, relationSuggestions: [], error: errorMessage(error) });
    }
  }

  const searchRelations = useCallback(async ({ relationType, query, signal }: {
    relationType?: string | null;
    query: string;
    signal: AbortSignal;
  }) => {
    const type = relationType?.trim();
    if (!type) return [];
    const result = await getEntities(
      {
        type,
        q: query.trim() || undefined,
        pageSize: 25,
        sort: "title",
        direction: "asc",
      },
      { signal },
      apiFetch,
    );
    return result.items;
  }, []);

  async function save() {
    if (!entity || !contentWritable) return;
    setSaving(true);
    setState((current) => ({ ...current, error: undefined }));
    try {
      const result = await saveEntity(entity.id, {
        revision: entity.revision,
        frontmatter: frontmatterPatch(entity.frontmatter, frontmatter),
        body,
      });
      navigate(`/entities/${encodeURIComponent(result.entity.id)}`);
    } catch (error) {
      setState((current) => ({ ...current, error: errorMessage(error) }));
    } finally {
      setSaving(false);
    }
  }

  function cancel() {
    if (entity) navigate(`/entities/${encodeURIComponent(entity.id)}`);
    else navigate("/library");
  }

  async function searchExternal(providerOverride?: string, queryOverride?: string) {
    if (!entity) return;
    const selectedProvider = providerOverride ?? externalProvider;
    const selectedQuery = queryOverride ?? externalQuery;
    if (providerOptions.length === 0) return;
    if (selectedProvider !== "all" && !providerOptions.includes(selectedProvider)) return;
    setExternalSearching(true);
    setSelectedCandidate(undefined);
    setSelectedFields(new Set());
    setState((current) => ({ ...current, error: undefined }));
    try {
      const result = await searchSources({
        provider: selectedProvider,
        q: selectedQuery || entity.title,
        type: entity.type,
        pageSize: 8,
      });
      setExternalCandidates(result.items);
    } catch (error) {
      setState((current) => ({ ...current, error: errorMessage(error) }));
    } finally {
      setExternalSearching(false);
    }
  }

  function refreshFromExternalRef(provider: string, value: string) {
    setMatchOpen(true);
    setExternalProvider(provider);
    setExternalQuery(value);
    void searchExternal(provider, value);
  }

  function chooseCandidate(candidate: ExternalCandidate) {
    setSelectedCandidate(candidate);
    setSelectedFields(new Set(candidateMetadataEntries(candidate, typeConfig).map((entry) => entry.field)));
  }

  function applyCandidate() {
    if (!selectedCandidate || !contentWritable) return;
    const next = {
      ...frontmatter,
      ...candidateMetadataPatch(selectedCandidate, typeConfig, selectedFields),
    };
    setFrontmatter(normalizeFrontmatter(next));
    setExternalQuery(selectedCandidate.title);
    setMatchOpen(false);
  }

  return (
    <AppFrame error={state.error}>
      <div className="mx-auto flex w-full max-w-5xl flex-col gap-4 p-4">
        <header className="flex flex-wrap items-start justify-between gap-3">
          <div className="min-w-0">
            <h1 className="truncate text-base font-semibold">
              {entity ? `Edit ${entity.title}` : "Edit Entity"}
            </h1>
            <p className="mt-1 truncate text-xs text-muted-foreground">
              {entity?.path ?? "Loading entity"}
            </p>
          </div>
          <div className="flex flex-wrap gap-2">
            <Button
              type="button"
              variant="outline"
              onClick={() => setMatchOpen(true)}
              disabled={!externalSearchEnabled}
            >
              <SearchIcon data-icon="inline-start" />
              Match
            </Button>
            <Button type="button" variant="outline" onClick={cancel} disabled={saving}>
              <ArrowLeftIcon data-icon="inline-start" />
              Back
            </Button>
          </div>
        </header>

        {!contentWritable ? (
          <div className="rounded-md border border-destructive/40 bg-destructive/10 p-3 text-sm text-destructive">
            Content writes are disabled.
          </div>
        ) : null}

        {state.loading ? (
          <div className="rounded-md border p-8 text-center text-sm text-muted-foreground">Loading</div>
        ) : entity ? (
          <>
            <ExternalMatchDialog
              open={matchOpen}
              query={externalQuery}
              provider={externalProvider}
              candidates={externalCandidates}
              selectedCandidate={selectedCandidate}
              metadataEntries={selectedCandidateEntries}
              selectedFields={selectedFields}
              providerOptions={providerOptions}
              externalSearchEnabled={externalSearchEnabled}
              existingExternalRefs={existingExternalRefs}
              currentValues={frontmatter}
              searching={externalSearching}
              applying={false}
              contentWritable={contentWritable}
              applyLabel="Use Selected"
              onOpenChange={setMatchOpen}
              onQueryChange={setExternalQuery}
              onProviderChange={setExternalProvider}
              onSearch={searchExternal}
              onRefreshRef={refreshFromExternalRef}
              onChooseCandidate={chooseCandidate}
              onSelectedFieldsChange={setSelectedFields}
              onApply={applyCandidate}
            />
            <MetadataEditor
              title="Metadata"
              path={entity.path}
              typeConfig={typeConfig}
              frontmatter={frontmatter}
              bodyText={body}
              saving={saving}
              disabled={!contentWritable}
              relationSuggestions={state.relationSuggestions}
              onRelationSearch={searchRelations}
              onFrontmatterChange={setFrontmatter}
              onBodyChange={setBody}
              onSave={save}
              onCancel={cancel}
            />
          </>
        ) : (
          <div className="rounded-md border p-8 text-center text-sm text-muted-foreground">
            Entity not found
          </div>
        )}
      </div>
    </AppFrame>
  );
}
