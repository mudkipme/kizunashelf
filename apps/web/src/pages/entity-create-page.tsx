import { useEffect, useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { SearchIcon } from "lucide-react";
import { useNavigate, useSearchParams } from "react-router-dom";

import { errorMessage } from "@/api/client";
import { addEntity } from "@/api/entities";
import { useInvalidateEntityData } from "@/api/invalidate-entity-data";
import { useRelationSearch } from "@/api/use-relation-search";
import { capabilitiesQuery, configQuery, providerCatalogQuery } from "@/api/queries";
import {
  type FrontmatterDraft,
  MetadataEditor,
  normalizeFrontmatter,
} from "@/components/entities/metadata-editor";
import { ExternalMatchDialog } from "@/components/entities/external-match-dialog";
import { useExternalMatch } from "@/components/entities/use-external-match";
import { AppFrame } from "@/components/layout/app-frame";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import { basenameValidationError, normalizeBasename } from "@/lib/basename";
import { applyExternalBodySections } from "@/lib/external-metadata";

export function EntityCreatePage() {
  const navigate = useNavigate();
  const [searchParams] = useSearchParams();
  const requestedType = searchParams.get("type");
  const invalidateEntityData = useInvalidateEntityData();
  const config = useQuery(configQuery());
  const providerCatalog = useQuery(providerCatalogQuery());
  const capabilities = useQuery(capabilitiesQuery());
  const [error, setError] = useState<string>();
  const [typeId, setTypeId] = useState("");
  const [basename, setBasename] = useState("");
  const [frontmatter, setFrontmatter] = useState<FrontmatterDraft>({});
  const [body, setBody] = useState("");
  const [creating, setCreating] = useState(false);
  const [message, setMessage] = useState<string>();
  const contentWritable = capabilities.data?.contentWritable !== false;
  const queryError = config.error ?? providerCatalog.error ?? capabilities.error;
  const normalizedBasename = normalizeBasename(basename);
  const basenameError = basenameValidationError(basename);
  const showBasenameError = Boolean(basename) && Boolean(basenameError);

  useEffect(() => {
    if (typeId) return;
    const types = config.data?.types;
    if (!types?.length) return;
    const preferred = types.find((type) => type.id === requestedType)?.id;
    setTypeId(preferred ?? types[0].id);
  }, [config.data, typeId, requestedType]);

  const selectedType = useMemo(
    () => config.data?.types.find((type) => type.id === typeId),
    [config.data, typeId],
  );
  const external = useExternalMatch({
    typeConfig: selectedType,
    providerCatalog: providerCatalog.data,
    entityType: typeId,
    defaultQuery: normalizedBasename,
    assetDownloadEnabled: capabilities.data?.assetDownloadEnabled === true,
    onError: setError,
  });

  const searchRelations = useRelationSearch();

  async function create() {
    if (!contentWritable) return;
    if (basenameError) {
      setBasename(normalizedBasename);
      setError(basenameError);
      return;
    }
    setCreating(true);
    setMessage(undefined);
    setError(undefined);
    try {
      const result = await addEntity({
        type: typeId,
        basename: normalizedBasename,
        frontmatter,
        body,
      });
      await external.maybeDownloadCover(result.entity);
      await invalidateEntityData();
      navigate(`/entities/${encodeURIComponent(result.entity.id)}`);
    } catch (error) {
      setError(errorMessage(error));
    } finally {
      setCreating(false);
    }
  }

  function applyCandidate() {
    if (!external.selectedCandidate || !contentWritable) return;
    const next = {
      ...frontmatter,
      ...external.selectedPatch(),
    };
    setFrontmatter(normalizeFrontmatter(next));
    setBody((currentBody) => applyExternalBodySections(currentBody, external.selectedBodyPatch()));
    setBasename((currentBasename) => currentBasename || external.selectedCandidate?.title || "");
    external.setQuery(external.selectedCandidate.title);
    setMessage(`Using ${external.selectedCandidate.provider}: ${external.selectedCandidate.title}`);
    external.setOpen(false);
  }

  return (
    <AppFrame error={error ?? (queryError ? errorMessage(queryError) : undefined)}>
      <div className="mx-auto flex w-full max-w-5xl flex-col gap-4 p-4">
        <header className="flex flex-wrap items-start justify-between gap-3">
          <div className="min-w-0">
            <h1 className="truncate text-base font-semibold">Add Entity</h1>
            <p className="mt-1 truncate text-xs text-muted-foreground">
              {selectedType ? `${selectedType.label} · ${selectedType.path}` : "Choose a type"}
            </p>
          </div>
        </header>

        {!contentWritable ? (
          <div className="rounded-md border border-destructive/40 bg-destructive/10 p-3 text-sm text-destructive">
            Content writes are disabled.
          </div>
        ) : null}
        {message ? <div className="rounded-md border p-3 text-sm text-muted-foreground">{message}</div> : null}

        <section className="rounded-md border p-4">
          <div className="grid gap-3 md:grid-cols-[220px_minmax(0,1fr)]">
            <label className="flex flex-col gap-1 text-sm font-medium">
              Type
              <Select value={typeId} onChange={(event) => setTypeId(event.target.value)} disabled={!contentWritable}>
                {config.data?.types.map((type) => (
                  <option key={type.id} value={type.id}>
                    {type.label}
                  </option>
                ))}
              </Select>
            </label>
            <label className="flex flex-col gap-1 text-sm font-medium">
              Filename
              <Input
                value={basename}
                onChange={(event) => setBasename(event.target.value)}
                onBlur={() => setBasename(normalizeBasename(basename))}
                placeholder="Entity title"
                disabled={!contentWritable}
                aria-invalid={showBasenameError}
              />
              {showBasenameError ? <span className="text-xs text-destructive">{basenameError}</span> : null}
            </label>
          </div>
          <div className="mt-3">
            <Button
              type="button"
              variant="outline"
              onClick={() => {
                if (normalizedBasename) external.setQuery(normalizedBasename);
                external.setOpen(true);
              }}
              disabled={!external.externalSearchEnabled}
            >
              <SearchIcon data-icon="inline-start" />
              Match Metadata
            </Button>
          </div>
        </section>

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
            setMessage(undefined);
            setError(undefined);
            void external.search();
          }}
          onChooseCandidate={external.chooseCandidate}
          onSelectedFieldsChange={external.setSelectedFields}
          onSelectedBodySectionsChange={external.setSelectedBodySections}
          onApply={applyCandidate}
        />

        <MetadataEditor
          title="Metadata"
          path={selectedType?.path}
          typeConfig={selectedType}
          frontmatter={frontmatter}
          bodyText={body}
          saving={creating}
          disabled={!contentWritable}
          relationSuggestions={[]}
          onRelationSearch={searchRelations}
          saveLabel="Create"
          onFrontmatterChange={setFrontmatter}
          onBodyChange={setBody}
          onSave={create}
        />
      </div>
    </AppFrame>
  );
}
