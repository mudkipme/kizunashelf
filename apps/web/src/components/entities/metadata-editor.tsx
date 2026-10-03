import { Trans, useLingui } from "@lingui/react/macro";
import { useQuery } from "@tanstack/react-query";
import { PlusIcon, Trash2Icon } from "lucide-react";
import { useEffect, useMemo, useState } from "react";

import { allTagsQuery, configQuery } from "@/api/queries";
import { DetailSection } from "@/components/assets/detail-section";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { useTitleLanguage } from "@/lib/language";
import type { EntitySummary, TypeConfig } from "@/types/api";

import { FormDisclosure } from "./form-disclosure";
import { editableFieldSpecs } from "./metadata-field-specs";
import { FieldValueInput } from "./metadata-inputs";
import type {
  EditableFieldSpec,
  FrontmatterDraft,
  FrontmatterValue,
  RelationSuggestionSearch,
  PickImage,
} from "./metadata-types";

export type {
  FrontmatterDraft,
  FrontmatterObject,
  FrontmatterValue,
  RelationSuggestionSearch,
} from "./metadata-types";
export { normalizeFrontmatter } from "./frontmatter-utils";
export { NumberStepper } from "./metadata-scalar-inputs";

/**
 * The frontmatter form: the schema's fields, any hand-added ones, and the body.
 *
 * It is only the form — no card around it, no header, no save button. The page
 * that hosts it owns the toolbar those belong in, which is what keeps the edit
 * and create pages reading like the detail page rather than like a document
 * with a form pasted into it. Each field is a label over its control; the
 * control's own border is the only one on the page, because a text field has to
 * look like a text field.
 */
export function MetadataEditor({
  entityId,
  onPickImage,
  typeConfig,
  frontmatter,
  bodyText,
  disabled = false,
  relationSuggestions = [],
  onRelationSearch,
  onFrontmatterChange,
  onBodyChange,
}: {
  /** The entity's id, present only when editing an existing entity. Enables the
   * image-field upload control (uploads place assets under the entity's dir). */
  entityId?: string;
  onPickImage?: PickImage;
  typeConfig?: TypeConfig;
  frontmatter: FrontmatterDraft;
  bodyText: string;
  disabled?: boolean;
  relationSuggestions?: EntitySummary[];
  onRelationSearch?: RelationSuggestionSearch;
  onFrontmatterChange: (value: FrontmatterDraft) => void;
  onBodyChange: (value: string) => void;
}) {
  const { t } = useLingui();
  const [newFieldName, setNewFieldName] = useState("");
  const language = useTitleLanguage();
  const allTagsData = useQuery(allTagsQuery()).data?.tags;
  const allTags = useMemo(() => allTagsData ?? [], [allTagsData]);
  const tagsFieldName = useQuery(configQuery()).data?.tagsField ?? undefined;
  const fieldSpecs = useMemo(
    () =>
      editableFieldSpecs(
        typeConfig,
        frontmatter,
        relationSuggestions,
        onRelationSearch,
        language,
        allTags,
        tagsFieldName,
        t`Tags`,
      ),
    [
      typeConfig,
      frontmatter,
      relationSuggestions,
      onRelationSearch,
      language,
      allTags,
      tagsFieldName,
      t,
    ],
  );

  const technicalKeys = new Set(
    (typeConfig?.fields ?? [])
      .filter((field) => field.fieldType === "id" || field.fieldType === "externalRef")
      .map((field) => field.field.trim()),
  );
  const isTechnical = (field: EditableFieldSpec) =>
    !field.configured || (field.key !== tagsFieldName && technicalKeys.has(field.key));
  function renderField(field: EditableFieldSpec) {
    return (
      <EditableFieldRow
        key={field.key}
        field={field}
        value={frontmatter[field.key]}
        disabled={disabled}
        entityId={entityId}
        onPickImage={onPickImage}
        onChange={(value) => updateField(field.key, value)}
        onRemove={field.configured ? undefined : () => updateField(field.key, undefined)}
        onRename={field.configured ? undefined : (key) => renameField(field.key, key)}
      />
    );
  }

  function updateField(key: string, value: FrontmatterValue | undefined) {
    const next = { ...frontmatter };
    if (value === undefined) delete next[key];
    else next[key] = value;
    onFrontmatterChange(next);
  }

  function renameField(oldKey: string, nextKey: string) {
    const key = nextKey.trim();
    if (!key || key === oldKey || key in frontmatter) return;
    const next = { ...frontmatter };
    next[key] = next[oldKey];
    delete next[oldKey];
    onFrontmatterChange(next);
  }

  function addCustomField() {
    const key = newFieldName.trim();
    if (!key || key in frontmatter) return;
    onFrontmatterChange({ ...frontmatter, [key]: "" });
    setNewFieldName("");
  }

  return (
    <>
      <DetailSection title={t`Metadata`}>
        <div className="grid gap-x-6 gap-y-4 md:grid-cols-2">
          {fieldSpecs.filter((field) => !isTechnical(field)).map(renderField)}
        </div>

        <FormDisclosure
          title={t({
            message: "IDs, sources & custom fields",
            comment:
              "Disclosure button in entity create/edit forms revealing schema IDs, external provider references, and custom metadata",
          })}
        >
          <div className="grid gap-x-6 gap-y-4 md:grid-cols-2">
            {fieldSpecs.filter(isTechnical).map(renderField)}
          </div>
          <div className="mt-2 flex flex-wrap items-end gap-2">
            <label className="flex min-w-48 flex-1 flex-col gap-1.5">
              <span className="text-xs font-medium text-muted-foreground">
                <Trans>Custom field</Trans>
              </span>
              <Input
                value={newFieldName}
                onChange={(event) => setNewFieldName(event.target.value)}
                placeholder="field_name"
                disabled={disabled}
              />
            </label>
            <Button
              type="button"
              variant="outline"
              onClick={addCustomField}
              disabled={disabled || !newFieldName.trim()}
            >
              <PlusIcon data-icon="inline-start" />
              <Trans>Add Field</Trans>
            </Button>
          </div>
        </FormDisclosure>
      </DetailSection>

      <DetailSection title={t`Notes`}>
        <Textarea
          className="min-h-72 font-mono text-code"
          value={bodyText}
          onChange={(event) => onBodyChange(event.target.value)}
          disabled={disabled}
          spellCheck={false}
          aria-label={t`Notes`}
        />
      </DetailSection>
    </>
  );
}

