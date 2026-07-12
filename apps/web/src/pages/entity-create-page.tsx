import { useEffect, useMemo, useState } from "react";
import { Trans, useLingui } from "@lingui/react/macro";
import { useQuery } from "@tanstack/react-query";
import { useNavigate, useSearchParams } from "react-router-dom";

import { errorMessage } from "@/api/client";
import { addEntity } from "@/api/entities";
import { useInvalidateEntityData } from "@/api/invalidate-entity-data";
import { useRelationSearch } from "@/api/use-relation-search";
import { configQuery } from "@/api/queries";
import { type FrontmatterDraft, MetadataEditor } from "@/components/entities/metadata-editor";
import { AppFrame } from "@/components/layout/app-frame";
import { PageContainer } from "@/components/layout/page-container";
import { Alert } from "@/components/ui/alert";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import { useEntityMutation } from "@/hooks/use-entity-mutation";
import { basenameValidationError, normalizeBasename } from "@/lib/basename";
import { CONTENT_WRITES_DISABLED, useCapabilities } from "@/lib/capabilities";

export function EntityCreatePage() {
  const { t } = useLingui();
  const navigate = useNavigate();
  const [searchParams] = useSearchParams();
  const requestedType = searchParams.get("type");
  const requestedTitle = searchParams.get("title");
  const invalidateEntityData = useInvalidateEntityData();
  const config = useQuery(configQuery());
  const capabilities = useCapabilities();
  const { saving: creating, run } = useEntityMutation();
  const [typeId, setTypeId] = useState("");
  const [basename, setBasename] = useState(requestedTitle ?? "");
  const [frontmatter, setFrontmatter] = useState<FrontmatterDraft>({});
  const [body, setBody] = useState("");
  const contentWritable = capabilities.contentWritable;
  const queryError = config.error ?? capabilities.error;
  const normalizedBasename = normalizeBasename(basename);
  const basenameError = basenameValidationError(basename);
  const showBasenameError = Boolean(basename) && Boolean(basenameError);

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

  const searchRelations = useRelationSearch();

  async function create() {
    if (!contentWritable) return;
    if (basenameError) {
      setBasename(normalizedBasename);
      return;
    }
    await run(async () => {
      const result = await addEntity({
        type: typeId,
        basename: normalizedBasename,
        frontmatter,
        body,
      });
      await invalidateEntityData();
      navigate(`/entities/${encodeURIComponent(result.entity.id)}`);
    });
  }

  return (
    <AppFrame error={queryError ? errorMessage(queryError) : undefined}>
      <PageContainer>
        <header className="flex flex-wrap items-start justify-between gap-3">
          <div className="min-w-0">
            <h1 className="truncate text-base font-semibold">
              <Trans>Add Entity</Trans>
            </h1>
            <p className="mt-1 truncate text-xs text-muted-foreground">
              {selectedType ? `${selectedType.label} · ${selectedType.path}` : t`Choose a type`}
            </p>
          </div>
        </header>

        {!contentWritable ? <Alert>{CONTENT_WRITES_DISABLED}</Alert> : null}

        <section className="rounded-md border p-4">
          <div className="grid gap-3 md:grid-cols-[220px_minmax(0,1fr)]">
            <label className="flex flex-col gap-1 text-sm font-medium">
              <Trans>Type</Trans>
              <Select value={typeId} onChange={(event) => setTypeId(event.target.value)} disabled={!contentWritable}>
                {config.data?.types.map((type) => (
                  <option key={type.id} value={type.id}>
                    {type.label}
                  </option>
                ))}
              </Select>
            </label>
            <label className="flex flex-col gap-1 text-sm font-medium">
              <Trans>File name</Trans>
              <Input
                value={basename}
                onChange={(event) => setBasename(event.target.value)}
                onBlur={() => setBasename(normalizeBasename(basename))}
                placeholder={t`Title`}
                disabled={!contentWritable}
                aria-invalid={showBasenameError}
              />
              {showBasenameError ? <span className="text-xs text-destructive">{basenameError}</span> : null}
            </label>
          </div>
        </section>

        <MetadataEditor
          title={t`Metadata`}
          path={selectedType?.path}
          typeConfig={selectedType}
          frontmatter={frontmatter}
          bodyText={body}
          saving={creating}
          disabled={!contentWritable}
          relationSuggestions={[]}
          onRelationSearch={searchRelations}
          saveLabel={t`Create`}
          onFrontmatterChange={setFrontmatter}
          onBodyChange={setBody}
          onSave={create}
        />
      </PageContainer>
    </AppFrame>
  );
}
