import { useEffect, useMemo, useState } from "react";
import { getConfig, getEntity, getEntityDates } from "@kizunashelf/api-contract";
import {
  CheckIcon,
  FilePenLineIcon,
  PencilIcon,
  SearchIcon,
  Trash2Icon,
  XIcon,
} from "lucide-react";
import { useNavigate, useParams } from "react-router-dom";

import { apiFetch, errorMessage } from "@/api/client";
import { getAppCapabilities, removeEntity, saveEntity, searchSources } from "@/api/entities";
import { EntityDetail } from "@/components/assets/entity-detail";
import { ExternalMatchDialog } from "@/components/entities/external-match-dialog";
import { AppFrame } from "@/components/layout/app-frame";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger,
} from "@/components/ui/alert-dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  candidateMetadataEntries,
  candidateMetadataPatch,
  candidateMetadataPreviewEntries,
  externalProviderPriority,
} from "@/lib/external-metadata";
import { basenameValidationError, normalizeBasename } from "@/lib/basename";
import { groupRelations } from "@/lib/relations";
import type {
  Capabilities,
  ConfigResponse,
  Entity,
  EntityDatesResponse,
  EntityDetailResponse,
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
    loading: boolean;
    error?: string;
  }>({ loading: true });
  const [renameOpen, setRenameOpen] = useState(false);
  const [renameBasename, setRenameBasename] = useState("");
  const [matchOpen, setMatchOpen] = useState(false);
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
  const providerOptions = useMemo(() => externalProviderPriority(typeConfig), [typeConfig]);
  const externalSearchEnabled = providerOptions.length > 0;
  const existingExternalRefs = useMemo(
    () =>
      typeConfig && entity
        ? (typeConfig.fields ?? [])
            .filter((field) => field.fieldType === "externalRef" && field.externalRef)
            .map((field) => ({
              field: field.field,
              provider: field.externalRef ?? "",
              value: entity.externalRefs[field.field],
            }))
            .filter((item): item is { field: string; provider: string; value: string } =>
              Boolean(item.provider && item.value && providerOptions.includes(item.provider)),
            )
        : [],
    [entity, providerOptions, typeConfig],
  );
  const selectedCandidateEntries = useMemo(
    () => (selectedCandidate ? candidateMetadataPreviewEntries(selectedCandidate, typeConfig) : []),
    [selectedCandidate, typeConfig],
  );
  const relationGroups = useMemo(
    () => groupRelations(state.detail?.relations ?? []),
    [state.detail],
  );

  useEffect(() => {
    if (!entity) return;
    setRenameBasename(entity.basename);
    setExternalQuery(entity.title);
  }, [entity?.id, entity?.revision]);

  useEffect(() => {
    if (providerOptions.length === 0) {
      if (externalProvider !== "all") setExternalProvider("all");
      return;
    }
    if (providerOptions.length === 1) {
      if (externalProvider !== providerOptions[0]) setExternalProvider(providerOptions[0]);
      return;
    }
    if (externalProvider !== "all" && !providerOptions.includes(externalProvider)) {
      setExternalProvider("all");
    }
  }, [externalProvider, providerOptions]);

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

  async function searchExternal(providerOverride?: string, queryOverride?: string) {
    if (!entity) return;
    const selectedProvider = providerOverride ?? externalProvider;
    const selectedQuery = queryOverride ?? externalQuery;
    if (providerOptions.length === 0) return;
    if (selectedProvider !== "all" && !providerOptions.includes(selectedProvider)) return;
    setExternalSearching(true);
    setSelectedCandidate(undefined);
    setSelectedFields(new Set());
    try {
      const result = await searchSources({
        provider: selectedProvider,
        q: selectedQuery || entity.title,
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

  function refreshFromExternalRef(provider: string, value: string) {
    setMatchOpen(true);
    setExternalProvider(provider);
    setExternalQuery(value);
    void searchExternal(provider, value);
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
              onEdit={() => navigate(`/entities/${encodeURIComponent(entity.id)}/edit`)}
              onRename={() => setRenameOpen((open) => !open)}
              onMatch={() => setMatchOpen(true)}
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
            <ExternalMatchDialog
              open={matchOpen}
              query={externalQuery}
              provider={externalProvider}
              candidates={externalCandidates}
              selectedCandidate={selectedCandidate}
              metadataEntries={selectedCandidateEntries}
              selectedFields={selectedFields}
              providerOptions={providerOptions}
              externalSearchEnabled={externalSearchEnabled}
              existingExternalRefs={existingExternalRefs}
              currentValues={entity.frontmatter as Record<string, unknown>}
              searching={externalSearching}
              applying={saving}
              contentWritable={contentWritable}
              onOpenChange={setMatchOpen}
              onQueryChange={setExternalQuery}
              onProviderChange={setExternalProvider}
              onSearch={searchExternal}
              onRefreshRef={refreshFromExternalRef}
              onChooseCandidate={chooseCandidate}
              onSelectedFieldsChange={setSelectedFields}
              onApply={applyCandidate}
            />
            <EntityDetail
              entity={entity}
              relations={state.detail?.relations ?? []}
              relatedEntities={state.detail?.relatedEntities ?? []}
              relationGroups={relationGroups}
              dates={state.dates}
              typeConfig={typeConfig}
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
        <AlertDialog>
          <AlertDialogTrigger asChild>
            <Button type="button" variant="outline" disabled={!contentWritable || saving}>
              <Trash2Icon data-icon="inline-start" />
              Delete
            </Button>
          </AlertDialogTrigger>
          <AlertDialogContent>
            <AlertDialogHeader>
              <AlertDialogTitle>Move to trash?</AlertDialogTitle>
              <AlertDialogDescription>
                This moves {entity.title} to KizunaShelf trash. You can restore it from the backup location if needed.
              </AlertDialogDescription>
            </AlertDialogHeader>
            <AlertDialogFooter>
              <AlertDialogCancel disabled={saving}>Cancel</AlertDialogCancel>
              <AlertDialogAction onClick={onDelete} disabled={saving}>
                Move to Trash
              </AlertDialogAction>
            </AlertDialogFooter>
          </AlertDialogContent>
        </AlertDialog>
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
