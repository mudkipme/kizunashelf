import { Trans, useLingui } from "@lingui/react/macro";
import { useQuery } from "@tanstack/react-query";
import { CheckIcon } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { useBlocker, useNavigate, useSearchParams } from "react-router-dom";

import { errorMessage } from "@/api/client";
import { addEntity, saveEntity } from "@/api/entities";
import { useInvalidateEntityData } from "@/api/invalidate-entity-data";
import { configQuery } from "@/api/queries";
import { useRelationSearch } from "@/api/use-relation-search";
import { DetailSection } from "@/components/assets/detail-section";
import { FormDisclosure } from "@/components/entities/form-disclosure";
import {
  type FrontmatterDraft,
  MetadataEditor,
  normalizeFrontmatter,
} from "@/components/entities/metadata-editor";
import { AppFrame } from "@/components/layout/app-frame";
import { CONTENT_MEASURE } from "@/components/layout/page-container";
import { SaveFailure } from "@/components/save-failure";
import { Alert } from "@/components/ui/alert";
import {
  AlertDialog,
  AlertDialogContent,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogCancel,
} from "@/components/ui/alert-dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import { useCreateImages } from "@/hooks/use-create-images";
import { useUnsavedChangesWarning } from "@/hooks/use-unsaved-changes-warning";
import { normalizeBasename } from "@/lib/basename";
import { CONTENT_WRITES_DISABLED, useCapabilities } from "@/lib/capabilities";
import { filenameTitleField, resolveCreateBasename } from "@/lib/entity-create-form";
import type { Entity } from "@/types/api";

