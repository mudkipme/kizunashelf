import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { ArrowLeftIcon, SearchIcon } from "lucide-react";
import { useNavigate, useParams } from "react-router-dom";

import { errorMessage, isConflictError } from "@/api/client";
import { saveEntity } from "@/api/entities";
import { useInvalidateEntityData } from "@/api/invalidate-entity-data";
import { useRelationSearch } from "@/api/use-relation-search";
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
import { useTitleLanguage } from "@/lib/language";
import { entityTitle } from "@/lib/title-language";

export function EntityEditPage() {
  const { id } = useParams();
  const navigate = useNavigate();
  const invalidateEntityData = useInvalidateEntityData();
  const detail = useQuery({ ...entityQuery(id ?? ""), enabled: Boolean(id) });
  const config = useQuery(configQuery());
  const providerCatalog = useQuery(providerCatalogQuery());
  const capabilities = useQuery(capabilitiesQuery());
  const [error, setError] = useState<string>();
  const [conflict, setConflict] = useState(false);
  const [frontmatter, setFrontmatter] = useState<FrontmatterDraft>({});
  const [body, setBody] = useState("");
  const [saving, setSaving] = useState(false);
  // Tracks which entity the local draft was seeded from, so a background refetch
  // of the same entity doesn't clobber in-progress edits.
  const seededEntityIdRef = useRef<string | undefined>(undefined);
  const loading =
    detail.isPending ||
    config.isPending ||
    providerCatalog.isPending ||
    capabilities.isPending;
  const queryError =
    detail.error ?? config.error ?? providerCatalog.error ?? capabilities.error;
  const entity = detail.data?.entity;
  const contentWritable = capabilities.data?.contentWritable !== false;
  const language = useTitleLanguage();
  const typeConfig = useMemo(
    () => config.data?.types.find((type) => type.id === entity?.type),
    [config.data, entity?.type],
  );
  const external = useExternalMatch({
    typeConfig,
    providerCatalog: providerCatalog.data,
    entityType: entity?.type,
    defaultQuery: entity ? entityTitle(entity, language) : undefined,
    externalRefs: entity?.externalRefs,
    assetDownloadEnabled: capabilities.data?.assetDownloadEnabled === true,
    onError: setError,
  });
  const { setQuery: setMatchQuery } = external;

  const seedDraft = useCallback(
    (source: NonNullable<typeof entity>) => {
      seededEntityIdRef.current = source.id;
      setFrontmatter(normalizeFrontmatter(source.frontmatter));
      setBody(source.body);
      setMatchQuery(entityTitle(source, language));
    },
    [language, setMatchQuery],
  );

  useEffect(() => {
    if (!entity) return;
    // Seed only when this is a different entity than the one already loaded.
    // Refetches of the same entity (window focus, cache invalidation) keep the
    // user's edits instead of resetting the form underneath them.
    if (seededEntityIdRef.current === entity.id) return;
    seedDraft(entity);
  }, [entity, seedDraft]);

  // Reloads the latest server version, replacing the local draft. Used to
  // recover from a 409 conflict after the entity changed on disk.
  async function reloadLatest() {
    const refreshed = await detail.refetch();
    const fresh = refreshed.data?.entity;
    if (!fresh) return;
    seedDraft(fresh);
    setConflict(false);
    setError(undefined);
  }

  const searchRelations = useRelationSearch();

  async function save() {
    if (!entity || !contentWritable) return;
    setSaving(true);
    setError(undefined);
    setConflict(false);
    try {
      const result = await saveEntity(entity.id, {
        revision: entity.revision,
        frontmatter: frontmatterPatch(entity.frontmatter, frontmatter),
        body,
      });
      await external.maybeDownloadCover(result.entity);
      await invalidateEntityData();
      navigate(`/entities/${encodeURIComponent(result.entity.id)}`);
    } catch (error) {
      if (isConflictError(error)) {
        // The file changed on disk since it was loaded. Keep the user's edits and
        // offer a reload so they can reapply them against the latest version.
        setConflict(true);
        setError(
          "This entity changed on disk since you opened it. Your edits are kept here — reload the latest version, then reapply them.",
        );
      } else {
        setError(errorMessage(error));
      }
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
    setBody((currentBody) =>
      applyExternalBodySections(currentBody, external.selectedBodyPatch()),
    );
    external.setQuery(external.selectedCandidate.title);
    external.setOpen(false);
  }

  return (
    <AppFrame
      error={
        conflict
          ? undefined
          : (error ?? (queryError ? errorMessage(queryError) : undefined))
      }
    >
      <div className="mx-auto flex w-full max-w-5xl flex-col gap-4 p-4">
        <header className="flex flex-wrap items-start justify-between gap-3">
          <div className="min-w-0">
            <h1 className="truncate text-base font-semibold">
              {entity ? `Edit ${entityTitle(entity, language)}` : "Edit Entity"}
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
            <Button
              type="button"
              variant="outline"
              onClick={cancel}
              disabled={saving}
            >
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

        {conflict ? (
          <div className="flex flex-wrap items-center justify-between gap-3 rounded-md border border-destructive/40 bg-destructive/10 p-3 text-sm text-destructive">
            <span className="min-w-0">{error}</span>
            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={() => void reloadLatest()}
              disabled={saving}
            >
              Reload latest version
            </Button>
          </div>
        ) : null}

        {loading ? (
          <div className="rounded-md border p-8 text-center text-sm text-muted-foreground">
            Loading
          </div>
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
