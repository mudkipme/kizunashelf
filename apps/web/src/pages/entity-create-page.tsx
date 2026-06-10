import { useCallback, useEffect, useMemo, useState } from "react";
import { getConfig, getEntities } from "@kizunashelf/api-contract";
import { PlusIcon, SearchIcon } from "lucide-react";
import { useNavigate } from "react-router-dom";

import { apiFetch, errorMessage, isAbortError } from "@/api/client";
import { addEntity, getAppCapabilities } from "@/api/entities";
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
import type { Capabilities, ConfigResponse, EntitySummary } from "@/types/api";

type CreateState = {
  config?: ConfigResponse;
  capabilities?: Capabilities;
  relationSuggestions: EntitySummary[];
  loading: boolean;
  error?: string;
};

export function EntityCreatePage() {
  const navigate = useNavigate();
  const [state, setState] = useState<CreateState>({ loading: true, relationSuggestions: [] });
  const [typeId, setTypeId] = useState("");
  const [basename, setBasename] = useState("");
  const [frontmatter, setFrontmatter] = useState<FrontmatterDraft>({});
  const [body, setBody] = useState("");
  const [creating, setCreating] = useState(false);
  const [message, setMessage] = useState<string>();
  const contentWritable = state.capabilities?.contentWritable !== false;
  const normalizedBasename = normalizeBasename(basename);
  const basenameError = basenameValidationError(basename);
  const showBasenameError = Boolean(basename) && Boolean(basenameError);

  useEffect(() => {
    const controller = new AbortController();
    void load(controller.signal);
    return () => controller.abort();
  }, []);

  useEffect(() => {
    const firstType = state.config?.types[0]?.id;
    if (!typeId && firstType) setTypeId(firstType);
  }, [state.config, typeId]);

  const selectedType = useMemo(
    () => state.config?.types.find((type) => type.id === typeId),
    [state.config, typeId],
  );
  const external = useExternalMatch({
    typeConfig: selectedType,
    entityType: typeId,
    defaultQuery: normalizedBasename,
    onError: (error) => setState((current) => ({ ...current, error })),
  });

  async function load(signal: AbortSignal) {
    setState({ loading: true, relationSuggestions: [] });
    try {
      const [config, capabilities] = await Promise.all([
        getConfig({ signal }, apiFetch),
        getAppCapabilities({ signal }),
      ]);
      setState({ config, capabilities, relationSuggestions: [], loading: false });
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

  async function create() {
    if (!contentWritable) return;
    if (basenameError) {
      setBasename(normalizedBasename);
      setState((current) => ({ ...current, error: basenameError }));
      return;
    }
    setCreating(true);
    setMessage(undefined);
    setState((current) => ({ ...current, error: undefined }));
    try {
      const result = await addEntity({
        type: typeId,
        basename: normalizedBasename,
        frontmatter,
        body,
      });
      navigate(`/entities/${encodeURIComponent(result.entity.id)}`);
    } catch (error) {
      setState((current) => ({ ...current, error: errorMessage(error) }));
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
    setBasename((currentBasename) => currentBasename || external.selectedCandidate?.title || "");
    external.setQuery(external.selectedCandidate.title);
    setMessage(`Using ${external.selectedCandidate.provider}: ${external.selectedCandidate.title}`);
    external.setOpen(false);
  }

  return (
    <AppFrame error={state.error}>
      <div className="mx-auto flex w-full max-w-5xl flex-col gap-4 p-4">
        <header className="flex flex-wrap items-start justify-between gap-3">
          <div className="min-w-0">
            <h1 className="truncate text-base font-semibold">Add Entity</h1>
            <p className="mt-1 truncate text-xs text-muted-foreground">
              {selectedType ? `${selectedType.label} · ${selectedType.path}` : "Choose a type"}
            </p>
          </div>
          <Button type="button" onClick={create} disabled={!contentWritable || creating || !typeId || Boolean(basenameError)}>
            <PlusIcon data-icon="inline-start" />
            {creating ? "Creating" : "Create"}
          </Button>
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
                {state.config?.types.map((type) => (
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
              onClick={() => external.setOpen(true)}
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
          selectedFields={external.selectedFields}
          providerOptions={external.providerOptions}
          externalSearchEnabled={external.externalSearchEnabled}
          searching={external.searching}
          applying={false}
          contentWritable={contentWritable}
          applyLabel="Use Selected"
          emptyMessage={external.emptyMessage}
          onOpenChange={external.setOpen}
          onQueryChange={external.setQuery}
          onProviderChange={external.setProvider}
          onSearch={() => {
            setMessage(undefined);
            setState((current) => ({ ...current, error: undefined }));
            void external.search();
          }}
          onChooseCandidate={external.chooseCandidate}
          onSelectedFieldsChange={external.setSelectedFields}
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
          relationSuggestions={state.relationSuggestions}
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
