import { Trans, useLingui } from "@lingui/react/macro";
import { useQuery } from "@tanstack/react-query";
import { CheckIcon, XIcon } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";

import { errorMessage } from "@/api/client";
import { saveEntity } from "@/api/entities";
import { useInvalidateEntityData } from "@/api/invalidate-entity-data";
import { configQuery, entityQuery } from "@/api/queries";
import { useRelationSearch } from "@/api/use-relation-search";
import {
  type FrontmatterDraft,
  MetadataEditor,
  normalizeFrontmatter,
} from "@/components/entities/metadata-editor";
import { AppFrame } from "@/components/layout/app-frame";
import { CONTENT_MEASURE } from "@/components/layout/page-container";
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
import { ENTITY_EDIT_CONFLICT_MESSAGE, useEntityMutation } from "@/hooks/use-entity-mutation";
import { useUnsavedChangesWarning } from "@/hooks/use-unsaved-changes-warning";
import { CONTENT_WRITES_DISABLED, useCapabilities } from "@/lib/capabilities";
import { useTitleLanguage } from "@/lib/language";
import { entityTitle } from "@/lib/title-language";

export function EntityEditPage() {
  const { t } = useLingui();
  const { id } = useParams();
  const navigate = useNavigate();
  const invalidateEntityData = useInvalidateEntityData();
  const detail = useQuery({ ...entityQuery(id ?? ""), enabled: Boolean(id) });
  const config = useQuery(configQuery());
  const capabilities = useCapabilities();
  const { saving, run } = useEntityMutation();
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

  // Reloads the latest server version, replacing the local draft. Used to
  // recover from a 409 conflict after the entity changed on disk.
  async function reloadLatest() {
    const refreshed = await detail.refetch();
    const fresh = refreshed.data?.entity;
    if (!fresh) return;
    seedDraft(fresh);
    setConflict(false);
  }

  const searchRelations = useRelationSearch();

  async function save() {
    if (!entity || !contentWritable) return;
    setConflict(false);
    await run(
      async () => {
        // The draft goes to the core verbatim; it serializes against the schema
        // and deletes keys the draft no longer carries (cleared fields).
        const result = await saveEntity(entity.id, {
          revision: seededRevisionRef.current ?? entity.revision,
          frontmatterDraft: frontmatter,
          body,
        });
        await invalidateEntityData();
        navigate(`/entities/${encodeURIComponent(result.entity.id)}`);
      },
      {
        // A 409 means the file changed on disk since it was loaded. Keep the
        // user's edits and offer a reload so they can reapply them — via the
        // inline banner below, so the conflict toast is suppressed here.
        conflictMessage: ENTITY_EDIT_CONFLICT_MESSAGE,
        onConflict: () => setConflict(true),
        silentConflict: true,
      },
    );
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
              disabled={saving || !contentWritable || !entity}
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
                <span className="min-w-0">{ENTITY_EDIT_CONFLICT_MESSAGE}</span>
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  onClick={() => void reloadLatest()}
                  disabled={saving}
                >
                  <Trans>Reload latest version</Trans>
                </Button>
              </Alert>
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
                disabled={!contentWritable}
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