export function EntityCreatePage() {
  const { t } = useLingui();
  const navigate = useNavigate();
  const [searchParams] = useSearchParams();
  const requestedType = searchParams.get("type");
  const requestedTitle = searchParams.get("title");
  const invalidateEntityData = useInvalidateEntityData();
  const config = useQuery(configQuery());
  const capabilities = useCapabilities();
  const [creating, setCreating] = useState(false);
  const createdEntity = useRef<Entity | undefined>(undefined);
  const [createdId, setCreatedId] = useState<string>();
  const saved = useRef(false);
  const [fileOptionsOpen, setFileOptionsOpen] = useState(false);
  const images = useCreateImages();
  const [saveError, setSaveError] = useState<unknown>();
  const [typeId, setTypeId] = useState("");
  const [basename, setBasename] = useState(requestedTitle ?? "");
  const [frontmatter, setFrontmatter] = useState<FrontmatterDraft>({});
  const [body, setBody] = useState("");
  const contentWritable = capabilities.contentWritable;
  const queryError = config.error ?? capabilities.error;

  useEffect(() => {
    if (typeId) return;
    const types = config.data?.types;
    if (!types?.length) return;
    const preferred = types.find((type) => type.id === requestedType)?.id;
    setTypeId(preferred ?? types[0].id);
  }, [config.data, typeId, requestedType]);

  const selectedType = useMemo(
    () => config.data?.types.find((type) => type.id === typeId),
    [config.data, typeId],
  );

  // The filename an empty input falls back to comes from the schema's filename
  // title field, mirroring the core's quick-add derivation. With neither a
  // filename nor a title, Create is disabled (with the explanation below)
  // instead of silently doing nothing.
  const filename = useMemo(
    () =>
      resolveCreateBasename({ typeConfig: selectedType, frontmatter, manualBasename: basename }),
    [selectedType, frontmatter, basename],
  );

  // Warn on tab close/reload once anything has been entered.
  const dirty =
    basename !== (requestedTitle ?? "") || Object.keys(frontmatter).length > 0 || body !== "";
  useUnsavedChangesWarning(dirty && !saved.current);
  const blocker = useBlocker(
    ({ currentLocation, nextLocation }) =>
      dirty &&
      !saved.current &&
      (currentLocation.pathname !== nextLocation.pathname ||
        currentLocation.search !== nextLocation.search),
  );
  const blockerRef = useRef(blocker);
  blockerRef.current = blocker;

  const searchRelations = useRelationSearch();

  async function create() {
    if (creating || !contentWritable || !selectedType || !filename.canCreate) return;
    setCreating(true);
    setSaveError(undefined);
    try {
      let entity = createdEntity.current;
      const finishing = Boolean(entity);
      let draft = frontmatter;
      if (!entity) {
        const result = await addEntity({
          type: typeId,
          basename: filename.basename,
          frontmatterDraft: images.withoutPending(draft),
          body,
        });
        entity = result.entity;
        createdEntity.current = entity;
        setCreatedId(entity.id);
        setBasename(filename.basename);
        // Keep server-generated fields (such as IDs) alongside the local images.
        draft = { ...normalizeFrontmatter(entity.frontmatter), ...draft };
        setFrontmatter(draft);
      }
      if (finishing || images.hasPending(draft)) {
        const resolved = await images.uploadImages(entity.id, draft);
        const result = await saveEntity(entity.id, {
          revision: entity.revision,
          frontmatterDraft: resolved,
          body,
        });
        createdEntity.current = result.entity;
      }
      await invalidateEntityData();
      saved.current = true;
      if (blockerRef.current.state === "blocked") blockerRef.current.proceed();
      else navigate(`/entities/${encodeURIComponent(entity.id)}`);
    } catch (error) {
      setSaveError(error);
    } finally {
      setCreating(false);
    }
  }

  return (
    <AppFrame error={queryError ? errorMessage(queryError) : undefined}>
      {/* Same shell as the edit page — the toolbar carries the one verb, the
          column below it carries the form. */}
      <div className="flex h-full min-h-0 flex-col overflow-hidden">
        <div className="flex min-h-(--toolbar-height) shrink-0 flex-wrap items-center gap-2 border-b px-3 py-1.5">
          <span className="min-w-0 flex-1 truncate text-xs text-muted-foreground">
            {selectedType?.path}
          </span>
          <Button
            type="button"
            size="sm"
            onClick={create}
            disabled={creating || !contentWritable || !selectedType || !filename.canCreate}
          >
            <CheckIcon data-icon="inline-start" />
            {creating ? (
              <Trans>Saving…</Trans>
            ) : createdId ? (
              <Trans>Finish saving</Trans>
            ) : (
              <Trans>Create</Trans>
            )}
          </Button>
        </div>

        <div className="min-h-0 flex-1 overflow-auto overscroll-contain px-4 py-4">
          <div className={CONTENT_MEASURE}>
            {saveError ? (
              <div className="mb-4">
                {createdId ? (
                  <p className="mb-2 text-sm text-muted-foreground">
                    <Trans>
                      The entry was created. Your selected images and edits are kept here. Retry to
                      finish saving.
                    </Trans>
                  </p>
                ) : null}
                <SaveFailure error={saveError} />
                {createdId ? (
                  <Button
                    type="button"
                    variant="link"
                    onClick={() => navigate(`/entities/${encodeURIComponent(createdId)}/edit`)}
                  >
                    <Trans>Edit saved entry</Trans>
                  </Button>
                ) : null}
              </div>
            ) : null}
            <h1 className="mb-8 text-2xl leading-tight font-semibold tracking-tight">
              <Trans>Add manually</Trans>
            </h1>

            {!contentWritable ? <Alert className="mb-6">{CONTENT_WRITES_DISABLED}</Alert> : null}

            <DetailSection title={t`File`}>
              <div className="grid gap-x-6 gap-y-4 md:grid-cols-[220px_minmax(0,1fr)]">
                <label className="flex flex-col gap-1.5 text-sm font-medium">
                  <span className="text-xs font-medium text-muted-foreground">
                    <Trans>Type</Trans>
                  </span>
                  <Select
                    aria-label={t`Type`}
                    value={typeId}
                    onChange={(event) => setTypeId(event.target.value)}
                    disabled={
                      !contentWritable ||
                      creating ||
                      Boolean(createdId) ||
                      images.hasPending(frontmatter)
                    }
                  >
                    {config.data?.types.map((type) => (
                      <option key={type.id} value={type.id}>
                        {type.label}
                      </option>
                    ))}
                  </Select>
                  {images.hasPending(frontmatter) && !createdId ? (
                    <span className="text-xs font-normal text-muted-foreground">
                      <Trans>Remove selected images before changing the type.</Trans>
                    </span>
                  ) : null}
                </label>
                <FormDisclosure
                  title={t`File name`}
                  open={
                    fileOptionsOpen || Boolean(filename.error) || !filenameTitleField(selectedType)
                  }
                  onOpenChange={setFileOptionsOpen}
                >
                  <label className="flex flex-col gap-1.5 text-sm font-medium">
                    <span className="text-xs font-medium text-muted-foreground">
                      <Trans>File name</Trans>
                    </span>
                    <Input
                      aria-label={t`File name`}
                      value={basename}
                      onChange={(event) => setBasename(event.target.value)}
                      onBlur={() => setBasename(normalizeBasename(basename))}
                      placeholder={filename.derived ?? t`Title`}
                      disabled={!contentWritable || creating || Boolean(createdId)}
                      aria-invalid={Boolean(filename.error)}
                    />
                    {filename.error ? (
                      <span className="text-xs text-destructive">{filename.error}</span>
                    ) : filename.source === "title" ? (
                      <span className="text-xs font-normal text-muted-foreground">
                        <Trans comment="Hint below the empty file-name field on the entity create page; the placeholder is the file name derived from the entity's title field">
                          Will be created as “{filename.basename}.md”, from the title.
                        </Trans>
                      </span>
                    ) : filename.source === "none" ? (
                      <span className="text-xs font-normal text-muted-foreground">
                        {filenameTitleField(selectedType) ? (
                          <Trans comment="Hint below the empty file-name field on the entity create page; the Create button stays disabled until a file name or a title is entered">
                            Required — enter a file name here or fill in the title below.
                          </Trans>
                        ) : (
                          <Trans comment="Hint below the empty file-name field on the entity create page for a type with no title field; the Create button stays disabled until a file name is entered">
                            Required — enter a file name.
                          </Trans>
                        )}
                      </span>
                    ) : null}
                  </label>
                </FormDisclosure>
              </div>
            </DetailSection>

            <MetadataEditor
              key={typeId}
              typeConfig={selectedType}
              onPickImage={images.stageImage}
              frontmatter={frontmatter}
              bodyText={body}
              disabled={!contentWritable || creating}
              relationSuggestions={[]}
              onRelationSearch={searchRelations}
              onFrontmatterChange={setFrontmatter}
              onBodyChange={setBody}
            />
          </div>
        </div>
      </div>
      <AlertDialog
        open={blocker.state === "blocked"}
        onOpenChange={(open) => {
          if (!open && !creating && blocker.state === "blocked") blocker.reset();
        }}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>
              <Trans>Unsaved changes</Trans>
            </AlertDialogTitle>
            <AlertDialogDescription>
              <Trans>Save your edits before leaving, or discard them.</Trans>
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={creating}>
              <Trans>Keep editing</Trans>
            </AlertDialogCancel>
            <Button
              type="button"
              variant="outline"
              disabled={creating}
              onClick={() => {
                if (blocker.state === "blocked") blocker.proceed();
              }}
            >
              <Trans>Discard</Trans>
            </Button>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </AppFrame>
  );
}
