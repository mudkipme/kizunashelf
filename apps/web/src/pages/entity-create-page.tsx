import { useCallback, useEffect, useMemo, useState } from "react";
import { getConfig, getEntities } from "@kizunashelf/api-contract";
import { PlusIcon, SearchIcon } from "lucide-react";
import { useNavigate } from "react-router-dom";

import { apiFetch, errorMessage, isAbortError } from "@/api/client";
import { addEntity, getAppCapabilities, searchSources } from "@/api/entities";
import {
  type FrontmatterDraft,
  MetadataEditor,
  normalizeFrontmatter,
} from "@/components/entities/metadata-editor";
import { ExternalMatchDialog } from "@/components/entities/external-match-dialog";
import { AppFrame } from "@/components/layout/app-frame";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import {
  candidateMetadataEntries,
  candidateMetadataPatch,
  candidateMetadataPreviewEntries,
  externalProviderPriority,
} from "@/lib/external-metadata";
import { basenameValidationError, normalizeBasename } from "@/lib/basename";
import type { Capabilities, ConfigResponse, EntitySummary, ExternalCandidate } from "@/types/api";

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
  const [matchOpen, setMatchOpen] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");
  const [provider, setProvider] = useState("all");
  const [searching, setSearching] = useState(false);
  const [candidates, setCandidates] = useState<ExternalCandidate[]>([]);
  const [selectedCandidate, setSelectedCandidate] = useState<ExternalCandidate>();
  const [selectedFields, setSelectedFields] = useState<Set<string>>(new Set());
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
  const selectedCandidateEntries = useMemo(
    () => (selectedCandidate ? candidateMetadataPreviewEntries(selectedCandidate, selectedType) : []),
    [selectedCandidate, selectedType],
  );
  const providerOptions = useMemo(() => externalProviderPriority(selectedType), [selectedType]);
  const externalSearchEnabled = providerOptions.length > 0;

  useEffect(() => {
    if (providerOptions.length === 0) {
      if (provider !== "all") setProvider("all");
      return;
    }
    if (providerOptions.length === 1) {
      if (provider !== providerOptions[0]) setProvider(providerOptions[0]);
      return;
    }
    if (provider !== "all" && !providerOptions.includes(provider)) setProvider("all");
  }, [provider, providerOptions]);

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

  async function searchExternal() {
    const query = searchQuery.trim() || normalizedBasename;
    if (!query || !typeId || !externalSearchEnabled) return;
    if (provider !== "all" && !providerOptions.includes(provider)) return;
    setSearching(true);
    setMessage(undefined);
    setState((current) => ({ ...current, error: undefined }));
    try {
      const result = await searchSources({
        provider,
        q: query,
        type: typeId,
        pageSize: 8,
      });
      setCandidates(result.items);
      setSelectedCandidate(undefined);
      setSelectedFields(new Set());
      if (result.items.length === 0) setMessage("No external matches");
    } catch (error) {
      setState((current) => ({ ...current, error: errorMessage(error) }));
    } finally {
      setSearching(false);
    }
  }

  function chooseCandidate(candidate: ExternalCandidate) {
    setSelectedCandidate(candidate);
    setSelectedFields(new Set(candidateMetadataEntries(candidate, selectedType).map((entry) => entry.field)));
  }

  function applyCandidate() {
    if (!selectedCandidate || !contentWritable) return;
    const next = {
      ...frontmatter,
      ...candidateMetadataPatch(selectedCandidate, selectedType, selectedFields),
    };
    setFrontmatter(normalizeFrontmatter(next));
    setBasename((currentBasename) => currentBasename || selectedCandidate.title);
    setSearchQuery(selectedCandidate.title);
    setMessage(`Using ${selectedCandidate.provider}: ${selectedCandidate.title}`);
    setMatchOpen(false);
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
              onClick={() => setMatchOpen(true)}
              disabled={!externalSearchEnabled}
            >
              <SearchIcon data-icon="inline-start" />
              Match Metadata
            </Button>
          </div>
        </section>

        <ExternalMatchDialog
          open={matchOpen}
          query={searchQuery}
          provider={provider}
          candidates={candidates}
          selectedCandidate={selectedCandidate}
          metadataEntries={selectedCandidateEntries}
          selectedFields={selectedFields}
          providerOptions={providerOptions}
          externalSearchEnabled={externalSearchEnabled}
          searching={searching}
          applying={false}
          contentWritable={contentWritable}
          applyLabel="Use Selected"
          emptyMessage={message === "No external matches" ? message : "No candidates loaded"}
          onOpenChange={setMatchOpen}
          onQueryChange={setSearchQuery}
          onProviderChange={setProvider}
          onSearch={searchExternal}
          onChooseCandidate={chooseCandidate}
          onSelectedFieldsChange={setSelectedFields}
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
