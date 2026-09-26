import { Trans, useLingui } from "@lingui/react/macro";
import { useQuery } from "@tanstack/react-query";
import { CheckIcon } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { useNavigate, useSearchParams } from "react-router-dom";

import { errorMessage } from "@/api/client";
import { addEntity } from "@/api/entities";
import { useInvalidateEntityData } from "@/api/invalidate-entity-data";
import { configQuery } from "@/api/queries";
import { useRelationSearch } from "@/api/use-relation-search";
import { DetailSection } from "@/components/assets/detail-section";
import { type FrontmatterDraft, MetadataEditor } from "@/components/entities/metadata-editor";
import { AppFrame } from "@/components/layout/app-frame";
import { CONTENT_MEASURE } from "@/components/layout/page-container";
import { SaveFailure } from "@/components/save-failure";
import { Alert } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import { useUnsavedChangesWarning } from "@/hooks/use-unsaved-changes-warning";
import { normalizeBasename } from "@/lib/basename";
import { CONTENT_WRITES_DISABLED, useCapabilities } from "@/lib/capabilities";
import { filenameTitleField, resolveCreateBasename } from "@/lib/entity-create-form";

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
  useUnsavedChangesWarning(dirty);

  const searchRelations = useRelationSearch();

  async function create() {
    if (creating || !contentWritable || !filename.canCreate) return;
    setCreating(true);
    setSaveError(undefined);
    try {
      const result = await addEntity({
        type: typeId,
        basename: filename.basename,
        frontmatter,
        body,
      });
      await invalidateEntityData();
      navigate(`/entities/${encodeURIComponent(result.entity.id)}`);
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
            disabled={creating || !contentWritable || !filename.canCreate}
          >
            <CheckIcon data-icon="inline-start" />
            {creating ? <Trans>Saving…</Trans> : <Trans>Create</Trans>}
          </Button>
        </div>

        <div className="min-h-0 flex-1 overflow-auto overscroll-contain px-4 py-4">
          <div className={CONTENT_MEASURE}>
            {saveError ? (
              <div className="mb-4">
                <SaveFailure error={saveError} revisionConflict={false} />
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
                    value={typeId}
                    onChange={(event) => setTypeId(event.target.value)}
                    disabled={!contentWritable}
                  >
                    {config.data?.types.map((type) => (
                      <option key={type.id} value={type.id}>
                        {type.label}
                      </option>
                    ))}
                  </Select>
                </label>
                <label className="flex flex-col gap-1.5 text-sm font-medium">
                  <span className="text-xs font-medium text-muted-foreground">
                    <Trans>File name</Trans>
                  </span>
                  <Input
                    value={basename}
                    onChange={(event) => setBasename(event.target.value)}
                    onBlur={() => setBasename(normalizeBasename(basename))}
                    placeholder={filename.derived ?? t`Title`}
                    disabled={!contentWritable}
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
              </div>
            </DetailSection>

            <MetadataEditor
              typeConfig={selectedType}
              frontmatter={frontmatter}
              bodyText={body}
              disabled={!contentWritable}
              relationSuggestions={[]}
              onRelationSearch={searchRelations}
              onFrontmatterChange={setFrontmatter}
              onBodyChange={setBody}
            />
          </div>
        </div>
      </div>
    </AppFrame>
  );
}
