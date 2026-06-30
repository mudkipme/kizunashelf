import { useEffect, useMemo, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import {
  CheckCircle2Icon,
  CheckIcon,
  CircleIcon,
  DownloadIcon,
  FilePenLineIcon,
  ListChecksIcon,
  MoreHorizontalIcon,
  PencilIcon,
  PlusIcon,
  SearchIcon,
  Trash2Icon,
  XIcon,
} from "lucide-react";
import { useNavigate, useParams } from "react-router-dom";

import { errorMessage } from "@/api/client";
import { downloadAssets, removeEntity, saveEntity } from "@/api/entities";
import { addItemToList, addList, removeItemFromList } from "@/api/lists";
import { useInvalidateEntityData } from "@/api/invalidate-entity-data";
import {
  configQuery,
  entityDatesQuery,
  entityListsQuery,
  entityQuery,
  providerCatalogQuery,
  queryKeys,
} from "@/api/queries";
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
} from "@/components/ui/alert-dialog";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Input } from "@/components/ui/input";
import { Placeholder } from "@/components/ui/placeholder";
import { reportEntityError, useEntityMutation } from "@/hooks/use-entity-mutation";
import { isRemoteAsset } from "@/lib/asset-src";
import { basenameValidationError, normalizeBasename } from "@/lib/basename";
import { CONTENT_WRITES_DISABLED, useCapabilities } from "@/lib/capabilities";
import { setEpisodeWatched } from "@/api/episodes";
import { applyExternalBodySections } from "@/lib/external-metadata";
import { useTitleLanguage } from "@/lib/language";
import { groupRelations } from "@/lib/relations";
import { coverTypeIds, fieldLabelsByType, typeLabelsById } from "@/lib/type-config";
import { entityTitle } from "@/lib/title-language";
import type { Entity } from "@/types/api";