function EditableFieldRow({
  field,
  value,
  disabled,
  entityId,
  onPickImage,
  onChange,
  onRemove,
  onRename,
}: {
  field: EditableFieldSpec;
  value: FrontmatterValue | undefined;
  disabled: boolean;
  entityId?: string;
  onPickImage?: PickImage;
  onChange: (value: FrontmatterValue) => void;
  onRemove?: () => void;
  onRename?: (key: string) => void;
}) {
  const { t } = useLingui();
  const [keyDraft, setKeyDraft] = useState(field.key);

  useEffect(() => {
    setKeyDraft(field.key);
  }, [field.key]);

  return (
    <div className="flex min-w-0 flex-col gap-1.5">
      {/* The label row keeps a control's height whether or not it holds one, so
          a renameable field and a schema one line their inputs up. */}
      <div className="flex min-h-(--control-height-sm) min-w-0 items-center justify-between gap-2">
        {onRename ? (
          <Input
            value={keyDraft}
            onChange={(event) => setKeyDraft(event.target.value)}
            onBlur={() => onRename(keyDraft)}
            className="h-(--control-height-sm) min-w-0 font-mono text-code"
            aria-label={t`Custom field name`}
            disabled={disabled}
          />
        ) : (
          // The same label treatment the detail page gives a field, so the two
          // views of one entity read as the same page in two modes.
          <span className="min-w-0 truncate text-xs font-medium text-muted-foreground">
            {field.label}
          </span>
        )}
        {onRemove ? (
          <Button
            type="button"
            variant="ghost"
            size="icon-sm"
            onClick={onRemove}
            aria-label={t`Remove ${field.key}`}
            disabled={disabled}
          >
            <Trash2Icon />
          </Button>
        ) : null}
      </div>
      <FieldValueInput
        field={field}
        value={value}
        disabled={disabled}
        entityId={entityId}
        onPickImage={onPickImage}
        onChange={onChange}
      />
    </div>
  );
}
