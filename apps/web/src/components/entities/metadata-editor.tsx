import { useEffect, useMemo, useState } from "react";
import { CheckIcon, PlusIcon, Trash2Icon, XIcon } from "lucide-react";

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
export { frontmatterPatch, normalizeFrontmatter } from "./frontmatter-utils";
export { NumberStepper } from "./metadata-inputs";

export function MetadataEditor({
  title,
  path,
  typeConfig,
  frontmatter,
  bodyText,
  saving,
  disabled = false,
  relationSuggestions = [],
  onRelationSearch,
  saveLabel = "Save",
  onFrontmatterChange,
  onBodyChange,
  onSave,
  onCancel,
}: {
  title: string;
  path?: string;
  typeConfig?: TypeConfig;
  frontmatter: FrontmatterDraft;
  bodyText: string;
  saving: boolean;
  disabled?: boolean;
  relationSuggestions?: EntitySummary[];
  onRelationSearch?: RelationSuggestionSearch;
  saveLabel?: string;
  onFrontmatterChange: (value: FrontmatterDraft) => void;
  onBodyChange: (value: string) => void;
  onSave: () => void;
  onCancel?: () => void;
}) {
  const [newFieldName, setNewFieldName] = useState("");
  const language = useTitleLanguage();
  const fieldSpecs = useMemo(
    () => editableFieldSpecs(typeConfig, frontmatter, relationSuggestions, onRelationSearch, language),
    [typeConfig, frontmatter, relationSuggestions, onRelationSearch, language],
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
              Cancel
            </Button>
          ) : null}
          <Button type="button" size="sm" onClick={onSave} disabled={saving || disabled}>
            <CheckIcon data-icon="inline-start" />
            {saving ? "Saving" : saveLabel}
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
            onChange={(value) => updateField(field.key, value)}
            onRemove={field.configured ? undefined : () => updateField(field.key, undefined)}
            onRename={field.configured ? undefined : (key) => renameField(field.key, key)}
          />
        ))}
      </div>

      <div className="mt-3 flex flex-wrap items-end gap-2 rounded-md border border-dashed p-3">
        <label className="min-w-48 flex-1 text-sm font-medium">
          Custom field
          <Input
            value={newFieldName}
            onChange={(event) => setNewFieldName(event.target.value)}
            placeholder="field_name"
            disabled={disabled}
          />
        </label>
        <Button type="button" variant="outline" onClick={addCustomField} disabled={disabled || !newFieldName.trim()}>
          <PlusIcon data-icon="inline-start" />
          Add Field
        </Button>
      </div>

      <div className="mt-4">
        <label className="flex flex-col gap-1 text-sm font-medium">
          Notes
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
  onChange,
  onRemove,
  onRename,
}: {
  field: EditableFieldSpec;
  value: FrontmatterValue | undefined;
  disabled: boolean;
  onChange: (value: FrontmatterValue) => void;
  onRemove?: () => void;
  onRename?: (key: string) => void;
}) {
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
            aria-label="Custom field name"
            disabled={disabled}
          />
        ) : (
          <div className="min-w-0">
            <div className="truncate text-sm font-medium">{field.label}</div>
          </div>
        )}
        {onRemove ? (
          <Button type="button" variant="ghost" size="icon" onClick={onRemove} aria-label={`Remove ${field.key}`} disabled={disabled}>
            <Trash2Icon />
          </Button>
        ) : null}
      </div>
      <FieldValueInput
        field={field}
        value={value}
        disabled={disabled}
        onChange={onChange}
      />
    </div>
  );
}
