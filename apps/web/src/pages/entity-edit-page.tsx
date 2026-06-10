import { useCallback, useEffect, useMemo, useState } from "react";
import { getConfig, getEntities, getEntity } from "@kizunashelf/api-contract";
import { ArrowLeftIcon, SearchIcon } from "lucide-react";
import { useNavigate, useParams } from "react-router-dom";

import { apiFetch, errorMessage, isAbortError } from "@/api/client";
import { getAppCapabilities, saveEntity } from "@/api/entities";
import { ExternalMatchDialog } from "@/components/entities/external-match-dialog";
import {
  type FrontmatterDraft,
  frontmatterPatch,
  MetadataEditor,
  normalizeFrontmatter,
} from "@/components/entities/metadata-editor";
import { useExternalMatch } from "@/components/entities/use-external-match";
import { AppFrame } from "@/components/layout/app-frame";
import { Button } from "@/components/ui/button";
import type {
  Capabilities,
  ConfigResponse,
  EntityDetailResponse,
  EntitySummary,
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
  const entity = state.detail?.entity;
  const contentWritable = state.capabilities?.contentWritable !== false;
  const typeConfig = useMemo(
    () => state.config?.types.find((type) => type.id === entity?.type),
    [state.config, entity?.type],
  );
  const external = useExternalMatch({
    typeConfig,
    entityType: entity?.type,
    defaultQuery: entity?.title,
    externalRefs: entity?.externalRefs,
    onError: (error) => setState((current) => ({ ...current, error })),
  });

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
    external.setQuery(entity.title);
  }, [entity?.id, entity?.revision, external.setQuery]);

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

  function applyCandidate() {
    if (!external.selectedCandidate || !contentWritable) return;
    const next = {
      ...frontmatter,
      ...external.selectedPatch(),
    };
    setFrontmatter(normalizeFrontmatter(next));
    external.setQuery(external.selectedCandidate.title);
    external.setOpen(false);
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
                onClick={() => external.setOpen(true)}
                disabled={!external.externalSearchEnabled}
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
              open={external.open}
              query={external.query}
              provider={external.provider}
              candidates={external.candidates}
              selectedCandidate={external.selectedCandidate}
              metadataEntries={external.metadataEntries}
              selectedFields={external.selectedFields}
              providerOptions={external.providerOptions}
              externalSearchEnabled={external.externalSearchEnabled}
              existingExternalRefs={external.existingExternalRefs}
              currentValues={frontmatter}
              searching={external.searching}
              applying={false}
              contentWritable={contentWritable}
              applyLabel="Use Selected"
              emptyMessage={external.emptyMessage}
              onOpenChange={external.setOpen}
              onQueryChange={external.setQuery}
              onProviderChange={external.setProvider}
              onSearch={() => {
                setState((current) => ({ ...current, error: undefined }));
                void external.search();
              }}
              onRefreshRef={external.refreshFromExternalRef}
              onChooseCandidate={external.chooseCandidate}
              onSelectedFieldsChange={external.setSelectedFields}
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
