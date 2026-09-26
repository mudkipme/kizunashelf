import type { EntityEditReviewResponse, EntityEditDraft } from "@kizunashelf/api-contract";
import { Trans, useLingui } from "@lingui/react/macro";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { CheckIcon, XIcon } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";

import { errorMessage, isConflictError } from "@/api/client";
import { saveEntity, reviewEntityDraft } from "@/api/entities";
import { useInvalidateEntityData } from "@/api/invalidate-entity-data";
import { configQuery, entityQuery } from "@/api/queries";
import { useRelationSearch } from "@/api/use-relation-search";
import { EditConflictReview } from "@/components/entities/edit-conflict-review";
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
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Button } from "@/components/ui/button";
import { Placeholder } from "@/components/ui/placeholder";
import { useUnsavedChangesWarning } from "@/hooks/use-unsaved-changes-warning";
import { CONTENT_WRITES_DISABLED, useCapabilities } from "@/lib/capabilities";
import { useTitleLanguage } from "@/lib/language";
import { entityTitle } from "@/lib/title-language";
import { fieldLabelForKey } from "@/lib/type-config";

export function EntityEditPage() {
  const { t } = useLingui();
  const { id } = useParams();
  const navigate = useNavigate();
  const invalidateEntityData = useInvalidateEntityData();
  const detail = useQuery({ ...entityQuery(id ?? ""), enabled: Boolean(id) });
  const config = useQuery(configQuery());
  const capabilities = useCapabilities();
  const queryClient = useQueryClient();
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<unknown>();
  const [reviewing, setReviewing] = useState(false);
  const [review, setReview] = useState<EntityEditReviewResponse>();
  const baselineRef = useRef<NonNullable<typeof entity> | undefined>(undefined);
  const draftBasenameRef = useRef<string | undefined>(undefined);
  const schemaRevisionRef = useRef<string | undefined>(undefined);
  const [conflict, setConflict] = useState(false);
  const [confirmDiscard, setConfirmDiscard] = useState(false);
  const [frontmatter, setFrontmatter] = useState<FrontmatterDraft>({});
  const [body, setBody] = useState("");
  // Tracks which entity the local draft was seeded from, so a background refetch
  // of the same entity doesn't clobber in-progress edits.
  const seededEntityIdRef = useRef<string | undefined>(undefined);
  // The optimistic-concurrency revision belongs to the draft, not the latest
  // query response. If Obsidian changes the entity while this draft is dirty,
  // saving must submit the old revision and receive a 409 instead of silently
  // overwriting the external edit.
  const seededRevisionRef = useRef<string | undefined>(undefined);
  // Snapshot of the seeded draft, for dirty detection (Back/Cancel confirmation
  // and the tab-close warning).
  const seededSnapshotRef = useRef<{ frontmatter: string; body: string } | null>(null);
  const loading = detail.isPending || config.isPending || capabilities.isPending;
  const queryError = detail.error ?? config.error ?? capabilities.error;
  const entity = detail.data?.entity;
  const contentWritable = capabilities.contentWritable;
  const language = useTitleLanguage();
  const typeConfig = useMemo(
    () => config.data?.types.find((type) => type.id === entity?.type),
    [config.data, entity?.type],
  );
  const seedDraft = useCallback((source: NonNullable<typeof entity>) => {
    baselineRef.current = source;
    draftBasenameRef.current = source.basename;
    schemaRevisionRef.current = undefined;
    seededEntityIdRef.current = source.id;
    seededRevisionRef.current = source.revision;
    const seeded = normalizeFrontmatter(source.frontmatter);
    seededSnapshotRef.current = { frontmatter: JSON.stringify(seeded), body: source.body };
    setFrontmatter(seeded);
    setBody(source.body);
    setConflict(false);
  }, []);

  const snapshot = seededSnapshotRef.current;
  const dirty = Boolean(
    snapshot && (JSON.stringify(frontmatter) !== snapshot.frontmatter || body !== snapshot.body),
  );

  // Warn on tab close/reload while edits are unsaved. In-app leaving (Back/
  // Cancel) is confirmed via the discard dialog below.
  useUnsavedChangesWarning(dirty);

  useEffect(() => {
    if (!entity) return;
    if (seededEntityIdRef.current !== entity.id) {
      seedDraft(entity);
      return;
    }
    if (seededRevisionRef.current === entity.revision) return;
    // Clean forms adopt an external edit immediately. Dirty forms keep their
    // draft and surface the existing reload/conflict recovery instead.
    if (dirty) setConflict(true);
    else seedDraft(entity);
  }, [dirty, entity, seedDraft]);

  async function reviewLatest() {
    const baseline = baselineRef.current;
    if (!baseline || reviewing || saving) return;
    setReviewing(true);
    setSaveError(undefined);
    try {
      const result = await reviewEntityDraft(baseline.id, {
        type: baseline.type,
        baseline: {
          basename: baseline.basename,
          body: baseline.body,
          frontmatter: baseline.frontmatter,
        },
        draft: { basename: draftBasenameRef.current ?? baseline.basename, body, frontmatter },
      });
      setReview(result);
    } catch (error) {
      setSaveError(error);
    } finally {
      setReviewing(false);
    }
  }

  function applyReviewedDraft(draft: EntityEditDraft) {
    if (!review) return;
    queryClient.setQueryData(entityQuery(review.entity.id).queryKey, (current) =>
      current ? { ...current, entity: review.entity } : current,
    );
    seedDraft(review.entity);
    schemaRevisionRef.current = review.schemaRevision;
    draftBasenameRef.current = draft.basename;
    setFrontmatter(normalizeFrontmatter(draft.frontmatter));
    setBody(draft.body);
    setReview(undefined);
    setSaveError(undefined);
    void queryClient.invalidateQueries({ queryKey: configQuery().queryKey });
  }

  const searchRelations = useRelationSearch();

  async function save() {
    if (!entity || !contentWritable || saving || reviewing || conflict) return;
    setSaving(true);
    setSaveError(undefined);
    try {
      const result = await saveEntity(entity.id, {
        revision: seededRevisionRef.current ?? entity.revision,
        schemaRevision: schemaRevisionRef.current,
        ...(draftBasenameRef.current !== baselineRef.current?.basename
          ? { renameTo: draftBasenameRef.current }
          : {}),
        frontmatterDraft: frontmatter,
        body,
      });
      await invalidateEntityData();
      navigate(`/entities/${encodeURIComponent(result.entity.id)}`);
    } catch (error) {
      setSaveError(error);
      if (isConflictError(error)) setConflict(true);
    } finally {
      setSaving(false);
    }
  }

  function leave() {
    if (entity) navigate(`/entities/${encodeURIComponent(entity.id)}`);
    else navigate("/library");
  }

  // Leaving discards the draft, so confirm first when dirty.
  function cancel() {
    if (dirty) {
      setConfirmDiscard(true);
      return;
    }
    leave();
  }

  return (
    <AppFrame error={queryError ? errorMessage(queryError) : undefined}>
      {/* The same shape as the detail page: a toolbar that stays put, and one
          scrolling column beneath it. Cancel is the only way back — the old
          header carried a "Back" button that called the very same handler. */}
      <div className="flex h-full min-h-0 flex-col overflow-hidden">
        <div className="flex min-h-(--toolbar-height) shrink-0 flex-wrap items-center gap-2 border-b px-3 py-1.5">
          {/* What is being edited, kept on screen while the form scrolls. */}
          <span className="min-w-0 flex-1 truncate text-xs text-muted-foreground">
            {entity?.path}
          </span>
          <div className="flex items-center gap-2">
            <Button type="button" variant="outline" size="sm" onClick={cancel} disabled={saving}>
              <XIcon data-icon="inline-start" />
              <Trans>Cancel</Trans>
            </Button>
            <Button
              type="button"
              size="sm"
              onClick={save}
              disabled={saving || reviewing || conflict || !contentWritable || !entity}
            >
              <CheckIcon data-icon="inline-start" />
              {saving ? <Trans>Saving…</Trans> : <Trans>Save</Trans>}
            </Button>
          </div>
        </div>

        <div className="min-h-0 flex-1 overflow-auto overscroll-contain px-4 py-4">
          <div className={CONTENT_MEASURE}>
            <h1 className="mb-8 text-2xl leading-tight font-semibold tracking-tight">
              {entity ? t`Edit ${entityTitle(entity, language)}` : t`Edit entity`}
            </h1>

            {!contentWritable ? <Alert className="mb-6">{CONTENT_WRITES_DISABLED}</Alert> : null}

            {conflict ? (
              <Alert className="mb-6 flex flex-wrap items-center justify-between gap-3">
                <span className="min-w-0">
                  <Trans>
                    This changed elsewhere. Review the latest version to keep your edits.
                  </Trans>
                </span>
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  onClick={() => void reviewLatest()}
                  disabled={saving || reviewing}
                >
                  <Trans>Review changes</Trans>
                </Button>
              </Alert>
            ) : null}

            {saveError && !isConflictError(saveError) ? (
              <div className="mb-4">
                <SaveFailure error={saveError} />
              </div>
            ) : null}
            {loading ? (
              <Placeholder>
                <Trans>Loading…</Trans>
              </Placeholder>
            ) : entity ? (
              <MetadataEditor
                entityId={entity.id}
                typeConfig={typeConfig}
                frontmatter={frontmatter}
                bodyText={body}
                disabled={!contentWritable || saving || reviewing || Boolean(review)}
                relationSuggestions={[]}
                onRelationSearch={searchRelations}
                onFrontmatterChange={setFrontmatter}
                onBodyChange={setBody}
              />
            ) : (
              <Placeholder>
                <Trans>Entity not found</Trans>
              </Placeholder>
            )}
          </div>
        </div>
      </div>

      {review ? (
        <EditConflictReview
          key={review.entity.revision}
          review={review}
          label={(field) => fieldLabelForKey(typeConfig, field)}
          onAccept={applyReviewedDraft}
          onCancel={() => setReview(undefined)}
        />
      ) : null}
      <AlertDialog open={confirmDiscard} onOpenChange={setConfirmDiscard}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>
              <Trans>Discard unsaved changes?</Trans>
            </AlertDialogTitle>
            <AlertDialogDescription>
              <Trans comment="Description in the confirmation dialog shown when leaving the entity edit page with unsaved changes">
                You have unsaved edits to this entity. Leaving will discard them.
              </Trans>
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>
              <Trans>Keep editing</Trans>
            </AlertDialogCancel>
            <AlertDialogAction
              onClick={() => {
                setConfirmDiscard(false);
                leave();
              }}
            >
              <Trans>Discard</Trans>
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </AppFrame>
  );
}
