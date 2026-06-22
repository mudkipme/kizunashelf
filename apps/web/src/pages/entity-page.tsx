import { useEffect, useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import {
  CheckIcon,
  DownloadIcon,
  FilePenLineIcon,
  MoreHorizontalIcon,
  PencilIcon,
  SearchIcon,
  Trash2Icon,
  XIcon,
} from "lucide-react";
import { useNavigate, useParams } from "react-router-dom";

import { errorMessage, isConflictError } from "@/api/client";
import { downloadAssets, removeEntity, saveEntity } from "@/api/entities";
import { useInvalidateEntityData } from "@/api/invalidate-entity-data";
import {
  capabilitiesQuery,
  configQuery,
  entityDatesQuery,
  entityQuery,
  providerCatalogQuery,
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
import { groupRelations } from "@/lib/relations";
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
  const [error, setError] = useState<string>();
  const [renameOpen, setRenameOpen] = useState(false);
  const [renameBasename, setRenameBasename] = useState("");
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
    defaultQuery: entity?.title,
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
    setMatchQuery(entity.title);
  }, [entity, setMatchQuery]);

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
                  onMatch={() => external.setOpen(true)}
                  onDownloadCover={downloadCover}
                  onDelete={deleteCurrentEntity}
                />
              }
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
  onMatch: () => void;
  onDownloadCover: () => void;
  onDelete: () => void;
}) {
  const [deleteOpen, setDeleteOpen] = useState(false);

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
              This moves {entity.title} to the vault's <code>.trash</code> folder. You can restore it from there if needed.
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