export function EntityPage() {
  const { id } = useParams();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const invalidateEntityData = useInvalidateEntityData();
  const detail = useQuery({ ...entityQuery(id ?? ""), enabled: Boolean(id) });
  const dates = useQuery({ ...entityDatesQuery(id ?? ""), enabled: Boolean(id) });
  const config = useQuery(configQuery());
  const providerCatalog = useQuery(providerCatalogQuery());
  const capabilities = useCapabilities();
  const language = useTitleLanguage();
  const { saving, error, setError, run } = useEntityMutation();
  const [renameOpen, setRenameOpen] = useState(false);
  const [renameBasename, setRenameBasename] = useState("");
  const [manageListsOpen, setManageListsOpen] = useState(false);
  const [episodesSaving, setEpisodesSaving] = useState(false);

  const loading =
    detail.isPending ||
    dates.isPending ||
    config.isPending ||
    providerCatalog.isPending ||
    capabilities.isPending;
  const queryError =
    detail.error ?? dates.error ?? config.error ?? providerCatalog.error ?? capabilities.error;
  const entity = detail.data?.entity;
  const contentWritable = capabilities.contentWritable;
  const canDownloadCover =
    contentWritable && capabilities.assetDownloadEnabled && isRemoteAsset(entity?.image);
  const typeConfig = config.data?.types.find((type) => type.id === entity?.type);
  const external = useExternalMatch({
    typeConfig,
    providerCatalog: providerCatalog.data,
    entityType: entity?.type,
    defaultQuery: entity ? entityTitle(entity, language) : undefined,
    externalRefs: entity?.externalRefs,
    assetDownloadEnabled: capabilities.assetDownloadEnabled,
    onError: setError,
  });
  const relationGroups = useMemo(
    () => groupRelations(detail.data?.relations ?? []),
    [detail.data],
  );
  const labelsByType = useMemo(() => fieldLabelsByType(config.data?.types), [config.data]);
  const typeLabels = useMemo(() => typeLabelsById(config.data?.types), [config.data]);
  // Type ids that declare a cover field — their connections render as a cover grid.
  const coverTypes = useMemo(() => coverTypeIds(config.data?.types), [config.data]);

  // A 409 means the entity changed on disk and the action's revision is stale:
  // refetch so a retry uses the latest revision (see `useEntityMutation`).
  const refetchOnConflict = () => void detail.refetch();
  const { setQuery: setMatchQuery } = external;

  useEffect(() => {
    if (!entity) return;
    setRenameBasename(entity.basename);
    setMatchQuery(entityTitle(entity, language));
  }, [entity, language, setMatchQuery]);

  async function saveRename() {
    if (!entity) return;
    const nextBasename = normalizeBasename(renameBasename);
    const validationError = basenameValidationError(nextBasename);
    setRenameBasename(nextBasename);
    if (validationError) {
      setError(validationError);
      return;
    }
    if (nextBasename === entity.basename) {
      setRenameOpen(false);
      return;
    }
    await run(async () => {
      const result = await saveEntity(entity.id, {
        revision: entity.revision,
        renameTo: nextBasename,
      });
      setRenameOpen(false);
      await invalidateEntityData();
      navigate(`/entities/${encodeURIComponent(result.entity.id)}`);
    }, { onConflict: refetchOnConflict });
  }

  async function applyCandidate() {
    if (!entity || !external.selectedCandidate) return;
    const patch = external.selectedPatch();
    const nextBody = applyExternalBodySections(entity.body, external.selectedBodyPatch());
    await run(async () => {
      const result = await saveEntity(entity.id, {
        revision: entity.revision,
        frontmatter: patch,
        body: nextBody === entity.body ? undefined : nextBody,
      });
      external.setOpen(false);
      await external.maybeDownloadCover(result.entity);
      await invalidateEntityData();
    }, { onConflict: refetchOnConflict });
  }

  async function downloadCover() {
    if (!entity) return;
    await run(async () => {
      const result = await downloadAssets(entity.id, { revision: entity.revision });
      await invalidateEntityData();
      const failures = result.results.filter((item) => item.status === "failed");
      if (failures.length > 0) {
        const reasons = failures
          .map((item) => item.message)
          .filter(Boolean)
          .join("; ");
        setError(reasons ? `Some images could not be downloaded: ${reasons}` : "Some images could not be downloaded");
      }
    }, { onConflict: refetchOnConflict });
  }

  async function deleteCurrentEntity() {
    if (!entity) return;
    await run(async () => {
      await removeEntity(entity.id, { revision: entity.revision });
      await invalidateEntityData();
      navigate("/library");
    }, { onConflict: refetchOnConflict });
  }

  async function toggleEpisodeWatched(group: string, key: string, index: number, watched: boolean) {
    if (!entity) return;
    setEpisodesSaving(true);
    try {
      // Sends only the changed episode (group + key, with index as the fallback
      // locator); the core stamps/clears the ✅ completion date and returns the
      // refreshed detail (with the new revision).
      const updated = await setEpisodeWatched(entity.id, {
        revision: entity.revision,
        group,
        key,
        index,
        watched,
      });
      queryClient.setQueryData(queryKeys.entity(entity.id), updated);
      // Refresh the resident watched/total badge in list views.
      void queryClient.invalidateQueries({ queryKey: ["entities"] });
      setError(undefined);
    } catch (error) {
      reportEntityError(error, setError, refetchOnConflict);
    } finally {
      setEpisodesSaving(false);
    }
  }

  return (
    <AppFrame error={error ?? (queryError ? errorMessage(queryError) : undefined)}>
      <div className="mx-auto flex w-full max-w-6xl flex-col gap-4 p-4">
        {loading ? (
          <Placeholder>Loading</Placeholder>
        ) : entity ? (
          <>
            <RenameDialog
              open={renameOpen}
              onOpenChange={(open) => {
                setRenameOpen(open);
                if (!open) setRenameBasename(entity.basename);
              }}
              currentBasename={entity.basename}
              basename={renameBasename}
              saving={saving}
              disabled={!contentWritable}
              onBasenameChange={setRenameBasename}
              onSave={saveRename}
            />
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
              currentValues={entity.frontmatter as Record<string, unknown>}
              bodyText={entity.body}
              searching={external.searching}
              applying={saving}
              contentWritable={contentWritable}
              coverDownloadAvailable={external.coverDownloadAvailable}
              downloadCover={external.downloadAfterApply}
              onDownloadCoverChange={external.setDownloadAfterApply}
              emptyMessage={external.emptyMessage}
              onOpenChange={external.setOpen}
              onQueryChange={external.setQuery}
              onProviderChange={external.setProvider}
              onSearch={() => void external.search()}
              onRefreshRef={external.refreshFromExternalRef}
              onChooseCandidate={external.chooseCandidate}
              onSelectedFieldsChange={external.setSelectedFields}
              onSelectedBodySectionsChange={external.setSelectedBodySections}
              onApply={applyCandidate}
            />
            <EntityDetail
              entity={entity}
              relations={detail.data?.relations ?? []}
              relatedEntities={detail.data?.relatedEntities ?? []}
              relationGroups={relationGroups}
              dates={dates.data}
              typeConfig={typeConfig}
              episodes={detail.data?.episodes ?? undefined}
              episodesSaving={episodesSaving}
              notesBody={detail.data?.notesBody}
              contentWritable={contentWritable}
              labelsByType={labelsByType}
              typeLabels={typeLabels}
              coverTypes={coverTypes}
              onToggleEpisode={toggleEpisodeWatched}
              actions={
                <EntityActions
                  entity={entity}
                  contentWritable={contentWritable}
                  saving={saving}
                  showDownloadCover={canDownloadCover}
                  onEdit={() => navigate(`/entities/${encodeURIComponent(entity.id)}/edit`)}
                  onRename={() => setRenameOpen(true)}
                  onManageLists={() => setManageListsOpen(true)}
                  onMatch={() => external.setOpen(true)}
                  onDownloadCover={downloadCover}
                  onDelete={deleteCurrentEntity}
                />
              }
            />
            <ManageListsDialog
              open={manageListsOpen}
              onOpenChange={setManageListsOpen}
              entityId={entity.id}
              entityName={entityTitle(entity, language)}
              contentWritable={contentWritable}
              onError={setError}
            />
          </>
        ) : (
          <Placeholder>
            Entity not found
          </Placeholder>
        )}
      </div>
    </AppFrame>
  );
}

