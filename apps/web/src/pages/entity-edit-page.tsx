import { useCallback, useEffect, useMemo, useState } from "react";
import { getEntities } from "@kizunashelf/api-contract";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { ArrowLeftIcon, SearchIcon } from "lucide-react";
import { useNavigate, useParams } from "react-router-dom";

import { apiFetch, errorMessage } from "@/api/client";
import { saveEntity } from "@/api/entities";
import {
  capabilitiesQuery,
  configQuery,
  entityQuery,
  providerCatalogQuery,
} from "@/api/queries";
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
import { applyExternalBodySections } from "@/lib/external-metadata";

export function EntityEditPage() {
  const { id } = useParams();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const detail = useQuery({ ...entityQuery(id ?? ""), enabled: Boolean(id) });
  const config = useQuery(configQuery());
  const providerCatalog = useQuery(providerCatalogQuery());
  const capabilities = useQuery(capabilitiesQuery());
  const [error, setError] = useState<string>();
  const [frontmatter, setFrontmatter] = useState<FrontmatterDraft>({});
  const [body, setBody] = useState("");
  const [saving, setSaving] = useState(false);
  const loading =
    detail.isPending || config.isPending || providerCatalog.isPending || capabilities.isPending;
  const queryError = detail.error ?? config.error ?? providerCatalog.error ?? capabilities.error;
  const entity = detail.data?.entity;
  const contentWritable = capabilities.data?.contentWritable !== false;
  const typeConfig = useMemo(
    () => config.data?.types.find((type) => type.id === entity?.type),
    [config.data, entity?.type],
  );
  const external = useExternalMatch({
    typeConfig,
    providerCatalog: providerCatalog.data,
    entityType: entity?.type,
    defaultQuery: entity?.title,
    externalRefs: entity?.externalRefs,
    assetDownloadEnabled: capabilities.data?.assetDownloadEnabled === true,
    onError: setError,
  });

  useEffect(() => {
    if (!entity) return;
    setFrontmatter(normalizeFrontmatter(entity.frontmatter));
    setBody(entity.body);
    external.setQuery(entity.title);
  }, [entity?.id, entity?.revision, external.setQuery]);

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
    setError(undefined);
    try {
      const result = await saveEntity(entity.id, {
        revision: entity.revision,
        frontmatter: frontmatterPatch(entity.frontmatter, frontmatter),
        body,
      });
      await external.maybeDownloadCover(result.entity);
      await queryClient.invalidateQueries();
      navigate(`/entities/${encodeURIComponent(result.entity.id)}`);
    } catch (error) {
      setError(errorMessage(error));
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
    setBody((currentBody) => applyExternalBodySections(currentBody, external.selectedBodyPatch()));
    external.setQuery(external.selectedCandidate.title);
    external.setOpen(false);
  }

  return (
    <AppFrame error={error ?? (queryError ? errorMessage(queryError) : undefined)}>
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

        {loading ? (
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
              bodyEntries={external.bodyEntries}
              selectedFields={external.selectedFields}
              selectedBodySections={external.selectedBodySections}
              providerCatalog={providerCatalog.data}
              providerOptions={external.providerOptions}
              externalSearchEnabled={external.externalSearchEnabled}
              existingExternalRefs={external.existingExternalRefs}
              currentValues={frontmatter}
              bodyText={body}
              searching={external.searching}
              applying={false}
              contentWritable={contentWritable}
              applyLabel="Use Selected"
              coverDownloadAvailable={external.coverDownloadAvailable}
              downloadCover={external.downloadAfterApply}
              onDownloadCoverChange={external.setDownloadAfterApply}
              emptyMessage={external.emptyMessage}
              onOpenChange={external.setOpen}
              onQueryChange={external.setQuery}
              onProviderChange={external.setProvider}
              onSearch={() => {
                setError(undefined);
                void external.search();
              }}
              onRefreshRef={external.refreshFromExternalRef}
              onChooseCandidate={external.chooseCandidate}
              onSelectedFieldsChange={external.setSelectedFields}
              onSelectedBodySectionsChange={external.setSelectedBodySections}
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
              relationSuggestions={[]}
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
