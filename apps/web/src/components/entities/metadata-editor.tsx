import { useEffect, useMemo, useState } from "react";
import { Trans, useLingui } from "@lingui/react/macro";
import { useQuery } from "@tanstack/react-query";
import { CheckIcon, PlusIcon, Trash2Icon, XIcon } from "lucide-react";

import { allTagsQuery, configQuery } from "@/api/queries";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { useTitleLanguage } from "@/lib/language";
import type { EntitySummary, TypeConfig } from "@/types/api";

import { FieldValueInput } from "./metadata-inputs";
import { editableFieldSpecs } from "./metadata-field-specs";
import type {
  EditableFieldSpec,
  FrontmatterDraft,
  FrontmatterValue,
  RelationSuggestionSearch,
} from "./metadata-types";

export type {
  FrontmatterDraft,
  FrontmatterObject,
  FrontmatterValue,
  RelationSuggestionSearch,
} from "./metadata-types";
export { normalizeFrontmatter } from "./frontmatter-utils";
export { NumberStepper } from "./metadata-scalar-inputs";

export function MetadataEditor({
  title,
  path,
  entityId,
  typeConfig,
  frontmatter,
  bodyText,
  saving,
  disabled = false,
  saveDisabled = false,
  relationSuggestions = [],
  onRelationSearch,
  saveLabel,
  onFrontmatterChange,
  onBodyChange,
  onSave,
  onCancel,
}: {
  title: string;
  path?: string;
  /** The entity's id, present only when editing an existing entity. Enables the
   * image-field upload control (uploads place assets under the entity's dir). */
  entityId?: string;
  typeConfig?: TypeConfig;
  frontmatter: FrontmatterDraft;
  bodyText: string;
  saving: boolean;
  disabled?: boolean;
  /** Disables only the save/create button (not the fields), e.g. while the
   * form is missing something required. */
  saveDisabled?: boolean;
  relationSuggestions?: EntitySummary[];
  onRelationSearch?: RelationSuggestionSearch;
  saveLabel?: string;
  onFrontmatterChange: (value: FrontmatterDraft) => void;
  onBodyChange: (value: string) => void;
  onSave: () => void;
  onCancel?: () => void;
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
    [typeConfig, frontmatter, relationSuggestions, onRelationSearch, language, allTags, tagsFieldName, t],
  );

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
    <section className="rounded-md border p-4">
      <div className="mb-3 flex items-center justify-between gap-2">
        <div className="min-w-0">
          <h2 className="truncate text-sm font-semibold">{title}</h2>
          {path ? <p className="mt-1 truncate text-xs text-muted-foreground">{path}</p> : null}
        </div>
        <div className="flex items-center gap-2">
          {onCancel ? (
            <Button type="button" variant="outline" size="sm" onClick={onCancel} disabled={saving}>
              <XIcon data-icon="inline-start" />
              <Trans>Cancel</Trans>
            </Button>
          ) : null}
          <Button type="button" size="sm" onClick={onSave} disabled={saving || disabled || saveDisabled}>
            <CheckIcon data-icon="inline-start" />
            {saving ? <Trans>Saving…</Trans> : (saveLabel ?? <Trans>Save</Trans>)}
          </Button>
        </div>
      </div>

      <div className="grid gap-3 md:grid-cols-2">
        {fieldSpecs.map((field) => (
          <EditableFieldRow
            key={field.key}
            field={field}
            value={frontmatter[field.key]}
            disabled={disabled}
            entityId={entityId}
            onChange={(value) => updateField(field.key, value)}
            onRemove={field.configured ? undefined : () => updateField(field.key, undefined)}
            onRename={field.configured ? undefined : (key) => renameField(field.key, key)}
          />
        ))}
      </div>

      <div className="mt-3 flex flex-wrap items-end gap-2 rounded-md border border-dashed p-3">
        <label className="min-w-48 flex-1 text-sm font-medium">
          <Trans>Custom field</Trans>
          <Input
            value={newFieldName}
            onChange={(event) => setNewFieldName(event.target.value)}
            placeholder="field_name"
            disabled={disabled}
          />
        </label>
        <Button type="button" variant="outline" onClick={addCustomField} disabled={disabled || !newFieldName.trim()}>
          <PlusIcon data-icon="inline-start" />
          <Trans>Add Field</Trans>
        </Button>
      </div>

      <div className="mt-4">
        <label className="flex flex-col gap-1 text-sm font-medium">
          <Trans>Notes</Trans>
          <Textarea
            className="min-h-72 font-mono text-xs"
            value={bodyText}
            onChange={(event) => onBodyChange(event.target.value)}
            disabled={disabled}
            spellCheck={false}
          />
        </label>
      </div>
    </section>
  );
}

function EditableFieldRow({
  field,
  value,
  disabled,
  entityId,
  onChange,
  onRemove,
  onRename,
}: {
  field: EditableFieldSpec;
  value: FrontmatterValue | undefined;
  disabled: boolean;
  entityId?: string;
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
    <div className="min-w-0 rounded-md border p-3">
      <div className="mb-2 flex min-w-0 items-center justify-between gap-2">
        {onRename ? (
          <Input
            value={keyDraft}
            onChange={(event) => setKeyDraft(event.target.value)}
            onBlur={() => onRename(keyDraft)}
            className="h-8 min-w-0 font-mono text-xs"
            aria-label={t`Custom field name`}
            disabled={disabled}
          />
        ) : (
          <div className="min-w-0">
            <div className="truncate text-sm font-medium">{field.label}</div>
          </div>
        )}
        {onRemove ? (
          <Button type="button" variant="ghost" size="icon" onClick={onRemove} aria-label={t`Remove ${field.key}`} disabled={disabled}>
            <Trash2Icon />
          </Button>
        ) : null}
      </div>
      <FieldValueInput
        field={field}
        value={value}
        disabled={disabled}
        entityId={entityId}
        onChange={onChange}
      />
    </div>
  );
}
