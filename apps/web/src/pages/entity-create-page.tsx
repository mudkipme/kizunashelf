import { useCallback, useEffect, useMemo, useState } from "react";
import { getConfig, getEntities } from "@kizunashelf/api-contract";
import { PlusIcon, SearchIcon, WandSparklesIcon } from "lucide-react";
import { useNavigate } from "react-router-dom";

import { apiFetch, errorMessage, isAbortError } from "@/api/client";
import { addEntity, getAppCapabilities, searchSources } from "@/api/entities";
import {
  type FrontmatterDraft,
  MetadataEditor,
  normalizeFrontmatter,
} from "@/components/entities/metadata-editor";
import { AppFrame } from "@/components/layout/app-frame";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import {
  candidateMetadataEntries,
  candidateMetadataPatch,
  candidateMetadataPreviewEntries,
  externalProviderPriority,
  externalSourceLabel,
  type ExternalMetadataPreviewEntry,
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
    if (!query || !typeId) return;
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
  }

  function toggleSelectedField(field: string) {
    const next = new Set(selectedFields);
    if (next.has(field)) next.delete(field);
    else next.add(field);
    setSelectedFields(next);
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
        </section>

        <section className="rounded-md border p-4">
          <div className="mb-3 flex flex-wrap items-end gap-2">
            <label className="flex min-w-48 flex-1 flex-col gap-1 text-sm font-medium">
              External search
              <Input
                value={searchQuery}
                onChange={(event) => setSearchQuery(event.target.value)}
                placeholder={basename || "Search media sources"}
              />
            </label>
            <Select value={provider} onChange={(event) => setProvider(event.target.value)} aria-label="Provider">
              <option value="all">All sources</option>
              {providerOptions.map((provider) => (
                <option key={provider} value={provider}>
                  {externalSourceLabel(provider)}
                </option>
              ))}
            </Select>
            <Button type="button" variant="outline" onClick={searchExternal} disabled={searching}>
              <SearchIcon data-icon="inline-start" />
              {searching ? "Searching" : "Search"}
            </Button>
          </div>
          <div className="grid gap-2 md:grid-cols-2">
            {candidates.map((candidate) => (
              <button
                key={`${candidate.provider}:${candidate.sourceId}`}
                type="button"
                className="min-w-0 rounded-md border p-3 text-left hover:bg-accent"
              onClick={() => chooseCandidate(candidate)}
              >
                <div className="flex min-w-0 items-center gap-2">
                  <Badge variant="secondary">{candidate.provider}</Badge>
                  <span className="min-w-0 truncate text-sm font-medium">{candidate.title}</span>
                </div>
                {candidate.brief ? (
                  <p className="mt-2 line-clamp-2 text-xs text-muted-foreground">{candidate.brief}</p>
                ) : null}
              </button>
            ))}
          </div>
          {selectedCandidate ? (
            <ExternalMetadataPicker
              entries={selectedCandidateEntries}
              selectedFields={selectedFields}
              contentWritable={contentWritable}
              onToggleField={toggleSelectedField}
              onApply={applyCandidate}
            />
          ) : null}
        </section>

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

function ExternalMetadataPicker({
  entries,
  selectedFields,
  contentWritable,
  onToggleField,
  onApply,
}: {
  entries: ExternalMetadataPreviewEntry[];
  selectedFields: Set<string>;
  contentWritable: boolean;
  onToggleField: (field: string) => void;
  onApply: () => void;
}) {
  return (
    <div className="mt-3 rounded-md border p-3">
      <h3 className="text-sm font-semibold">Selected Metadata</h3>
      <div className="mt-3 flex flex-col gap-2">
        {entries.map((entry) => (
          <label key={entry.field} className="flex min-w-0 items-start gap-2 text-sm">
            <input
              type="checkbox"
              checked={selectedFields.has(entry.field)}
              onChange={() => onToggleField(entry.field)}
              className="mt-1"
              disabled={!contentWritable || !entry.hasValue}
            />
            <span className="min-w-0">
              <span className="block font-medium">{entry.label}</span>
              <span className="block font-mono text-[11px] text-muted-foreground">
                {entry.field} · {entry.externalField ?? "external ref"}
              </span>
              <span className="block break-words text-xs text-muted-foreground">
                {entry.hasValue ? formatMetadataValue(entry.value) : "No value returned"}
              </span>
            </span>
          </label>
        ))}
        {entries.length === 0 ? (
          <p className="text-sm text-muted-foreground">No candidate fields match this type schema.</p>
        ) : null}
        <Button type="button" onClick={onApply} disabled={!contentWritable || selectedFields.size === 0}>
          <WandSparklesIcon data-icon="inline-start" />
          Apply Selected
        </Button>
      </div>
    </div>
  );
}

function formatMetadataValue(value: unknown) {
  if (typeof value === "string") return value;
  return JSON.stringify(value);
}
