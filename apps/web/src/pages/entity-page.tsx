import { plural } from "@lingui/core/macro";
import { Plural, Trans, useLingui } from "@lingui/react/macro";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import {
  CheckCircle2Icon,
  CircleIcon,
  DownloadIcon,
  FilePenLineIcon,
  ListChecksIcon,
  MoreHorizontalIcon,
  NotebookPenIcon,
  PencilIcon,
  PlusIcon,
  SearchIcon,
  Trash2Icon,
  XIcon,
} from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { toast } from "sonner";

import { errorMessage } from "@/api/client";
import { applyMatch, downloadAssets, removeEntity, saveEntity } from "@/api/entities";
import { setEpisodeWatched } from "@/api/episodes";
import { useInvalidateEntityData } from "@/api/invalidate-entity-data";
import { useInvalidateLists } from "@/api/invalidate-lists";
import { addItemToList, addList, removeItemFromList } from "@/api/lists";
import {
  configQuery,
  entityDatesQuery,
  entityListsQuery,
  entityQuery,
  providerCatalogQuery,
  queryKeys,
} from "@/api/queries";
import { setTaskDone } from "@/api/tasks";
import { EntityDetail } from "@/components/assets/entity-detail";
import { QuickLogDialog } from "@/components/assets/quick-log-dialog";
import { ExternalMatchDialog } from "@/components/entities/external-match-dialog";
import { useExternalMatch } from "@/components/entities/use-external-match";
import { AppFrame } from "@/components/layout/app-frame";
import { RenameDialog } from "@/components/rename-dialog";
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
import { todayLocal } from "@/lib/date";
import { useTitleLanguage } from "@/lib/language";
import { groupRelations } from "@/lib/relations";
import { entityTitle } from "@/lib/title-language";
import {
  coverTypeIds,
  entityFieldLabel,
  fieldLabelsByType,
  typeLabelsById,
} from "@/lib/type-config";
import type { Entity } from "@/types/api";