function EntityActions({
  entity,
  contentWritable,
  saving,
  showDownloadCover,
  onEdit,
  onRename,
  onManageLists,
  onMatch,
  onDownloadCover,
  onDelete,
}: {
  entity: Entity;
  contentWritable: boolean;
  saving: boolean;
  showDownloadCover: boolean;
  onEdit: () => void;
  onRename: () => void;
  onManageLists: () => void;
  onMatch: () => void;
  onDownloadCover: () => void;
  onDelete: () => void;
}) {
  const [deleteOpen, setDeleteOpen] = useState(false);
  const language = useTitleLanguage();

  return (
    <>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button type="button" variant="outline" size="sm" aria-label="Actions">
            <MoreHorizontalIcon />
            <span className="hidden sm:inline">Actions</span>
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" className="w-64">
          <DropdownMenuItem onSelect={onEdit} disabled={!contentWritable}>
            <PencilIcon />
            Edit
          </DropdownMenuItem>
          <DropdownMenuItem onSelect={onRename} disabled={!contentWritable || saving}>
            <FilePenLineIcon />
            Rename
          </DropdownMenuItem>
          <DropdownMenuItem onSelect={onManageLists} disabled={!contentWritable}>
            <ListChecksIcon />
            Manage lists
          </DropdownMenuItem>
          <DropdownMenuItem onSelect={onMatch}>
            <SearchIcon />
            Match
          </DropdownMenuItem>
          {showDownloadCover ? (
            <DropdownMenuItem onSelect={onDownloadCover} disabled={saving}>
              <DownloadIcon />
              Download cover
            </DropdownMenuItem>
          ) : null}
          <DropdownMenuSeparator />
          <DropdownMenuItem
            variant="destructive"
            disabled={!contentWritable || saving}
            onSelect={() => setDeleteOpen(true)}
          >
            <Trash2Icon />
            Delete
          </DropdownMenuItem>
          <DropdownMenuSeparator />
          <DropdownMenuLabel className="font-normal break-all text-xs text-muted-foreground">
            {contentWritable
              ? entity.path
              : `${CONTENT_WRITES_DISABLED} Editing actions are unavailable.`}
          </DropdownMenuLabel>
        </DropdownMenuContent>
      </DropdownMenu>

      <AlertDialog open={deleteOpen} onOpenChange={setDeleteOpen}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Move to trash?</AlertDialogTitle>
            <AlertDialogDescription>
              This moves {entityTitle(entity, language)} to the Trash. You can restore it later if you need it.
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
    </>
  );
}

