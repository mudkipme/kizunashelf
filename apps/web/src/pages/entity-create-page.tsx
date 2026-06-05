import { useEffect, useMemo, useState } from "react";
import { getConfig } from "@kizunashelf/api-contract";
import { PlusIcon, SearchIcon, WandSparklesIcon } from "lucide-react";
import { useNavigate } from "react-router-dom";

import { apiFetch, errorMessage, isAbortError } from "@/api/client";
import { addEntity, getAppCapabilities, searchSources } from "@/api/entities";
import { AppFrame } from "@/components/layout/app-frame";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import { Textarea } from "@/components/ui/textarea";
import type { Capabilities, ConfigResponse, ExternalCandidate } from "@/types/api";

type CreateState = {
  config?: ConfigResponse;
  capabilities?: Capabilities;
  loading: boolean;
  error?: string;
};

export function EntityCreatePage() {
  const navigate = useNavigate();
  const [state, setState] = useState<CreateState>({ loading: true });
  const [typeId, setTypeId] = useState("");
  const [basename, setBasename] = useState("");
  const [frontmatterText, setFrontmatterText] = useState("{}");
  const [body, setBody] = useState("");
  const [creating, setCreating] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");
  const [provider, setProvider] = useState("all");
  const [searching, setSearching] = useState(false);
  const [candidates, setCandidates] = useState<ExternalCandidate[]>([]);
  const [message, setMessage] = useState<string>();
  const contentWritable = state.capabilities?.contentWritable !== false;

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

  async function load(signal: AbortSignal) {
    setState({ loading: true });
    try {
      const [config, capabilities] = await Promise.all([
        getConfig({ signal }, apiFetch),
        getAppCapabilities({ signal }),
      ]);
      setState({ config, capabilities, loading: false });
    } catch (error) {
      if (isAbortError(error)) return;
      setState({ loading: false, error: errorMessage(error) });
    }
  }

  async function create() {
    setCreating(true);
    setMessage(undefined);
    setState((current) => ({ ...current, error: undefined }));
    try {
      const frontmatter = parseFrontmatter(frontmatterText);
      const result = await addEntity({
        type: typeId,
        basename,
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
    const query = searchQuery.trim() || basename.trim();
    if (!query) return;
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
      if (result.items.length === 0) setMessage("No external matches");
    } catch (error) {
      setState((current) => ({ ...current, error: errorMessage(error) }));
    } finally {
      setSearching(false);
    }
  }

  function useCandidate(candidate: ExternalCandidate) {
    const current = parseFrontmatter(frontmatterText);
    const next = {
      ...current,
      ...candidate.metadata,
    };
    setFrontmatterText(JSON.stringify(next, null, 2));
    setBasename((currentBasename) => currentBasename || candidate.title);
    setSearchQuery(candidate.title);
    setMessage(`Using ${candidate.provider}: ${candidate.title}`);
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
          <Button type="button" onClick={create} disabled={!contentWritable || creating || !typeId || !basename.trim()}>
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
              <Select value={typeId} onChange={(event) => setTypeId(event.target.value)}>
                {state.config?.types.map((type) => (
                  <option key={type.id} value={type.id}>
                    {type.label}
                  </option>
                ))}
              </Select>
            </label>
            <label className="flex flex-col gap-1 text-sm font-medium">
              Filename
              <Input value={basename} onChange={(event) => setBasename(event.target.value)} placeholder="Entity title" />
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
              <option value="bangumi">Bangumi</option>
              <option value="igdb">IGDB</option>
              <option value="thetvdb">TheTVDB</option>
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
                onClick={() => useCandidate(candidate)}
              >
                <div className="flex min-w-0 items-center gap-2">
                  <Badge variant="secondary">{candidate.provider}</Badge>
                  <span className="min-w-0 truncate text-sm font-medium">{candidate.title}</span>
                </div>
                {candidate.subtitle ? (
                  <div className="mt-1 truncate text-xs text-muted-foreground">{candidate.subtitle}</div>
                ) : null}
                {candidate.brief ? (
                  <p className="mt-2 line-clamp-2 text-xs text-muted-foreground">{candidate.brief}</p>
                ) : null}
              </button>
            ))}
          </div>
        </section>

        <section className="grid gap-4 lg:grid-cols-2">
          <label className="flex flex-col gap-1 text-sm font-medium">
            Frontmatter JSON
            <Textarea
              value={frontmatterText}
              onChange={(event) => setFrontmatterText(event.target.value)}
              className="min-h-80 font-mono text-xs"
              spellCheck={false}
            />
          </label>
          <label className="flex flex-col gap-1 text-sm font-medium">
            Markdown Body
            <Textarea
              value={body}
              onChange={(event) => setBody(event.target.value)}
              className="min-h-80 font-mono text-xs"
              spellCheck={false}
            />
          </label>
        </section>
      </div>
    </AppFrame>
  );
}

function parseFrontmatter(value: string) {
  const parsed = JSON.parse(value || "{}") as unknown;
  if (!parsed || Array.isArray(parsed) || typeof parsed !== "object") {
    throw new Error("Frontmatter must be a JSON object");
  }
  return parsed as Record<string, unknown>;
}