export function EntityPage() {
  const { t } = useLingui();
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
  const { saving, run } = useEntityMutation();
  const [renameOpen, setRenameOpen] = useState(false);
  const [manageListsOpen, setManageListsOpen] = useState(false);
  const [logOpen, setLogOpen] = useState(false);
  const [ratingField, setRatingField] = useState<string | null>(null);
  const [episodesSaving, setEpisodesSaving] = useState(false);
  const [tasksSaving, setTasksSaving] = useState(false);

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
  // The activities offered in the log dialog: always "progress", plus
  // "started"/"completed" when the log would *do* something for that kind —
  // either stamp a matching `dateRole` field, or flip a mapped `enumRole: status`
  // field (started → ongoing, completed → completed).
  const statusField = typeConfig?.fields.find((field) => field.enumRole === "status");
  const logKinds: ("progress" | "started" | "completed")[] = [
    "progress",
    ...(typeConfig?.fields.some((field) => field.dateRole === "started") ||
    (statusField?.statusValues?.ongoing?.length ?? 0) > 0
      ? (["started"] as const)
      : []),
    ...(typeConfig?.fields.some((field) => field.dateRole === "completed") ||
    (statusField?.statusValues?.completed?.length ?? 0) > 0
      ? (["completed"] as const)
      : []),
  ];
  // The Log button is shown only when the type is configured for daily-note
  // logging. Episode check-offs are independent — they only stamp the ✅
  // completion date via `/episodes/watch` and never write a daily-note line.
  const canLog = contentWritable && Boolean(entity) && Boolean(typeConfig?.log);
  const external = useExternalMatch({
    typeConfig,
    providerCatalog: providerCatalog.data,
    entityId: entity?.id,
    entityType: entity?.type,
    defaultQuery: entity ? entityTitle(entity, language) : undefined,
    externalRefs: entity?.externalRefs,
    assetDownloadEnabled: capabilities.assetDownloadEnabled,
  });
  const relationGroups = useMemo(() => groupRelations(detail.data?.relations ?? []), [detail.data]);
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
    setMatchQuery(entityTitle(entity, language));
  }, [entity, language, setMatchQuery]);

  async function saveRename(nextBasename: string) {
    if (!entity || !contentWritable) return;
    await run(
      async () => {
        const result = await saveEntity(entity.id, {
          revision: entity.revision,
          renameTo: nextBasename,
        });
        setRenameOpen(false);
        await invalidateEntityData();
        const updated = result.updatedLinks?.links ?? 0;
        toast.success(
          updated > 0
            ? t`Renamed — updated ${plural(updated, { one: "# link", other: "# links" })}`
            : t`Renamed`,
        );
        navigate(`/entities/${encodeURIComponent(result.entity.id)}`);
      },
      { onConflict: refetchOnConflict },
    );
  }

  // The core re-resolves the candidate and applies the selected fields/body
  // sections server-side; the client only names what to apply. The candidate
  // comes from the review response (already enriched with provider detail) so
  // apply doesn't fetch it from the provider a second time.
  async function applyCandidate() {
    if (!entity || !external.candidateForApply) return;
    const candidate = external.candidateForApply;
    await run(
      async () => {
        const result = await applyMatch(entity.id, {
          revision: entity.revision,
          candidate,
          fields: [...external.selectedFields],
          sections: [...external.selectedBodySections],
        });
        external.setOpen(false);
        await external.maybeDownloadCover(result.entity);
        await invalidateEntityData();
      },
      { onConflict: refetchOnConflict },
    );
  }

  async function downloadCover() {
    if (!entity) return;
    await run(
      async () => {
        const result = await downloadAssets(entity.id, { revision: entity.revision });
        await invalidateEntityData();
        const failures = result.results.filter((item) => item.status === "failed");
        if (failures.length > 0) {
          const reasons = failures
            .map((item) => item.message)
            .filter(Boolean)
            .join("; ");
          toast.error(
            reasons
              ? t`Some images could not be downloaded: ${reasons}`
              : t`Some images could not be downloaded`,
          );
        }
      },
      { onConflict: refetchOnConflict },
    );
  }

  async function deleteCurrentEntity() {
    if (!entity) return;
    await run(
      async () => {
        await removeEntity(entity.id, { revision: entity.revision });
        await invalidateEntityData();
        toast.success(t`Moved to trash`);
        navigate("/library");
      },
      { onConflict: refetchOnConflict },
    );
  }

  // Writes one episode through `/episodes/watch` (group + key, index as the fallback
  // locator): the core stamps/clears the ✅ date and returns the refreshed detail.
  // Independent of daily-note logging.
  async function persistEpisode(args: {
    group: string;
    key: string;
    index: number;
    watched: boolean;
    date: string;
  }) {
    if (!entity) return;
    setEpisodesSaving(true);
    try {
      const response = await setEpisodeWatched(entity.id, { revision: entity.revision, ...args });
      queryClient.setQueryData(queryKeys.entity(entity.id), response);
      // Refresh everything derived from the entity set (watched/total badges,
      // home/upcoming shelves, activity, calendar, …) via the shared helper so
      // this path can't drift from the other mutation flows.
      void invalidateEntityData();
    } catch (error) {
      reportEntityError(error, { onConflict: refetchOnConflict });
    } finally {
      setEpisodesSaving(false);
    }
  }

  // Checkbox toggle: stamp/clear the ✅ using the user's local date.
  function toggleEpisodeWatched(group: string, key: string, index: number, watched: boolean) {
    void persistEpisode({ group, key, index, watched, date: todayLocal() });
  }

  // Edit a checked episode's completion date (re-check with the chosen date).
  function setEpisodeDate(group: string, key: string, index: number, date: string) {
    void persistEpisode({ group, key, index, watched: true, date });
  }

  // Checks/unchecks a `- [ ]` the user wrote in the notes. The item is located by
  // its line in `notesBody` plus that line's source text (the core verifies the
  // pair before writing), and the ✅ is stamped with the user's local date.
  async function toggleNoteTask(line: number, text: string, done: boolean) {
    if (!entity) return;
    setTasksSaving(true);
    try {
      const response = await setTaskDone(entity.id, {
        revision: entity.revision,
        line,
        text,
        done,
        date: todayLocal(),
      });
      queryClient.setQueryData(queryKeys.entity(entity.id), response);
      void invalidateEntityData();
    } catch (error) {
      reportEntityError(error, { onConflict: refetchOnConflict });
    } finally {
      setTasksSaving(false);
    }
  }

  return (
    <AppFrame error={queryError ? errorMessage(queryError) : undefined}>
      {/* Full-bleed, like Library and Home: the window is the frame, so the page
          doesn't draw a second one inside it. `EntityDetail` owns the toolbar
          and the two scrolling panes, so the shell only has to hand it the
          height and stay out of the way. */}
      <div className="h-full min-h-full overflow-hidden">
        {loading ? (
          <div className="p-4">
            <Placeholder>
              <Trans>Loading…</Trans>
            </Placeholder>
          </div>
        ) : entity ? (
          <>
            <RenameDialog
              open={renameOpen}
              onOpenChange={setRenameOpen}
              title={t`Rename`}
              label={t`File name`}
              currentName={entity.basename}
              saving={saving}
              disabled={!contentWritable}
              onRename={saveRename}
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
              providers={external.providers}
              externalSearchEnabled={external.externalSearchEnabled}
              existingExternalRefs={external.existingExternalRefs}
              currentValues={entity.frontmatter as Record<string, unknown>}
              fieldLocks={external.fieldLocks}
              sectionLocks={external.sectionLocks}
              sectionModes={external.sectionModes}
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
              ratingField={ratingField}
              onRatingFieldChange={setRatingField}
              entity={entity}
              relations={detail.data?.relations ?? []}
              relatedEntities={detail.data?.relatedEntities ?? []}
              relationGroups={relationGroups}
              dates={dates.data}
              typeConfig={typeConfig}
              episodes={detail.data?.episodes ?? undefined}
              episodesSaving={episodesSaving}
              notesBody={detail.data?.notesBody}
              tasksSaving={tasksSaving}
              contentWritable={contentWritable}
              labelsByType={labelsByType}
              typeLabels={typeLabels}
              coverTypes={coverTypes}
              onToggleEpisode={toggleEpisodeWatched}
              onSetEpisodeDate={setEpisodeDate}
              onToggleTask={(line, text, done) => void toggleNoteTask(line, text, done)}
              actions={
                <div className="flex items-center gap-2">
                  {/* Edit is the page's main verb, so it sits in the toolbar
                      and only there — a duplicate in the ⋯ menu would just be a
                      second way to press the button already on screen. */}
                  <Button
                    type="button"
                    size="sm"
                    disabled={!contentWritable}
                    title={!contentWritable ? CONTENT_WRITES_DISABLED : undefined}
                    onClick={() => navigate(`/entities/${encodeURIComponent(entity.id)}/edit`)}
                  >
                    <PencilIcon data-icon="inline-start" />
                    <Trans>Edit</Trans>
                  </Button>
                  {canLog ? (
                    <Button
                      type="button"
                      variant="outline"
                      size="sm"
                      onClick={() => setLogOpen(true)}
                    >
                      <NotebookPenIcon data-icon="inline-start" />
                      <Trans>Log</Trans>
                    </Button>
                  ) : null}
                  <EntityActions
                    entity={entity}
                    contentWritable={contentWritable}
                    saving={saving}
                    showDownloadCover={canDownloadCover}
                    onRename={() => setRenameOpen(true)}
                    onManageLists={() => setManageListsOpen(true)}
                    onMatch={() => external.setOpen(true)}
                    onDownloadCover={downloadCover}
                    onDelete={deleteCurrentEntity}
                  />
                </div>
              }
            />
            {canLog ? (
              <QuickLogDialog
                onCompleted={() => {
                  const field =
                    entity.ratings?.find((rating) => rating.value == null)?.field ??
                    entity.ratings?.[0]?.field;
                  if (field)
                    toast.success(t`Completed`, {
                      action: { label: t`Rate`, onClick: () => setRatingField(field) },
                    });
                }}
                open={logOpen}
                onOpenChange={setLogOpen}
                entityId={entity.id}
                revision={entity.revision}
                kinds={logKinds}
                fieldLabel={(field) => entityFieldLabel(labelsByType, entity.type, field)}
              />
            ) : null}
            <ManageListsDialog
              open={manageListsOpen}
              onOpenChange={setManageListsOpen}
              entityId={entity.id}
              contentWritable={contentWritable}
            />
          </>
        ) : (
          <div className="p-4">
            <Placeholder>
              <Trans>Entity not found</Trans>
            </Placeholder>
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
  onRename: () => void;
  onManageLists: () => void;
  onMatch: () => void;
  onDownloadCover: () => void;
  onDelete: () => void;
}) {
  const { t } = useLingui();
  const [deleteOpen, setDeleteOpen] = useState(false);
  const language = useTitleLanguage();

  return (
    <>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button type="button" variant="outline" size="sm" aria-label={t`Actions`}>
            <MoreHorizontalIcon />
            <span className="hidden sm:inline">
              <Trans>Actions</Trans>
            </span>
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" className="w-64">
          <DropdownMenuItem onSelect={onRename} disabled={!contentWritable || saving}>
            <FilePenLineIcon />
            <Trans>Rename</Trans>
          </DropdownMenuItem>
          <DropdownMenuItem onSelect={onManageLists} disabled={!contentWritable}>
            <ListChecksIcon />
            <Trans>Manage lists</Trans>
          </DropdownMenuItem>
          <DropdownMenuItem onSelect={onMatch}>
            <SearchIcon />
            <Trans>Match</Trans>
          </DropdownMenuItem>
          {showDownloadCover ? (
            <DropdownMenuItem onSelect={onDownloadCover} disabled={saving}>
              <DownloadIcon />
              <Trans>Download cover</Trans>
            </DropdownMenuItem>
          ) : null}
          <DropdownMenuSeparator />
          <DropdownMenuItem
            variant="destructive"
            disabled={!contentWritable || saving}
            onSelect={() => setDeleteOpen(true)}
          >
            <Trash2Icon />
            <Trans>Delete</Trans>
          </DropdownMenuItem>
          <DropdownMenuSeparator />
          <DropdownMenuLabel className="text-xs font-normal break-all text-muted-foreground">
            {contentWritable
              ? entity.path
              : t`${CONTENT_WRITES_DISABLED} Editing actions are unavailable.`}
          </DropdownMenuLabel>
        </DropdownMenuContent>
      </DropdownMenu>

      <AlertDialog open={deleteOpen} onOpenChange={setDeleteOpen}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>
              <Trans>Move to trash?</Trans>
            </AlertDialogTitle>
            <AlertDialogDescription>
              <Trans>
                This moves {entityTitle(entity, language)} to the Trash. You can restore it later if
                you need it.
              </Trans>
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={saving}>
              <Trans>Cancel</Trans>
            </AlertDialogCancel>
            <AlertDialogAction onClick={onDelete} disabled={saving}>
              <Trans>Move to Trash</Trans>
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
  contentWritable,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  entityId: string;
  contentWritable: boolean;
}) {
  const { t } = useLingui();
  const invalidateLists = useInvalidateLists();
  // Membership-annotated list of every list (each carries `contains`).
  const lists = useQuery({ ...entityListsQuery(entityId), enabled: open });
  const [pendingId, setPendingId] = useState<string>();
  const [newName, setNewName] = useState("");
  const [creating, setCreating] = useState(false);

  useEffect(() => {
    if (open) setNewName("");
  }, [open]);

  const newNameError = newName.trim()
    ? basenameValidationError(normalizeBasename(newName))
    : undefined;
  // Smart lists derive membership from their filters, so they can't be joined/left by hand.
  const items = (lists.data?.items ?? []).filter((list) => list.kind === "static");

  async function toggle(listId: string, contains: boolean) {
    setPendingId(listId);
    try {
      if (contains) await removeItemFromList(listId, entityId);
      else await addItemToList(listId, { entityId });
      await invalidateLists(listId);
    } catch (error) {
      toast.error(errorMessage(error));
    } finally {
      setPendingId(undefined);
    }
  }

  async function createAndAdd() {
    const name = normalizeBasename(newName);
    if (!name.trim() || newNameError) return;
    setCreating(true);
    try {
      const created = await addList({ name });
      setNewName("");
      await addItemToList(created.id, { entityId });
      await invalidateLists(created.id);
      toast.success(t`List created`);
    } catch (error) {
      toast.error(errorMessage(error));
    } finally {
      setCreating(false);
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent aria-describedby={undefined}>
        <DialogHeader>
          <DialogTitle>
            <Trans>Manage lists</Trans>
          </DialogTitle>
        </DialogHeader>
        <div className="flex min-h-0 flex-col gap-1 overflow-auto max-sm:flex-1 sm:max-h-72">
          {lists.isPending ? (
            <p className="p-3 text-center text-sm text-muted-foreground">
              <Trans>Loading…</Trans>
            </p>
          ) : items.length === 0 ? (
            <p className="p-3 text-center text-sm text-muted-foreground">
              <Trans>No lists yet. Create one below.</Trans>
            </p>
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
                      <Plural value={list.itemCount} one="# item" other="# items" />
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
          <label className="text-sm font-medium">
            <Trans>New list</Trans>
          </label>
          <div className="flex gap-2">
            <Input
              value={newName}
              onChange={(event) => setNewName(event.target.value)}
              placeholder={t`Watchlist`}
              disabled={!contentWritable || creating}
              aria-invalid={Boolean(newNameError)}
            />
            <Button
              type="submit"
              disabled={!contentWritable || creating || !newName.trim() || Boolean(newNameError)}
            >
              <PlusIcon data-icon="inline-start" />
              {creating ? <Trans>Creating…</Trans> : <Trans>Create & add</Trans>}
            </Button>
          </div>
          {newNameError ? <p className="text-xs text-destructive">{newNameError}</p> : null}
        </form>
        <DialogFooter>
          <Button type="button" variant="outline" onClick={() => onOpenChange(false)}>
            <XIcon data-icon="inline-start" />
            <Trans>Done</Trans>
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