function ManageListsDialog({
  open,
  onOpenChange,
  entityId,
  entityName,
  contentWritable,
  onError,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  entityId: string;
  entityName: string;
  contentWritable: boolean;
  onError: (message?: string) => void;
}) {
  const queryClient = useQueryClient();
  // Membership-annotated list of every list (each carries `contains`).
  const lists = useQuery({ ...entityListsQuery(entityId), enabled: open });
  const [pendingId, setPendingId] = useState<string>();
  const [newName, setNewName] = useState("");
  const [creating, setCreating] = useState(false);

  useEffect(() => {
    if (open) setNewName("");
  }, [open]);

  const newNameError = newName.trim() ? basenameValidationError(normalizeBasename(newName)) : undefined;
  const items = lists.data?.items ?? [];

  // Refetch the membership view (and any open detail) after a change. The
  // `["lists"]` prefix covers both the plain index and this entity-scoped query.
  function invalidate(listId: string) {
    return Promise.all([
      queryClient.invalidateQueries({ queryKey: queryKeys.lists }),
      queryClient.invalidateQueries({ queryKey: queryKeys.list(listId) }),
    ]);
  }

  async function toggle(listId: string, contains: boolean) {
    setPendingId(listId);
    onError(undefined);
    try {
      if (contains) await removeItemFromList(listId, entityId);
      else await addItemToList(listId, { entityId });
      await invalidate(listId);
    } catch (error) {
      onError(errorMessage(error));
    } finally {
      setPendingId(undefined);
    }
  }

  async function createAndAdd() {
    const name = normalizeBasename(newName);
    if (!name.trim() || newNameError) return;
    setCreating(true);
    onError(undefined);
    try {
      const created = await addList({ name });
      setNewName("");
      await addItemToList(created.id, { entityId });
      await invalidate(created.id);
    } catch (error) {
      onError(errorMessage(error));
    } finally {
      setCreating(false);
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Manage lists</DialogTitle>
          <DialogDescription className="truncate">Choose which lists {entityName} belongs to.</DialogDescription>
        </DialogHeader>
        <div className="flex max-h-72 flex-col gap-1 overflow-auto">
          {lists.isPending ? (
            <p className="p-3 text-center text-sm text-muted-foreground">Loading</p>
          ) : items.length === 0 ? (
            <p className="p-3 text-center text-sm text-muted-foreground">No lists yet. Create one below.</p>
          ) : (
            items.map((list) => {
              const contains = list.contains === true;
              return (
                <button
                  key={list.id}
                  type="button"
                  disabled={!contentWritable || pendingId === list.id}
                  onClick={() => void toggle(list.id, contains)}
                  className="flex items-center gap-2 rounded-md p-2 text-left transition-colors hover:bg-accent disabled:opacity-60"
                >
                  <span className="min-w-0 flex-1">
                    <span className="block truncate text-sm font-medium">{list.name}</span>
                    <span className="block text-xs text-muted-foreground">
                      {list.itemCount} {list.itemCount === 1 ? "item" : "items"}
                    </span>
                  </span>
                  {contains ? (
                    <CheckCircle2Icon className="size-5 shrink-0 text-primary" />
                  ) : (
                    <CircleIcon className="size-5 shrink-0 text-muted-foreground" />
                  )}
                </button>
              );
            })
          )}
        </div>
        <form
          onSubmit={(event) => {
            event.preventDefault();
            void createAndAdd();
          }}
          className="flex flex-col gap-2 border-t pt-3"
        >
          <label className="text-sm font-medium">New list</label>
          <div className="flex gap-2">
            <Input
              value={newName}
              onChange={(event) => setNewName(event.target.value)}
              placeholder="Watchlist"
              disabled={!contentWritable || creating}
              aria-invalid={Boolean(newNameError)}
            />
            <Button type="submit" disabled={!contentWritable || creating || !newName.trim() || Boolean(newNameError)}>
              <PlusIcon data-icon="inline-start" />
              {creating ? "Creating" : "Create & add"}
            </Button>
          </div>
          {newNameError ? <p className="text-xs text-destructive">{newNameError}</p> : null}
        </form>
        <DialogFooter>
          <Button type="button" variant="outline" onClick={() => onOpenChange(false)}>
            <XIcon data-icon="inline-start" />
            Done
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function RenameDialog({
  open,
  onOpenChange,
  currentBasename,
  basename,
  saving,
  disabled,
  onBasenameChange,
  onSave,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  currentBasename: string;
  basename: string;
  saving: boolean;
  disabled: boolean;
  onBasenameChange: (value: string) => void;
  onSave: () => void;
}) {
  const normalizedBasename = normalizeBasename(basename);
  const validationError = basenameValidationError(basename);
  const unchanged = normalizedBasename === currentBasename;
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-sm:inset-0 max-sm:flex max-sm:max-w-none max-sm:translate-x-0 max-sm:translate-y-0 max-sm:flex-col max-sm:rounded-none max-sm:border-0">
        <DialogHeader>
          <DialogTitle>Rename</DialogTitle>
          <DialogDescription>
            Only the name changes — everything else stays the same.
          </DialogDescription>
        </DialogHeader>
        <form
          onSubmit={(event) => {
            event.preventDefault();
            onSave();
          }}
          className="flex flex-col gap-2"
        >
          <label className="text-sm font-medium">
            Basename
            <Input
              value={basename}
              onChange={(event) => onBasenameChange(event.target.value)}
              onBlur={() => onBasenameChange(normalizedBasename)}
              disabled={disabled || saving}
              aria-invalid={Boolean(validationError)}
            />
          </label>
          {validationError ? <p className="text-xs text-destructive">{validationError}</p> : null}
          <DialogFooter className="mt-2">
            <Button
              type="button"
              variant="outline"
              onClick={() => onOpenChange(false)}
              disabled={saving}
            >
              <XIcon data-icon="inline-start" />
              Cancel
            </Button>
            <Button
              type="submit"
              disabled={disabled || saving || Boolean(validationError) || unchanged}
            >
              <CheckIcon data-icon="inline-start" />
              {saving ? "Renaming" : "Rename"}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
