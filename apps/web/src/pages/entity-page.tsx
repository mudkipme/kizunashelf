import { useEffect, useMemo, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import {
  CheckIcon,
  DownloadIcon,
  FilePenLineIcon,
  ListPlusIcon,
  MoreHorizontalIcon,
  PencilIcon,
  PlusIcon,
  SearchIcon,
  Trash2Icon,
  XIcon,
} from "lucide-react";
import { useNavigate, useParams } from "react-router-dom";

import { errorMessage, isConflictError } from "@/api/client";
import { downloadAssets, removeEntity, saveEntity } from "@/api/entities";
import { addItemToList, addList } from "@/api/lists";
import { useInvalidateEntityData } from "@/api/invalidate-entity-data";
import {
  capabilitiesQuery,
  configQuery,
  entityDatesQuery,
  entityQuery,
  listsQuery,
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
import { isRemoteAsset } from "@/lib/asset-src";
import { basenameValidationError, normalizeBasename } from "@/lib/basename";
import { applyExternalBodySections } from "@/lib/external-metadata";
import { useTitleLanguage } from "@/lib/language";
import { groupRelations } from "@/lib/relations";
import { entityTitle } from "@/lib/title-language";
import type {
  Entity,
} from "@/types/api";

export function EntityPage() {
  const { id } = useParams();
  const navigate = useNavigate();
  const invalidateEntityData = useInvalidateEntityData();
  const detail = useQuery({ ...entityQuery(id ?? ""), enabled: Boolean(id) });
  const dates = useQuery({ ...entityDatesQuery(id ?? ""), enabled: Boolean(id) });
  const config = useQuery(configQuery());
  const providerCatalog = useQuery(providerCatalogQuery());
  const capabilities = useQuery(capabilitiesQuery());
  const language = useTitleLanguage();
  const [error, setError] = useState<string>();
  const [renameOpen, setRenameOpen] = useState(false);
  const [renameBasename, setRenameBasename] = useState("");
  const [addToListOpen, setAddToListOpen] = useState(false);
  const [saving, setSaving] = useState(false);

  const loading =
    detail.isPending ||
    dates.isPending ||
    config.isPending ||
    providerCatalog.isPending ||
    capabilities.isPending;
  const queryError =
    detail.error ?? dates.error ?? config.error ?? providerCatalog.error ?? capabilities.error;
  const entity = detail.data?.entity;
  const contentWritable = capabilities.data?.contentWritable !== false;
  const canDownloadCover =
    contentWritable &&
    capabilities.data?.assetDownloadEnabled === true &&
    isRemoteAsset(entity?.image);
  const typeConfig = config.data?.types.find((type) => type.id === entity?.type);
  const external = useExternalMatch({
    typeConfig,
    providerCatalog: providerCatalog.data,
    entityType: entity?.type,
    defaultQuery: entity ? entityTitle(entity, language) : undefined,
    externalRefs: entity?.externalRefs,
    assetDownloadEnabled: capabilities.data?.assetDownloadEnabled === true,
    onError: setError,
  });
  const relationGroups = useMemo(
    () => groupRelations(detail.data?.relations ?? []),
    [detail.data],
  );

  // A 409 means the entity changed on disk and the action's revision is stale.
  // Refetch so a retry uses the latest revision, and explain rather than dumping
  // a raw "409 …" string.
  function reportActionError(error: unknown) {
    if (isConflictError(error)) {
      setError("This entity changed on disk since it was loaded. Reloaded the latest version — please try again.");
      void detail.refetch();
    } else {
      setError(errorMessage(error));
    }
  }
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
    setSaving(true);
    try {
      const result = await saveEntity(entity.id, {
        revision: entity.revision,
        renameTo: nextBasename,
      });
      setRenameOpen(false);
      await invalidateEntityData();
      navigate(`/entities/${encodeURIComponent(result.entity.id)}`);
    } catch (error) {
      reportActionError(error);
    } finally {
      setSaving(false);
    }
  }

  async function applyCandidate() {
    if (!entity || !external.selectedCandidate) return;
    const patch = external.selectedPatch();
    const nextBody = applyExternalBodySections(entity.body, external.selectedBodyPatch());
    setSaving(true);
    try {
      const result = await saveEntity(entity.id, {
        revision: entity.revision,
        frontmatter: patch,
        body: nextBody === entity.body ? undefined : nextBody,
      });
      external.setOpen(false);
      await external.maybeDownloadCover(result.entity);
      await invalidateEntityData();
    } catch (error) {
      reportActionError(error);
    } finally {
      setSaving(false);
    }
  }

  async function downloadCover() {
    if (!entity) return;
    setSaving(true);
    try {
      const result = await downloadAssets(entity.id, { revision: entity.revision });
      await invalidateEntityData();
      const failures = result.results.filter((item) => item.status === "failed");
      if (failures.length > 0) {
        const reasons = failures
          .map((item) => item.message)
          .filter(Boolean)
          .join("; ");
        setError(reasons ? `Some images could not be downloaded: ${reasons}` : "Some images could not be downloaded");
      } else {
        setError(undefined);
      }
    } catch (error) {
      reportActionError(error);
    } finally {
      setSaving(false);
    }
  }

  async function deleteCurrentEntity() {
    if (!entity) return;
    setSaving(true);
    try {
      await removeEntity(entity.id, { revision: entity.revision });
      await invalidateEntityData();
      navigate("/library");
    } catch (error) {
      reportActionError(error);
    } finally {
      setSaving(false);
    }
  }

  return (
    <AppFrame error={error ?? (queryError ? errorMessage(queryError) : undefined)}>
      <div className="mx-auto flex w-full max-w-6xl flex-col gap-4 p-4">
        {loading ? (
          <div className="rounded-md border p-8 text-center text-sm text-muted-foreground">Loading</div>
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
              actions={
                <EntityActions
                  entity={entity}
                  contentWritable={contentWritable}
                  saving={saving}
                  showDownloadCover={canDownloadCover}
                  onEdit={() => navigate(`/entities/${encodeURIComponent(entity.id)}/edit`)}
                  onRename={() => setRenameOpen(true)}
                  onAddToList={() => setAddToListOpen(true)}
                  onMatch={() => external.setOpen(true)}
                  onDownloadCover={downloadCover}
                  onDelete={deleteCurrentEntity}
                />
              }
            />
            <AddToListDialog
              open={addToListOpen}
              onOpenChange={setAddToListOpen}
              entityId={entity.id}
              entityName={entityTitle(entity, language)}
              contentWritable={contentWritable}
              onError={setError}
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
  showDownloadCover,
  onEdit,
  onRename,
  onAddToList,
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
  onAddToList: () => void;
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
          <DropdownMenuItem onSelect={onAddToList} disabled={!contentWritable}>
            <ListPlusIcon />
            Add to list
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
              : "Content writes are disabled. Editing actions are unavailable."}
          </DropdownMenuLabel>
        </DropdownMenuContent>
      </DropdownMenu>

      <AlertDialog open={deleteOpen} onOpenChange={setDeleteOpen}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Move to trash?</AlertDialogTitle>
            <AlertDialogDescription>
              This moves {entityTitle(entity, language)} to the vault's <code>.trash</code> folder. You can restore it from there if needed.
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

function AddToListDialog({
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
  const lists = useQuery({ ...listsQuery(), enabled: open });
  const [addedIds, setAddedIds] = useState<Set<string>>(new Set());
  const [pendingId, setPendingId] = useState<string>();
  const [newName, setNewName] = useState("");
  const [creating, setCreating] = useState(false);

  useEffect(() => {
    if (open) {
      setAddedIds(new Set());
      setNewName("");
    }
  }, [open]);

  const newNameError = newName.trim() ? basenameValidationError(normalizeBasename(newName)) : undefined;
  const items = lists.data?.items ?? [];

  async function add(listId: string) {
    setPendingId(listId);
    onError(undefined);
    try {
      await addItemToList(listId, { entityId });
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: queryKeys.list(listId) }),
        queryClient.invalidateQueries({ queryKey: queryKeys.lists }),
      ]);
      setAddedIds((current) => new Set(current).add(listId));
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
      await queryClient.invalidateQueries({ queryKey: queryKeys.lists });
      setNewName("");
      await add(created.id);
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
          <DialogTitle>Add to list</DialogTitle>
          <DialogDescription className="truncate">Add {entityName} to one of your lists.</DialogDescription>
        </DialogHeader>
        <div className="flex max-h-72 flex-col gap-1 overflow-auto">
          {lists.isPending ? (
            <p className="p-3 text-center text-sm text-muted-foreground">Loading</p>
          ) : items.length === 0 ? (
            <p className="p-3 text-center text-sm text-muted-foreground">No lists yet. Create one below.</p>
          ) : (
            items.map((list) => {
              const added = addedIds.has(list.id);
              return (
                <div key={list.id} className="flex items-center gap-2 rounded-md p-1">
                  <span className="min-w-0 flex-1 truncate text-sm font-medium">{list.name}</span>
                  {added ? (
                    <span className="flex items-center gap-1 text-xs text-muted-foreground">
                      <CheckIcon className="size-4" />
                      Added
                    </span>
                  ) : (
                    <Button
                      type="button"
                      size="sm"
                      variant="outline"
                      disabled={!contentWritable || pendingId === list.id}
                      onClick={() => void add(list.id)}
                    >
                      <PlusIcon data-icon="inline-start" />
                      Add
                    </Button>
                  )}
                </div>
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
            File path stays in the same folder. Only the Markdown basename changes.
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
