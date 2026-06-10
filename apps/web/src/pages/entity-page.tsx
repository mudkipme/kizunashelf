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
import { getAppCapabilities, removeEntity, saveEntity } from "@/api/entities";
import { EntityDetail } from "@/components/assets/entity-detail";
import { ExternalMatchDialog } from "@/components/entities/external-match-dialog";
import { useExternalMatch } from "@/components/entities/use-external-match";
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
import { basenameValidationError, normalizeBasename } from "@/lib/basename";
import { groupRelations } from "@/lib/relations";
import type {
  Capabilities,
  ConfigResponse,
  Entity,
  EntityDatesResponse,
  EntityDetailResponse,
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
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    if (!id) return;
    void loadEntity();
  }, [id]);

  const entity = state.detail?.entity;
  const contentWritable = state.capabilities?.contentWritable !== false;
  const typeConfig = state.config?.types.find((type) => type.id === entity?.type);
  const external = useExternalMatch({
    typeConfig,
    entityType: entity?.type,
    defaultQuery: entity?.title,
    externalRefs: entity?.externalRefs,
    onError: (error) => setState((current) => ({ ...current, error })),
  });
  const relationGroups = useMemo(
    () => groupRelations(state.detail?.relations ?? []),
    [state.detail],
  );

  useEffect(() => {
    if (!entity) return;
    setRenameBasename(entity.basename);
    external.setQuery(entity.title);
  }, [entity?.id, entity?.revision, external.setQuery]);

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

  async function applyCandidate() {
    if (!entity || !external.selectedCandidate) return;
    const patch = external.selectedPatch();
    setSaving(true);
    try {
      await saveEntity(entity.id, {
        revision: entity.revision,
        frontmatter: patch,
      });
      external.setOpen(false);
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
              onMatch={() => external.setOpen(true)}
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
              open={external.open}
              query={external.query}
              provider={external.provider}
              candidates={external.candidates}
              selectedCandidate={external.selectedCandidate}
              metadataEntries={external.metadataEntries}
              selectedFields={external.selectedFields}
              providerOptions={external.providerOptions}
              externalSearchEnabled={external.externalSearchEnabled}
              existingExternalRefs={external.existingExternalRefs}
              currentValues={entity.frontmatter as Record<string, unknown>}
              searching={external.searching}
              applying={saving}
              contentWritable={contentWritable}
              emptyMessage={external.emptyMessage}
              onOpenChange={external.setOpen}
              onQueryChange={external.setQuery}
              onProviderChange={external.setProvider}
              onSearch={() => void external.search()}
              onRefreshRef={external.refreshFromExternalRef}
              onChooseCandidate={external.chooseCandidate}
              onSelectedFieldsChange={external.setSelectedFields}
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
