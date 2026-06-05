import type { KeyboardEvent } from "react";
import { useEffect, useMemo, useState } from "react";
import { getConfig, getEntity, getEntityDates } from "@kizunashelf/api-contract";
import {
  CheckIcon,
  MinusIcon,
  PencilIcon,
  PlusIcon,
  SearchIcon,
  Trash2Icon,
  WandSparklesIcon,
  XIcon,
} from "lucide-react";
import { useNavigate, useParams } from "react-router-dom";

import { apiFetch, errorMessage } from "@/api/client";
import { getAppCapabilities, removeEntity, saveEntity, searchSources } from "@/api/entities";
import { EntityDetail } from "@/components/assets/entity-detail";
import { AppFrame } from "@/components/layout/app-frame";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import { Textarea } from "@/components/ui/textarea";
import { groupRelations } from "@/lib/relations";
import type {
  Capabilities,
  ConfigResponse,
  Entity,
  EntityDatesResponse,
  EntityDetailResponse,
  ExternalCandidate,
  TypeConfig,
} from "@/types/api";

type FrontmatterValue = null | boolean | number | string | FrontmatterValue[] | FrontmatterObject;
type FrontmatterObject = { [key: string]: FrontmatterValue | undefined };

export function EntityPage() {
  const { id } = useParams();
  const navigate = useNavigate();
  const [state, setState] = useState<{
    detail?: EntityDetailResponse;
    dates?: EntityDatesResponse;
    config?: ConfigResponse;
    capabilities?: Capabilities;
    loading: boolean;
    error?: string;
  }>({ loading: true });
  const [editOpen, setEditOpen] = useState(false);
  const [matchOpen, setMatchOpen] = useState(false);
  const [draftFrontmatter, setDraftFrontmatter] = useState<Record<string, FrontmatterValue>>({});
  const [bodyText, setBodyText] = useState("");
  const [saving, setSaving] = useState(false);
  const [quickStatus, setQuickStatus] = useState("");
  const [quickProgress, setQuickProgress] = useState("");
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
  const statusField = typeConfig?.statusFields[0] ?? "status";
  const progressField = typeConfig?.progressFields[0] ?? inferProgressField(entity);
  const relationGroups = useMemo(
    () => groupRelations(state.detail?.relations ?? []),
    [state.detail],
  );

  useEffect(() => {
    if (!entity) return;
    setDraftFrontmatter(normalizeFrontmatter(entity.frontmatter));
    setBodyText(entity.body);
    setQuickStatus(entity.status ?? "");
    setQuickProgress(progressField ? String(entity.frontmatter[progressField] ?? "") : "");
    setExternalQuery(entity.title);
  }, [entity?.id, entity?.revision, progressField]);

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
      setState({ detail, dates, config, capabilities, loading: false });
    } catch (error: unknown) {
      setState((current) => ({ ...current, loading: false, error: errorMessage(error) }));
    }
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

  async function saveQuickUpdate() {
    if (!entity) return;
    const patch: Record<string, unknown> = {};
    patch[statusField] = quickStatus.trim() || null;
    if (progressField) patch[progressField] = numberOrString(quickProgress.trim());
    setSaving(true);
    try {
      await saveEntity(entity.id, {
        revision: entity.revision,
        frontmatter: patch,
      });
      await loadEntity();
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
    setSelectedFields(new Set(Object.keys(candidate.metadata ?? {})));
  }

  async function applyCandidate() {
    if (!entity || !selectedCandidate) return;
    const patch: Record<string, unknown> = {};
    const metadata = selectedCandidate.metadata ?? {};
    for (const field of selectedFields) {
      patch[field] = metadata[field];
    }
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
              statusField={statusField}
              statusOptions={statusOptions(typeConfig, entity.status)}
              progressField={progressField}
              quickStatus={quickStatus}
              quickProgress={quickProgress}
              onQuickStatusChange={setQuickStatus}
              onQuickProgressChange={setQuickProgress}
              onQuickSave={saveQuickUpdate}
              onEdit={() => setEditOpen(true)}
              onMatch={() => setMatchOpen((open) => !open)}
              onDelete={deleteCurrentEntity}
            />
            {editOpen ? (
              <EditPanel
                entity={entity}
                typeConfig={typeConfig}
                frontmatter={draftFrontmatter}
                bodyText={bodyText}
                saving={saving}
                onFrontmatterChange={setDraftFrontmatter}
                onBodyChange={setBodyText}
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
  statusField,
  statusOptions,
  progressField,
  quickStatus,
  quickProgress,
  onQuickStatusChange,
  onQuickProgressChange,
  onQuickSave,
  onEdit,
  onMatch,
  onDelete,
}: {
  entity: Entity;
  contentWritable: boolean;
  saving: boolean;
  statusField: string;
  statusOptions: string[];
  progressField?: string;
  quickStatus: string;
  quickProgress: string;
  onQuickStatusChange: (value: string) => void;
  onQuickProgressChange: (value: string) => void;
  onQuickSave: () => void;
  onEdit: () => void;
  onMatch: () => void;
  onDelete: () => void;
}) {
  return (
    <section className="rounded-md border p-3">
      <div className="flex flex-wrap items-end gap-2">
        <div className="min-w-36 flex-1">
          <label className="text-xs font-medium text-muted-foreground">{statusField}</label>
          <Select
            value={quickStatus}
            onChange={(event) => onQuickStatusChange(event.target.value)}
            disabled={!contentWritable}
            className="w-full"
            aria-label={statusField}
          >
            <option value="">No status</option>
            {statusOptions.map((option) => (
              <option key={option} value={option}>
                {option}
              </option>
            ))}
          </Select>
        </div>
        {progressField ? (
          <div className="min-w-36">
            <label className="text-xs font-medium text-muted-foreground">{progressField}</label>
            <NumberStepper
              value={quickProgress}
              onChange={onQuickProgressChange}
              disabled={!contentWritable}
              ariaLabel={progressField}
            />
          </div>
        ) : null}
        <Button type="button" variant="outline" onClick={onQuickSave} disabled={!contentWritable || saving}>
          <CheckIcon data-icon="inline-start" />
          Update
        </Button>
        <Button type="button" variant="outline" onClick={onEdit} disabled={!contentWritable}>
          <PencilIcon data-icon="inline-start" />
          Edit
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

function EditPanel({
  entity,
  typeConfig,
  frontmatter,
  bodyText,
  saving,
  onFrontmatterChange,
  onBodyChange,
  onSave,
  onCancel,
}: {
  entity: Entity;
  typeConfig?: TypeConfig;
  frontmatter: Record<string, FrontmatterValue>;
  bodyText: string;
  saving: boolean;
  onFrontmatterChange: (value: Record<string, FrontmatterValue>) => void;
  onBodyChange: (value: string) => void;
  onSave: () => void;
  onCancel: () => void;
}) {
  const [newFieldName, setNewFieldName] = useState("");
  const fieldSpecs = useMemo(
    () => editableFieldSpecs(typeConfig, frontmatter),
    [typeConfig, frontmatter],
  );

  function updateField(key: string, value: FrontmatterValue | undefined) {
    const next = { ...frontmatter };
    if (value === undefined) delete next[key];
    else next[key] = value;
    onFrontmatterChange(next);
  }

  function renameField(oldKey: string, nextKey: string) {
    const key = nextKey.trim();
    if (!key || key === oldKey || key in frontmatter) return;
    const next = { ...frontmatter };
    next[key] = next[oldKey];
    delete next[oldKey];
    onFrontmatterChange(next);
  }

  function addCustomField() {
    const key = newFieldName.trim();
    if (!key || key in frontmatter) return;
    onFrontmatterChange({ ...frontmatter, [key]: "" });
    setNewFieldName("");
  }

  return (
    <section className="rounded-md border p-4">
      <div className="mb-3 flex items-center justify-between gap-2">
        <div className="min-w-0">
          <h2 className="truncate text-sm font-semibold">Edit {entity.title}</h2>
          <p className="mt-1 truncate text-xs text-muted-foreground">{entity.path}</p>
        </div>
        <div className="flex items-center gap-2">
          <Button type="button" variant="outline" size="sm" onClick={onCancel}>
            <XIcon data-icon="inline-start" />
            Cancel
          </Button>
          <Button type="button" size="sm" onClick={onSave} disabled={saving}>
            <CheckIcon data-icon="inline-start" />
            {saving ? "Saving" : "Save"}
          </Button>
        </div>
      </div>

      <div className="grid gap-3 md:grid-cols-2">
        {fieldSpecs.map((field) => (
          <EditableFieldRow
            key={field.key}
            field={field}
            value={frontmatter[field.key]}
            statusOptions={statusOptions(typeConfig, entity.status)}
            onChange={(value) => updateField(field.key, value)}
            onRemove={field.configured ? undefined : () => updateField(field.key, undefined)}
            onRename={field.configured ? undefined : (key) => renameField(field.key, key)}
          />
        ))}
      </div>

      <div className="mt-3 flex flex-wrap items-end gap-2 rounded-md border border-dashed p-3">
        <label className="min-w-48 flex-1 text-sm font-medium">
          Custom field
          <Input
            value={newFieldName}
            onChange={(event) => setNewFieldName(event.target.value)}
            placeholder="field_name"
          />
        </label>
        <Button type="button" variant="outline" onClick={addCustomField} disabled={!newFieldName.trim()}>
          <PlusIcon data-icon="inline-start" />
          Add Field
        </Button>
      </div>

      <div className="mt-4">
        <label className="flex flex-col gap-1 text-sm font-medium">
          Markdown Body
          <Textarea
            className="min-h-72 font-mono text-xs"
            value={bodyText}
            onChange={(event) => onBodyChange(event.target.value)}
            spellCheck={false}
          />
        </label>
      </div>
    </section>
  );
}

type EditableFieldSpec = {
  key: string;
  label: string;
  kind: "text" | "status" | "number" | "boolean" | "list" | "json";
  configured: boolean;
};

function EditableFieldRow({
  field,
  value,
  statusOptions,
  onChange,
  onRemove,
  onRename,
}: {
  field: EditableFieldSpec;
  value: FrontmatterValue | undefined;
  statusOptions: string[];
  onChange: (value: FrontmatterValue) => void;
  onRemove?: () => void;
  onRename?: (key: string) => void;
}) {
  const [keyDraft, setKeyDraft] = useState(field.key);

  useEffect(() => {
    setKeyDraft(field.key);
  }, [field.key]);

  return (
    <div className="min-w-0 rounded-md border p-3">
      <div className="mb-2 flex min-w-0 items-center justify-between gap-2">
        {onRename ? (
          <Input
            value={keyDraft}
            onChange={(event) => setKeyDraft(event.target.value)}
            onBlur={() => onRename(keyDraft)}
            className="h-8 min-w-0 font-mono text-xs"
            aria-label="Custom field name"
          />
        ) : (
          <div className="min-w-0">
            <div className="truncate text-sm font-medium">{field.label}</div>
            <div className="truncate font-mono text-[11px] text-muted-foreground">{field.key}</div>
          </div>
        )}
        {onRemove ? (
          <Button type="button" variant="ghost" size="icon" onClick={onRemove} aria-label={`Remove ${field.key}`}>
            <Trash2Icon />
          </Button>
        ) : null}
      </div>
      <FieldValueInput
        field={field}
        value={value}
        statusOptions={statusOptions}
        onChange={onChange}
      />
    </div>
  );
}

function FieldValueInput({
  field,
  value,
  statusOptions,
  onChange,
}: {
  field: EditableFieldSpec;
  value: FrontmatterValue | undefined;
  statusOptions: string[];
  onChange: (value: FrontmatterValue) => void;
}) {
  if (field.kind === "status") {
    return (
      <Select
        value={valueToText(value)}
        onChange={(event) => onChange(event.target.value || null)}
        className="w-full"
        aria-label={field.label}
      >
        <option value="">No status</option>
        {statusOptions.map((option) => (
          <option key={option} value={option}>
            {option}
          </option>
        ))}
      </Select>
    );
  }

  if (field.kind === "number") {
    return (
      <NumberStepper
        value={valueToText(value)}
        onChange={(next) => onChange(numberOrString(next))}
        ariaLabel={field.label}
      />
    );
  }

  if (field.kind === "boolean") {
    return (
      <Select
        value={value === true ? "true" : value === false ? "false" : ""}
        onChange={(event) =>
          onChange(event.target.value === "" ? null : event.target.value === "true")
        }
        className="w-full"
        aria-label={field.label}
      >
        <option value="">Empty</option>
        <option value="true">Yes</option>
        <option value="false">No</option>
      </Select>
    );
  }

  if (field.kind === "list") {
    return (
      <Textarea
        value={Array.isArray(value) ? value.map(valueToText).join("\n") : valueToText(value)}
        onChange={(event) =>
          onChange(
            event.target.value
              .split("\n")
              .map((item) => item.trim())
              .filter(Boolean),
          )
        }
        className="min-h-24"
        aria-label={field.label}
      />
    );
  }

  if (field.kind === "json") {
    return (
      <Textarea
        value={JSON.stringify(value ?? null, null, 2)}
        onChange={(event) => onChange(parseJsonFieldValue(event.target.value))}
        className="min-h-24 font-mono text-xs"
        spellCheck={false}
        aria-label={field.label}
      />
    );
  }

  return (
    <Input
      value={valueToText(value)}
      onChange={(event) => onChange(event.target.value || null)}
      aria-label={field.label}
    />
  );
}

function NumberStepper({
  value,
  onChange,
  disabled = false,
  ariaLabel,
}: {
  value: string;
  onChange: (value: string) => void;
  disabled?: boolean;
  ariaLabel: string;
}) {
  const number = Number(value || 0);
  const current = Number.isFinite(number) ? number : 0;
  const step = (delta: number) => onChange(String(Math.max(0, current + delta)));
  const keyStep = (event: KeyboardEvent<HTMLButtonElement>, delta: number) => {
    if (event.key !== "Enter" && event.key !== " ") return;
    event.preventDefault();
    step(delta);
  };
  return (
    <div className="flex min-w-0 items-center gap-1">
      <Button
        type="button"
        variant="outline"
        size="icon"
        disabled={disabled}
        onClick={() => step(-1)}
        onKeyDown={(event) => keyStep(event, -1)}
        aria-label={`Decrease ${ariaLabel}`}
      >
        <MinusIcon />
      </Button>
      <Input
        type="number"
        min={0}
        step={1}
        value={value}
        onChange={(event) => onChange(event.target.value)}
        disabled={disabled}
        className="min-w-0 text-center tabular-nums"
        aria-label={ariaLabel}
      />
      <Button
        type="button"
        variant="outline"
        size="icon"
        disabled={disabled}
        onClick={() => step(1)}
        onKeyDown={(event) => keyStep(event, 1)}
        aria-label={`Increase ${ariaLabel}`}
      >
        <PlusIcon />
      </Button>
    </div>
  );
}

function ExternalMatchPanel({
  query,
  provider,
  candidates,
  selectedCandidate,
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
              {candidate.subtitle ? <div className="mt-1 text-xs text-muted-foreground">{candidate.subtitle}</div> : null}
              {candidate.brief ? <p className="mt-2 line-clamp-2 text-xs text-muted-foreground">{candidate.brief}</p> : null}
            </button>
          ))}
          {candidates.length === 0 ? <div className="rounded-md border p-4 text-sm text-muted-foreground">No candidates loaded</div> : null}
        </div>
        <div className="rounded-md border p-3">
          <h3 className="text-sm font-semibold">Selected Metadata</h3>
          {selectedCandidate ? (
            <div className="mt-3 flex flex-col gap-2">
              {Object.entries(selectedCandidate.metadata ?? {}).map(([field, value]) => (
                <label key={field} className="flex min-w-0 items-start gap-2 text-sm">
                  <input
                    type="checkbox"
                    checked={selectedFields.has(field)}
                    onChange={() => toggleField(field)}
                    className="mt-1"
                  />
                  <span className="min-w-0">
                    <span className="block font-medium">{field}</span>
                    <span className="block break-words text-xs text-muted-foreground">{formatMetadataValue(value)}</span>
                  </span>
                </label>
              ))}
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

function editableFieldSpecs(
  typeConfig: TypeConfig | undefined,
  frontmatter: Record<string, FrontmatterValue>,
) {
  const specs: EditableFieldSpec[] = [];
  const seen = new Set<string>();
  const addFields = (
    fields: string[] | undefined,
    labelPrefix: string,
    kind: EditableFieldSpec["kind"] = "text",
  ) => {
    for (const key of fields ?? []) {
      if (!key || seen.has(key) || isVirtualTitleField(key)) continue;
      seen.add(key);
      specs.push({
        key,
        label: labelPrefix ? `${labelPrefix}: ${key}` : humanizeField(key),
        kind,
        configured: true,
      });
    }
  };

  addFields(typeConfig?.idFields, "Stable ID");
  for (const [language, fields] of Object.entries(typeConfig?.titleLanguageFields ?? {})) {
    addFields(fields, `${language} title`);
  }
  addFields(typeConfig?.subtitleFields, "Subtitle");
  addFields(typeConfig?.imageFields, "Image");
  addFields(typeConfig?.statusFields, "Status", "status");
  addFields(typeConfig?.progressFields, "Progress", "number");
  addFields(typeConfig?.totalProgressFields, "Total", "number");
  addFields(typeConfig?.ratingFields, "Rating", "number");
  addFields(typeConfig?.dateRoles.planning, "Planning date");
  addFields(typeConfig?.dateRoles.completed, "Completed date");
  addFields(typeConfig?.externalRefFields, "External ref");
  addFields(typeConfig?.relationFields, "Relation", "list");

  for (const [key, value] of Object.entries(frontmatter)) {
    if (seen.has(key)) continue;
    seen.add(key);
    specs.push({
      key,
      label: humanizeField(key),
      kind: inferFieldKind(value),
      configured: false,
    });
  }

  return specs;
}

function statusOptions(typeConfig: TypeConfig | undefined, current?: string | null) {
  const options = new Set(typeConfig?.statusOptions ?? []);
  if (options.size === 0) {
    for (const option of ["Backlog", "Watching", "Playing", "Reading", "Completed", "Paused", "Dropped"]) {
      options.add(option);
    }
  }
  if (current) options.add(current);
  return [...options];
}

function inferProgressField(entity: Entity | undefined) {
  if (!entity) return undefined;
  for (const field of ["progress", "episode", "episodes_watched", "watched", "chapter", "chapters_read"]) {
    if (field in entity.frontmatter) return field;
  }
  return undefined;
}

function normalizeFrontmatter(value: Record<string, unknown>) {
  return Object.fromEntries(
    Object.entries(value).map(([key, item]) => [key, normalizeFrontmatterValue(item)]),
  ) as Record<string, FrontmatterValue>;
}

function normalizeFrontmatterValue(value: unknown): FrontmatterValue {
  if (value === null || ["boolean", "number", "string"].includes(typeof value)) {
    return value as FrontmatterValue;
  }
  if (Array.isArray(value)) {
    return value.map(normalizeFrontmatterValue);
  }
  if (typeof value === "object" && value) {
    return Object.fromEntries(
      Object.entries(value).map(([key, item]) => [key, normalizeFrontmatterValue(item)]),
    );
  }
  return String(value ?? "");
}

function inferFieldKind(value: FrontmatterValue | undefined): EditableFieldSpec["kind"] {
  if (typeof value === "number") return "number";
  if (typeof value === "boolean") return "boolean";
  if (Array.isArray(value)) {
    return value.every((item) => item === null || ["string", "number", "boolean"].includes(typeof item))
      ? "list"
      : "json";
  }
  if (typeof value === "object" && value !== null) return "json";
  return "text";
}

function valueToText(value: FrontmatterValue | undefined): string {
  if (value === null || value === undefined) return "";
  if (typeof value === "string") return value;
  if (typeof value === "number" || typeof value === "boolean") return String(value);
  return JSON.stringify(value);
}

function parseJsonFieldValue(value: string): FrontmatterValue {
  try {
    return normalizeFrontmatterValue(JSON.parse(value));
  } catch {
    return value;
  }
}

function frontmatterPatch(original: Record<string, unknown>, next: Record<string, FrontmatterValue>) {
  const patch: Record<string, unknown> = { ...next };
  for (const key of Object.keys(original)) {
    if (!(key in next)) patch[key] = null;
  }
  return patch;
}

function numberOrString(value: string) {
  if (!value) return null;
  const number = Number(value);
  return Number.isFinite(number) && String(number) === value ? number : value;
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
