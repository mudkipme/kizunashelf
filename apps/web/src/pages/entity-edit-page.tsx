import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Trans, useLingui } from "@lingui/react/macro";
import { useQuery } from "@tanstack/react-query";
import { ArrowLeftIcon } from "lucide-react";
import { useNavigate, useParams } from "react-router-dom";

import { errorMessage } from "@/api/client";
import { saveEntity } from "@/api/entities";
import { useInvalidateEntityData } from "@/api/invalidate-entity-data";
import { useRelationSearch } from "@/api/use-relation-search";
import { configQuery, entityQuery } from "@/api/queries";
import {
  type FrontmatterDraft,
  MetadataEditor,
  normalizeFrontmatter,
} from "@/components/entities/metadata-editor";
import { AppFrame } from "@/components/layout/app-frame";
import { PageContainer } from "@/components/layout/page-container";
import { Alert } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Placeholder } from "@/components/ui/placeholder";
import { ENTITY_EDIT_CONFLICT_MESSAGE, useEntityMutation } from "@/hooks/use-entity-mutation";
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
  const [frontmatter, setFrontmatter] = useState<FrontmatterDraft>({});
  const [body, setBody] = useState("");
  // Tracks which entity the local draft was seeded from, so a background refetch
  // of the same entity doesn't clobber in-progress edits.
  const seededEntityIdRef = useRef<string | undefined>(undefined);
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
    setFrontmatter(normalizeFrontmatter(source.frontmatter));
    setBody(source.body);
  }, []);

  useEffect(() => {
    if (!entity) return;
    // Seed only when this is a different entity than the one already loaded.
    // Refetches of the same entity (window focus, cache invalidation) keep the
    // user's edits instead of resetting the form underneath them.
    if (seededEntityIdRef.current === entity.id) return;
    seedDraft(entity);
  }, [entity, seedDraft]);

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
          revision: entity.revision,
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

  function cancel() {
    if (entity) navigate(`/entities/${encodeURIComponent(entity.id)}`);
    else navigate("/library");
  }

  return (
    <AppFrame error={queryError ? errorMessage(queryError) : undefined}>
      <PageContainer>
        <header className="flex flex-wrap items-start justify-between gap-3">
          <div className="min-w-0">
            <h1 className="truncate text-base font-semibold">
              {entity ? t`Edit ${entityTitle(entity, language)}` : t`Edit entity`}
            </h1>
          </div>
          <div className="flex flex-wrap gap-2">
            <Button
              type="button"
              variant="outline"
              onClick={cancel}
              disabled={saving}
            >
              <ArrowLeftIcon data-icon="inline-start" />
              <Trans>Back</Trans>
            </Button>
          </div>
        </header>

        {!contentWritable ? <Alert>{CONTENT_WRITES_DISABLED}</Alert> : null}

        {conflict ? (
          <Alert className="flex flex-wrap items-center justify-between gap-3">
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
            title={t`Metadata`}
            path={entity.path}
            entityId={entity.id}
            typeConfig={typeConfig}
            frontmatter={frontmatter}
            bodyText={body}
            saving={saving}
            disabled={!contentWritable}
            relationSuggestions={[]}
            onRelationSearch={searchRelations}
            onFrontmatterChange={setFrontmatter}
            onBodyChange={setBody}
            onSave={save}
            onCancel={cancel}
          />
        ) : (
          <Placeholder>
            <Trans>Entity not found</Trans>
          </Placeholder>
        )}
      </PageContainer>
    </AppFrame>
  );
}
