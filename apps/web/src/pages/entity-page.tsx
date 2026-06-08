import { useEffect, useMemo, useState } from "react";
import { getConfig, getEntities, getEntity, getEntityDates } from "@kizunashelf/api-contract";
import {
  CheckIcon,
  FilePenLineIcon,
  PencilIcon,
  SearchIcon,
  Trash2Icon,
  WandSparklesIcon,
  XIcon,
} from "lucide-react";
import { useNavigate, useParams } from "react-router-dom";

import { apiFetch, errorMessage } from "@/api/client";
import { getAppCapabilities, removeEntity, saveEntity, searchSources } from "@/api/entities";
import { EntityDetail } from "@/components/assets/entity-detail";
import {
  type FrontmatterDraft,
  frontmatterPatch,
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
  type ExternalMetadataEntry,
} from "@/lib/external-metadata";
import { basenameValidationError, normalizeBasename } from "@/lib/basename";
import { groupRelations } from "@/lib/relations";
import type {
  Capabilities,
  ConfigResponse,
  Entity,
  EntityDatesResponse,
  EntityDetailResponse,
  EntitySummary,
  ExternalCandidate,
} from "@/types/api";

export function EntityPage() {
  const { id } = useParams();
  const navigate = useNavigate();
  const [state, setState] = useState<{
    detail?: EntityDetailResponse;
    dates?: EntityDatesResponse;
    config?: ConfigResponse;
    capabilities?: Capabilities;
    relationSuggestions: EntitySummary[];
    loading: boolean;
    error?: string;
  }>({ loading: true, relationSuggestions: [] });
  const [editOpen, setEditOpen] = useState(false);
  const [renameOpen, setRenameOpen] = useState(false);
  const [renameBasename, setRenameBasename] = useState("");
  const [matchOpen, setMatchOpen] = useState(false);
  const [draftFrontmatter, setDraftFrontmatter] = useState<FrontmatterDraft>({});
  const [bodyText, setBodyText] = useState("");
  const [saving, setSaving] = useState(false);
  const [externalQuery, setExternalQuery] = useState("");
  const [externalProvider, setExternalProvider] = useState("all");
  const [externalCandidates, setExternalCandidates] = useState<ExternalCandidate[]>([]);
  const [externalSearching, setExternalSearching] = useState(false);
  const [selectedCandidate, setSelectedCandidate] = useState<ExternalCandidate>();
  const [selectedFields, setSelectedFields] = useState<Set<string>>(new Set());

  useEffect(() => {
    if (!id) return;
    void loadEntity();
  }, [id]);

  const entity = state.detail?.entity;
  const contentWritable = state.capabilities?.contentWritable !== false;
  const typeConfig = state.config?.types.find((type) => type.id === entity?.type);
  const selectedCandidateEntries = useMemo(
    () => (selectedCandidate ? candidateMetadataEntries(selectedCandidate, typeConfig) : []),
    [selectedCandidate, typeConfig],
  );
  const relationGroups = useMemo(
    () => groupRelations(state.detail?.relations ?? []),
    [state.detail],
  );

  useEffect(() => {
    if (!entity) return;
    setDraftFrontmatter(normalizeFrontmatter(entity.frontmatter));
    setBodyText(entity.body);
    setRenameBasename(entity.basename);
    setExternalQuery(entity.title);
  }, [entity?.id, entity?.revision]);

  async function loadEntity() {
    if (!id) return;
    setState((current) => ({ ...current, loading: true, error: undefined }));
    try {
      const [detail, dates, config, capabilities] = await Promise.all([
        getEntity(id, undefined, apiFetch),
        getEntityDates(id, undefined, apiFetch),
        getConfig(undefined, apiFetch),
        getAppCapabilities(),
      ]);
      const relationSuggestions = await loadRelationSuggestions();
      setState({ detail, dates, config, capabilities, relationSuggestions, loading: false });
    } catch (error: unknown) {
      setState((current) => ({ ...current, loading: false, error: errorMessage(error) }));
    }
  }

  async function loadRelationSuggestions() {
    const result = await getEntities(
      { type: "all", pageSize: 500, sort: "title", direction: "asc" },
      undefined,
      apiFetch,
    );
    return result.items;
  }

  async function saveFullEdit() {
    if (!entity) return;
    setSaving(true);
    try {
      await saveEntity(entity.id, {
        revision: entity.revision,
        frontmatter: frontmatterPatch(entity.frontmatter, draftFrontmatter),
        body: bodyText,
      });
      setEditOpen(false);
      await loadEntity();
    } catch (error) {
      setState((current) => ({ ...current, error: errorMessage(error) }));
    } finally {
      setSaving(false);
    }
  }

  async function saveRename() {
    if (!entity) return;
    const nextBasename = normalizeBasename(renameBasename);
    const validationError = basenameValidationError(nextBasename);
    setRenameBasename(nextBasename);
    if (validationError) {
      setState((current) => ({ ...current, error: validationError }));
      return;
    }
    if (nextBasename === entity.basename) {
      setRenameOpen(false);
      return;
    }
    setSaving(true);
    try {
      const result = await saveEntity(entity.id, {
        revision: entity.revision,
        renameTo: nextBasename,
      });
      setRenameOpen(false);
      navigate(`/entities/${encodeURIComponent(result.entity.id)}`);
    } catch (error) {
      setState((current) => ({ ...current, error: errorMessage(error) }));
    } finally {
      setSaving(false);
    }
  }

  async function searchExternal() {
    if (!entity) return;
    setExternalSearching(true);
    setSelectedCandidate(undefined);
    setSelectedFields(new Set());
    try {
      const result = await searchSources({
        provider: externalProvider,
        q: externalQuery || entity.title,
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

  function chooseCandidate(candidate: ExternalCandidate) {
    setSelectedCandidate(candidate);
    setSelectedFields(new Set(candidateMetadataEntries(candidate, typeConfig).map((entry) => entry.field)));
  }

  async function applyCandidate() {
    if (!entity || !selectedCandidate) return;
    const patch = candidateMetadataPatch(selectedCandidate, typeConfig, selectedFields);
    setSaving(true);
    try {
      await saveEntity(entity.id, {
        revision: entity.revision,
        frontmatter: patch,
      });
      setMatchOpen(false);
      await loadEntity();
    } catch (error) {
      setState((current) => ({ ...current, error: errorMessage(error) }));
    } finally {
      setSaving(false);
    }
  }

  async function deleteCurrentEntity() {
    if (!entity) return;
    if (!window.confirm(`Move ${entity.title} to KizunaShelf trash?`)) return;
    setSaving(true);
    try {
      await removeEntity(entity.id, { revision: entity.revision, mode: "trash" });
      navigate("/library");
    } catch (error) {
      setState((current) => ({ ...current, error: errorMessage(error) }));
    } finally {
      setSaving(false);
    }
  }

  return (
    <AppFrame error={state.error}>
      <div className="mx-auto flex w-full max-w-6xl flex-col gap-4 p-4">
        {state.loading ? (
          <div className="rounded-md border p-8 text-center text-sm text-muted-foreground">Loading</div>
        ) : entity ? (
          <>
            <EntityActions
              entity={entity}
              contentWritable={contentWritable}
              saving={saving}
              onEdit={() => setEditOpen(true)}
              onRename={() => setRenameOpen((open) => !open)}
              onMatch={() => setMatchOpen((open) => !open)}
              onDelete={deleteCurrentEntity}
            />
            {renameOpen ? (
              <RenamePanel
                currentBasename={entity.basename}
                basename={renameBasename}
                saving={saving}
                disabled={!contentWritable}
                onBasenameChange={setRenameBasename}
                onSave={saveRename}
                onCancel={() => {
                  setRenameBasename(entity.basename);
                  setRenameOpen(false);
                }}
              />
            ) : null}
            {editOpen ? (
              <MetadataEditor
                title={`Edit ${entity.title}`}
                path={entity.path}
                typeConfig={typeConfig}
                frontmatter={draftFrontmatter}
                bodyText={bodyText}
                saving={saving}
                relationSuggestions={state.relationSuggestions}
                onFrontmatterChange={setDraftFrontmatter}
                onBodyChange={setBodyText}
                disabled={!contentWritable}
                onSave={saveFullEdit}
                onCancel={() => setEditOpen(false)}
              />
            ) : null}
            {matchOpen ? (
              <ExternalMatchPanel
                query={externalQuery}
                provider={externalProvider}
                candidates={externalCandidates}
                selectedCandidate={selectedCandidate}
                metadataEntries={selectedCandidateEntries}
                selectedFields={selectedFields}
                searching={externalSearching}
                saving={saving}
                contentWritable={contentWritable}
                onQueryChange={setExternalQuery}
                onProviderChange={setExternalProvider}
                onSearch={searchExternal}
                onChooseCandidate={chooseCandidate}
                onSelectedFieldsChange={setSelectedFields}
                onApply={applyCandidate}
              />
            ) : null}
            <EntityDetail
              entity={entity}
              relations={state.detail?.relations ?? []}
              relatedEntities={state.detail?.relatedEntities ?? []}
              relationGroups={relationGroups}
              dates={state.dates}
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

function EntityActions({
  entity,
  contentWritable,
  saving,
  onEdit,
  onRename,
  onMatch,
  onDelete,
}: {
  entity: Entity;
  contentWritable: boolean;
  saving: boolean;
  onEdit: () => void;
  onRename: () => void;
  onMatch: () => void;
  onDelete: () => void;
}) {
  return (
    <section className="rounded-md border p-3">
      <div className="flex flex-wrap items-end gap-2">
        <Button type="button" variant="outline" onClick={onEdit} disabled={!contentWritable}>
          <PencilIcon data-icon="inline-start" />
          Edit
        </Button>
        <Button type="button" variant="outline" onClick={onRename} disabled={!contentWritable || saving}>
          <FilePenLineIcon data-icon="inline-start" />
          Rename
        </Button>
        <Button type="button" variant="outline" onClick={onMatch}>
          <SearchIcon data-icon="inline-start" />
          Match
        </Button>
        <Button type="button" variant="outline" onClick={onDelete} disabled={!contentWritable || saving}>
          <Trash2Icon data-icon="inline-start" />
          Delete
        </Button>
      </div>
      {!contentWritable ? (
        <p className="mt-2 text-xs text-muted-foreground">Content writes are disabled. Editing actions are unavailable.</p>
      ) : (
        <p className="mt-2 truncate text-xs text-muted-foreground">{entity.path}</p>
      )}
    </section>
  );
}

function RenamePanel({
  currentBasename,
  basename,
  saving,
  disabled,
  onBasenameChange,
  onSave,
  onCancel,
}: {
  currentBasename: string;
  basename: string;
  saving: boolean;
  disabled: boolean;
  onBasenameChange: (value: string) => void;
  onSave: () => void;
  onCancel: () => void;
}) {
  const normalizedBasename = normalizeBasename(basename);
  const validationError = basenameValidationError(basename);
  const unchanged = normalizedBasename === currentBasename;
  return (
    <section className="rounded-md border p-4">
      <div className="flex flex-wrap items-end gap-2">
        <label className="min-w-60 flex-1 text-sm font-medium">
          Basename
          <Input
            value={basename}
            onChange={(event) => onBasenameChange(event.target.value)}
            onBlur={() => onBasenameChange(normalizedBasename)}
            disabled={disabled || saving}
            aria-invalid={Boolean(validationError)}
          />
        </label>
        <Button
          type="button"
          onClick={onSave}
          disabled={disabled || saving || Boolean(validationError) || unchanged}
        >
          <CheckIcon data-icon="inline-start" />
          {saving ? "Renaming" : "Rename"}
        </Button>
        <Button type="button" variant="outline" onClick={onCancel} disabled={saving}>
          <XIcon data-icon="inline-start" />
          Cancel
        </Button>
      </div>
      <p className="mt-2 text-xs text-muted-foreground">
        File path stays in the same folder. Only the Markdown basename changes.
      </p>
      {validationError ? <p className="mt-1 text-xs text-destructive">{validationError}</p> : null}
    </section>
  );
}

function ExternalMatchPanel({
  query,
  provider,
  candidates,
  selectedCandidate,
  metadataEntries,
  selectedFields,
  searching,
  saving,
  contentWritable,
  onQueryChange,
  onProviderChange,
  onSearch,
  onChooseCandidate,
  onSelectedFieldsChange,
  onApply,
}: {
  query: string;
  provider: string;
  candidates: ExternalCandidate[];
  selectedCandidate?: ExternalCandidate;
  metadataEntries: ExternalMetadataEntry[];
  selectedFields: Set<string>;
  searching: boolean;
  saving: boolean;
  contentWritable: boolean;
  onQueryChange: (value: string) => void;
  onProviderChange: (value: string) => void;
  onSearch: () => void;
  onChooseCandidate: (candidate: ExternalCandidate) => void;
  onSelectedFieldsChange: (fields: Set<string>) => void;
  onApply: () => void;
}) {
  function toggleField(field: string) {
    const next = new Set(selectedFields);
    if (next.has(field)) next.delete(field);
    else next.add(field);
    onSelectedFieldsChange(next);
  }

  return (
    <section className="rounded-md border p-4">
      <div className="flex flex-wrap items-end gap-2">
        <label className="min-w-48 flex-1 text-sm font-medium">
          Search
          <Input value={query} onChange={(event) => onQueryChange(event.target.value)} />
        </label>
        <Select value={provider} onChange={(event) => onProviderChange(event.target.value)} aria-label="Provider">
          <option value="all">All sources</option>
          <option value="bangumi">Bangumi</option>
          <option value="igdb">IGDB</option>
          <option value="thetvdb">TheTVDB</option>
        </Select>
        <Button type="button" variant="outline" onClick={onSearch} disabled={searching}>
          <SearchIcon data-icon="inline-start" />
          {searching ? "Searching" : "Search"}
        </Button>
      </div>
      <div className="mt-3 grid gap-3 lg:grid-cols-[minmax(0,1fr)_minmax(280px,360px)]">
        <div className="grid gap-2">
          {candidates.map((candidate) => (
            <button
              key={`${candidate.provider}:${candidate.sourceId}`}
              type="button"
              className="rounded-md border p-3 text-left hover:bg-accent"
              onClick={() => onChooseCandidate(candidate)}
            >
              <div className="flex min-w-0 items-center gap-2">
                <Badge variant="secondary">{candidate.provider}</Badge>
                <span className="truncate text-sm font-medium">{candidate.title}</span>
              </div>
              {candidate.brief ? <p className="mt-2 line-clamp-2 text-xs text-muted-foreground">{candidate.brief}</p> : null}
            </button>
          ))}
          {candidates.length === 0 ? <div className="rounded-md border p-4 text-sm text-muted-foreground">No candidates loaded</div> : null}
        </div>
        <div className="rounded-md border p-3">
          <h3 className="text-sm font-semibold">Selected Metadata</h3>
          {selectedCandidate ? (
            <div className="mt-3 flex flex-col gap-2">
              {metadataEntries.map((entry) => (
                <label key={entry.field} className="flex min-w-0 items-start gap-2 text-sm">
                  <input
                    type="checkbox"
                    checked={selectedFields.has(entry.field)}
                    onChange={() => toggleField(entry.field)}
                    className="mt-1"
                  />
                  <span className="min-w-0">
                    <span className="block font-medium">{entry.label}</span>
                    <span className="block font-mono text-[11px] text-muted-foreground">{entry.field}</span>
                    <span className="block break-words text-xs text-muted-foreground">{formatMetadataValue(entry.value)}</span>
                  </span>
                </label>
              ))}
              {metadataEntries.length === 0 ? (
                <p className="text-sm text-muted-foreground">No candidate fields match this type schema.</p>
              ) : null}
              <Button type="button" onClick={onApply} disabled={!contentWritable || saving || selectedFields.size === 0}>
                <WandSparklesIcon data-icon="inline-start" />
                Apply Selected
              </Button>
            </div>
          ) : (
            <p className="mt-2 text-sm text-muted-foreground">Choose a candidate to compare fields.</p>
          )}
        </div>
      </div>
    </section>
  );
}

function formatMetadataValue(value: unknown) {
  if (typeof value === "string") return value;
  return JSON.stringify(value);
}

function humanizeField(key: string) {
  return key.replace(/[_-]+/g, " ");
}

function isVirtualTitleField(key: string) {
  return ["filename", "basename", "$filename", "$basename"].includes(key);
}
