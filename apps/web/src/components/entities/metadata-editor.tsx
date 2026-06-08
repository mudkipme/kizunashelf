import type { KeyboardEvent } from "react";
import { useEffect, useMemo, useRef, useState } from "react";
import { CalendarIcon, CheckIcon, MinusIcon, PlusIcon, Trash2Icon, XIcon } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Calendar } from "@/components/ui/calendar";
import { Input } from "@/components/ui/input";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { Select } from "@/components/ui/select";
import { Textarea } from "@/components/ui/textarea";
import {
  configFields,
  configuredFieldLabel,
  isListFieldType,
  type FieldConfig,
} from "@/lib/type-config";
import type { EntitySummary, TypeConfig } from "@/types/api";

export type FrontmatterValue = null | boolean | number | string | FrontmatterValue[] | FrontmatterObject;
export type FrontmatterObject = { [key: string]: FrontmatterValue | undefined };
export type FrontmatterDraft = Record<string, FrontmatterValue>;

type FieldKind = "text" | "select" | "number" | "progress" | "boolean" | "list" | "relation" | "season" | "date" | "object";

type EditableFieldSpec = {
  key: string;
  label: string;
  kind: FieldKind;
  configured: boolean;
  options: string[];
  relationOptions: MultiValueOption[];
  loadRelationOptions?: (query: string, signal: AbortSignal) => Promise<MultiValueOption[]>;
  relationType?: string | null;
  seasonLanguage: SeasonLanguage;
};

type SeasonLanguage = "zh" | "ja" | "en";
type SeasonKey = "winter" | "spring" | "summer" | "autumn";
type SeasonRow = { kind: "season"; year: string; season: SeasonKey } | { kind: "raw"; value: string };
type MultiValueOption = { value: string; label?: string; detail?: string };
export type RelationSuggestionSearch = (params: {
  relationType?: string | null;
  query: string;
  signal: AbortSignal;
}) => Promise<EntitySummary[]>;

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
  const fieldSpecs = useMemo(
    () => editableFieldSpecs(typeConfig, frontmatter, relationSuggestions, onRelationSearch),
    [typeConfig, frontmatter, relationSuggestions, onRelationSearch],
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
          Markdown Body
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
            <div className="truncate font-mono text-[11px] text-muted-foreground">{field.key}</div>
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

function FieldValueInput({
  field,
  value,
  disabled,
  onChange,
}: {
  field: EditableFieldSpec;
  value: FrontmatterValue | undefined;
  disabled: boolean;
  onChange: (value: FrontmatterValue) => void;
}) {
  if (field.kind === "select") {
    const options = withCurrentOption(field.options, valueToText(value));
    return (
      <Select
        value={valueToText(value)}
        onChange={(event) => onChange(event.target.value || null)}
        className="w-full"
        aria-label={field.label}
        disabled={disabled}
      >
        <option value="">Empty</option>
        {options.map((option) => (
          <option key={option} value={option}>
            {option}
          </option>
        ))}
      </Select>
    );
  }

  if (field.kind === "number") {
    return (
      <NumberStepper
        value={valueToText(value)}
        onChange={(next) => onChange(numberOrString(next))}
        ariaLabel={field.label}
        disabled={disabled}
      />
    );
  }

  if (field.kind === "progress") {
    return (
      <ProgressStepper
        value={valueToText(value)}
        onChange={(next) => onChange(numberOrString(next))}
        ariaLabel={field.label}
        disabled={disabled}
      />
    );
  }

  if (field.kind === "date") {
    return (
      <DatePickerInput
        value={valueToText(value)}
        onChange={(next) => onChange(next || null)}
        ariaLabel={field.label}
        disabled={disabled}
      />
    );
  }

  if (field.kind === "boolean") {
    return (
      <Select
        value={value === true ? "true" : value === false ? "false" : ""}
        onChange={(event) =>
          onChange(event.target.value === "" ? null : event.target.value === "true")
        }
        className="w-full"
        aria-label={field.label}
        disabled={disabled}
      >
        <option value="">Empty</option>
        <option value="true">Yes</option>
        <option value="false">No</option>
      </Select>
    );
  }

  if (field.kind === "list" || field.kind === "relation") {
    const relation = field.kind === "relation";
    return (
      <MultiValueInput
        values={listDisplayValues(value, relation)}
        options={relation ? field.relationOptions : field.options.map((option) => ({ value: option }))}
        loadOptions={relation ? field.loadRelationOptions : undefined}
        placeholder={relation ? relationPlaceholder(field.relationType) : "Add value"}
        ariaLabel={field.label}
        wikilinks={relation}
        disabled={disabled}
        onChange={(values) => onChange(relation ? values.map(toWikilink) : values)}
      />
    );
  }

  if (field.kind === "season") {
    return (
      <SeasonListInput
        values={listDisplayValues(value, false)}
        language={field.seasonLanguage}
        ariaLabel={field.label}
        disabled={disabled}
        onChange={onChange}
      />
    );
  }

  if (field.kind === "object") {
    return (
      <ObjectValueInput
        value={value}
        onChange={onChange}
        disabled={disabled}
        ariaLabel={field.label}
      />
    );
  }

  return (
    <Input
      value={valueToText(value)}
      onChange={(event) => onChange(event.target.value || null)}
      list={field.options.length > 0 ? `${field.key}-options` : undefined}
      aria-label={field.label}
      disabled={disabled}
    />
  );
}

function MultiValueInput({
  values,
  options,
  loadOptions,
  placeholder,
  ariaLabel,
  wikilinks,
  disabled,
  onChange,
}: {
  values: string[];
  options: MultiValueOption[];
  loadOptions?: (query: string, signal: AbortSignal) => Promise<MultiValueOption[]>;
  placeholder: string;
  ariaLabel: string;
  wikilinks: boolean;
  disabled: boolean;
  onChange: (values: string[]) => void;
}) {
  const [inputValue, setInputValue] = useState("");
  const [open, setOpen] = useState(false);
  const [remoteOptions, setRemoteOptions] = useState<MultiValueOption[]>([]);
  const [loadingOptions, setLoadingOptions] = useState(false);
  const [optionsError, setOptionsError] = useState<string>();
  const containerRef = useRef<HTMLDivElement | null>(null);
  const selectedValues = uniqueStrings(values.map((value) => normalizeListItem(value, wikilinks)).filter(Boolean));
  const normalizedOptions = uniqueOptions([...options, ...remoteOptions], wikilinks);
  const customValue = normalizeListItem(inputValue, wikilinks);
  const query = customValue.toLowerCase();
  const suggestedItems = normalizedOptions
    .filter((option) => !selectedValues.includes(option.value))
    .filter(
      (option) =>
        loadOptions ||
        !query ||
        option.value.toLowerCase().includes(query) ||
        option.label?.toLowerCase().includes(query) ||
        option.detail?.toLowerCase().includes(query),
    );
  const optionValues = normalizedOptions.map((option) => option.value);
  const customItem =
    customValue && !selectedValues.includes(customValue) && !optionValues.includes(customValue)
      ? ({ value: customValue } satisfies MultiValueOption)
      : undefined;
  const items = (loadOptions ? [...suggestedItems, customItem] : [customItem, ...suggestedItems]).filter(
    (item): item is MultiValueOption => Boolean(item),
  );
  const labels = new Map(normalizedOptions.map((option) => [option.value, option.label || option.value]));

  useEffect(() => {
    if (!open || disabled || !loadOptions) return;
    const controller = new AbortController();
    const timer = window.setTimeout(() => {
      setLoadingOptions(true);
      setOptionsError(undefined);
      loadOptions(inputValue.trim(), controller.signal)
        .then((items) => {
          if (!controller.signal.aborted) setRemoteOptions(items);
        })
        .catch((error) => {
          if (isAbortError(error) || controller.signal.aborted) return;
          setRemoteOptions([]);
          setOptionsError(error instanceof Error ? error.message : "Could not load options.");
        })
        .finally(() => {
          if (!controller.signal.aborted) setLoadingOptions(false);
        });
    }, 200);
    return () => {
      controller.abort();
      window.clearTimeout(timer);
    };
  }, [disabled, inputValue, loadOptions, open]);

  useEffect(() => {
    if (!open || disabled) return;
    function closeOnOutsidePointer(event: PointerEvent) {
      if (containerRef.current?.contains(event.target as Node)) return;
      setOpen(false);
    }
    document.addEventListener("pointerdown", closeOnOutsidePointer);
    return () => document.removeEventListener("pointerdown", closeOnOutsidePointer);
  }, [open]);

  function updateValues(nextValues: string[]) {
    if (disabled) return;
    onChange(uniqueStrings(nextValues.map((value) => normalizeListItem(value, wikilinks)).filter(Boolean)));
    setInputValue("");
  }

  function commitInput(value = inputValue) {
    if (disabled) return;
    const nextValue = normalizeListItem(value, wikilinks);
    if (!nextValue || selectedValues.includes(nextValue)) return;
    updateValues([...selectedValues, nextValue]);
    setOpen(false);
  }

  function removeValue(value: string) {
    if (disabled) return;
    updateValues(selectedValues.filter((item) => item !== value));
  }

  function chooseValue(value: string) {
    if (disabled) return;
    updateValues([...selectedValues, value]);
    setOpen(false);
  }

  function handleKeyDown(event: KeyboardEvent<HTMLInputElement>) {
    if (disabled || (event.key !== "Enter" && event.key !== ",")) return;
    event.preventDefault();
    if (items[0]) chooseValue(items[0].value);
    else commitInput(event.currentTarget.value);
  }

  return (
    <div ref={containerRef} className="relative">
      <div
        role="toolbar"
        aria-label={ariaLabel}
        className="border-input bg-background focus-within:ring-ring flex min-h-9 w-full flex-wrap items-center gap-1 rounded-md border px-2 py-1 shadow-xs focus-within:ring-2"
      >
        {selectedValues.map((value) => (
          <span
            key={value}
            className="bg-secondary text-secondary-foreground inline-flex max-w-full items-center gap-1 rounded-md border-transparent px-2 py-0.5 text-xs font-medium"
          >
            <span className="truncate">{labels.get(value) ?? value}</span>
            <button
              type="button"
              className="text-muted-foreground hover:text-foreground rounded-sm outline-none"
              onClick={() => removeValue(value)}
              aria-label={`Remove ${value}`}
              disabled={disabled}
            >
              <XIcon />
            </button>
          </span>
        ))}
        <input
          role="combobox"
          aria-expanded={open}
          aria-haspopup="listbox"
          aria-autocomplete="list"
          aria-label={ariaLabel}
          value={inputValue}
          placeholder={selectedValues.length === 0 ? placeholder : ""}
          onFocus={() => {
            if (!disabled) setOpen(true);
          }}
          onChange={(event) => {
            setInputValue(event.target.value);
            if (!disabled) setOpen(true);
          }}
          onKeyDown={handleKeyDown}
          onBlur={(event) => commitInput(event.currentTarget.value)}
          className="placeholder:text-muted-foreground min-w-28 flex-1 bg-transparent px-1 py-0.5 text-sm outline-none disabled:cursor-not-allowed disabled:opacity-50"
          disabled={disabled}
        />
      </div>
      {open ? (
        <div className="bg-popover text-popover-foreground border-border absolute top-full right-0 left-0 mt-1 max-h-64 overflow-hidden rounded-md border shadow-md">
          <div role="listbox" className="max-h-64 overflow-auto p-1 outline-none">
            {items.length > 0 ? (
              items.map((item) => (
                <button
                  key={item.value}
                  type="button"
                  role="option"
                  aria-selected={selectedValues.includes(item.value)}
                  className="hover:bg-accent hover:text-accent-foreground flex w-full cursor-default items-center gap-2 rounded-sm px-2 py-1.5 text-left text-sm outline-none"
                  onMouseDown={(event) => event.preventDefault()}
                  onClick={() => chooseValue(item.value)}
                >
                  <span className="min-w-0 flex-1 truncate">
                    {customItem && item.value === customItem.value ? `Add "${item.value}"` : (item.label ?? item.value)}
                  </span>
                  {item.detail ? <span className="truncate text-xs text-muted-foreground">{item.detail}</span> : null}
                  {selectedValues.includes(item.value) ? <CheckIcon /> : null}
                </button>
              ))
            ) : loadingOptions ? (
              <div className="px-3 py-6 text-center text-sm text-muted-foreground">Searching...</div>
            ) : optionsError ? (
              <div className="px-3 py-6 text-center text-sm text-destructive">{optionsError}</div>
            ) : (
              <div className="px-3 py-6 text-center text-sm text-muted-foreground">No values found.</div>
            )}
          </div>
        </div>
      ) : null}
    </div>
  );
}

function SeasonListInput({
  values,
  language,
  ariaLabel,
  disabled,
  onChange,
}: {
  values: string[];
  language: SeasonLanguage;
  ariaLabel: string;
  disabled: boolean;
  onChange: (value: FrontmatterValue) => void;
}) {
  const seasons = seasonOptions(language);
  const rows = values.map(parseSeasonValue);

  function updateRows(nextRows: SeasonRow[]) {
    onChange(
      nextRows
        .filter((row) => (row.kind === "raw" ? row.value.trim() : row.year.trim()))
        .map((row) => (row.kind === "raw" ? row.value : formatSeasonValue(row, language))),
    );
  }

  function updateRow(index: number, row: SeasonRow) {
    updateRows(rows.map((item, itemIndex) => (itemIndex === index ? row : item)));
  }

  return (
    <div className="flex flex-col gap-2">
      {rows.map((row, index) => (
        <div key={index} className="flex items-center gap-2">
          {row.kind === "raw" ? (
            <Input
              value={row.value}
              onChange={(event) => updateRow(index, { kind: "raw", value: event.target.value })}
              className="min-w-0 flex-1"
              aria-label={ariaLabel}
              disabled={disabled}
            />
          ) : (
            <>
              <Input
                type="number"
                min={1900}
                max={2100}
                value={row.year}
                onChange={(event) => updateRow(index, { ...row, year: event.target.value })}
                className="w-28"
                aria-label={`${ariaLabel} year`}
                disabled={disabled}
              />
              <Select
                value={row.season}
                onChange={(event) => updateRow(index, { ...row, season: event.target.value as SeasonKey })}
                className="min-w-32 flex-1"
                aria-label={`${ariaLabel} season`}
                disabled={disabled}
              >
                {seasons.map((season) => (
                  <option key={season.key} value={season.key}>
                    {season.label}
                  </option>
                ))}
              </Select>
            </>
          )}
          <Button
            type="button"
            variant="ghost"
            size="icon"
            onClick={() => updateRows(rows.filter((_, itemIndex) => itemIndex !== index))}
            aria-label={`Remove ${ariaLabel}`}
            disabled={disabled}
          >
            <Trash2Icon />
          </Button>
        </div>
      ))}
      <Button
        type="button"
        variant="outline"
        size="sm"
        onClick={() => updateRows([...rows, { kind: "season", year: String(new Date().getFullYear()), season: "spring" }])}
        disabled={disabled}
      >
        <PlusIcon data-icon="inline-start" />
        Add Season
      </Button>
    </div>
  );
}

function ObjectValueInput({
  value,
  disabled,
  onChange,
  ariaLabel,
}: {
  value: FrontmatterValue | undefined;
  disabled: boolean;
  onChange: (value: FrontmatterValue) => void;
  ariaLabel: string;
}) {
  const [newKey, setNewKey] = useState("");
  const object = isFrontmatterObject(value) ? value : {};
  const entries = Object.entries(object);

  function updateObject(next: FrontmatterObject) {
    onChange(next);
  }

  function renameKey(oldKey: string, nextKey: string) {
    const key = nextKey.trim();
    if (!key || key === oldKey || key in object) return;
    const next = { ...object, [key]: object[oldKey] };
    delete next[oldKey];
    updateObject(next);
  }

  function updateValue(key: string, nextValue: string) {
    updateObject({ ...object, [key]: parseScalarValue(nextValue) });
  }

  function removeKey(key: string) {
    const next = { ...object };
    delete next[key];
    updateObject(next);
  }

  function addKey() {
    const key = newKey.trim();
    if (!key || key in object) return;
    updateObject({ ...object, [key]: "" });
    setNewKey("");
  }

  return (
    <div className="flex flex-col gap-2" aria-label={ariaLabel}>
      {entries.map(([key, item]) => (
        <div key={key} className="grid min-w-0 grid-cols-[minmax(0,0.8fr)_minmax(0,1fr)_auto] gap-2">
          <Input
            value={key}
            onChange={(event) => renameKey(key, event.target.value)}
            className="font-mono text-xs"
            disabled={disabled}
            aria-label={`${ariaLabel} key`}
          />
          {isScalarFrontmatterValue(item) ? (
            <Input
              value={valueToText(item)}
              onChange={(event) => updateValue(key, event.target.value)}
              disabled={disabled}
              aria-label={`${ariaLabel} value`}
            />
          ) : (
            <div className="min-w-0 truncate rounded-md border bg-muted px-3 py-2 text-xs text-muted-foreground">
              Nested value
            </div>
          )}
          <Button
            type="button"
            variant="ghost"
            size="icon"
            onClick={() => removeKey(key)}
            disabled={disabled}
            aria-label={`Remove ${key}`}
          >
            <Trash2Icon />
          </Button>
        </div>
      ))}
      <div className="flex min-w-0 gap-2">
        <Input
          value={newKey}
          onChange={(event) => setNewKey(event.target.value)}
          placeholder="property"
          className="font-mono text-xs"
          disabled={disabled}
          aria-label={`${ariaLabel} new property`}
        />
        <Button type="button" variant="outline" onClick={addKey} disabled={disabled || !newKey.trim()}>
          <PlusIcon data-icon="inline-start" />
          Add
        </Button>
      </div>
    </div>
  );
}

export function NumberStepper({
  value,
  onChange,
  disabled = false,
  ariaLabel,
}: {
  value: string;
  onChange: (value: string) => void;
  disabled?: boolean;
  ariaLabel: string;
}) {
  const number = Number(value || 0);
  const current = Number.isFinite(number) ? number : 0;
  const step = (delta: number) => onChange(String(Math.max(0, current + delta)));
  const keyStep = (event: KeyboardEvent<HTMLButtonElement>, delta: number) => {
    if (event.key !== "Enter" && event.key !== " ") return;
    event.preventDefault();
    step(delta);
  };
  return (
    <div className="flex min-w-0 items-center gap-1">
      <Button
        type="button"
        variant="outline"
        size="icon"
        disabled={disabled}
        onClick={() => step(-1)}
        onKeyDown={(event) => keyStep(event, -1)}
        aria-label={`Decrease ${ariaLabel}`}
      >
        <MinusIcon />
      </Button>
      <Input
        type="number"
        min={0}
        step={1}
        value={value}
        onChange={(event) => onChange(event.target.value)}
        disabled={disabled}
        className="min-w-0 text-center tabular-nums"
        aria-label={ariaLabel}
      />
      <Button
        type="button"
        variant="outline"
        size="icon"
        disabled={disabled}
        onClick={() => step(1)}
        onKeyDown={(event) => keyStep(event, 1)}
        aria-label={`Increase ${ariaLabel}`}
      >
        <PlusIcon />
      </Button>
    </div>
  );
}

function ProgressStepper({
  value,
  onChange,
  disabled = false,
  ariaLabel,
}: {
  value: string;
  onChange: (value: string) => void;
  disabled?: boolean;
  ariaLabel: string;
}) {
  const number = Number(value || 0);
  const current = Number.isFinite(number) ? number : 0;
  const step = (delta: number) => onChange(String(Math.max(0, current + delta)));
  return (
    <div className="grid min-w-0 grid-cols-[auto_minmax(4rem,1fr)_auto] items-center gap-1">
      <Button
        type="button"
        variant="outline"
        size="icon"
        disabled={disabled}
        onClick={() => step(-1)}
        aria-label={`Decrease ${ariaLabel}`}
      >
        <MinusIcon />
      </Button>
      <output
        className="border-input bg-muted text-foreground flex h-9 min-w-0 items-center justify-center rounded-md border px-3 text-center text-sm tabular-nums"
        aria-label={ariaLabel}
      >
        {value || "0"}
      </output>
      <Button
        type="button"
        variant="outline"
        size="icon"
        disabled={disabled}
        onClick={() => step(1)}
        aria-label={`Increase ${ariaLabel}`}
      >
        <PlusIcon />
      </Button>
    </div>
  );
}

function DatePickerInput({
  value,
  onChange,
  disabled,
  ariaLabel,
}: {
  value: string;
  onChange: (value: string) => void;
  disabled: boolean;
  ariaLabel: string;
}) {
  const [open, setOpen] = useState(false);
  const selectedDate = parseDateValue(value);

  function chooseDate(date: Date | undefined) {
    onChange(date ? formatDateValue(date) : "");
    setOpen(false);
  }

  function clearDate() {
    onChange("");
    setOpen(false);
  }

  return (
    <div className="flex min-w-0 gap-1">
      <Popover open={open} onOpenChange={setOpen}>
        <PopoverTrigger asChild>
          <Button
            type="button"
            variant="outline"
            data-empty={!value}
            className="min-w-0 flex-1 justify-start text-left font-normal data-[empty=true]:text-muted-foreground"
            disabled={disabled}
            aria-label={ariaLabel}
          >
            <CalendarIcon data-icon="inline-start" />
            <span className="truncate">{value || "Pick a date"}</span>
          </Button>
        </PopoverTrigger>
        <PopoverContent className="w-auto p-0" align="start">
          <Calendar
            mode="single"
            selected={selectedDate}
            defaultMonth={selectedDate}
            onSelect={chooseDate}
            disabled={disabled}
          />
        </PopoverContent>
      </Popover>
      {value ? (
        <Button
          type="button"
          variant="outline"
          size="icon"
          onClick={clearDate}
          disabled={disabled}
          aria-label={`Clear ${ariaLabel}`}
        >
          <XIcon />
        </Button>
      ) : null}
    </div>
  );
}

function editableFieldSpecs(
  typeConfig: TypeConfig | undefined,
  frontmatter: FrontmatterDraft,
  relationSuggestions: EntitySummary[],
  onRelationSearch: RelationSuggestionSearch | undefined,
) {
  const specs: EditableFieldSpec[] = [];
  const seen = new Set<string>();

  for (const field of configFields(typeConfig)) {
    const key = field.field.trim();
    if (!key || seen.has(key) || isVirtualTitleField(key)) continue;
    seen.add(key);
    specs.push({
      key,
      label: configuredFieldLabel(field),
      kind: fieldKind(field, frontmatter[key]),
      options: optionsForConfiguredField(field, frontmatter[key]),
      relationOptions: field.fieldType === "relation" ? relationOptionsForField(field, relationSuggestions) : [],
      loadRelationOptions:
        field.fieldType === "relation" && onRelationSearch
          ? async (query, signal) => relationOptionsForField(field, await onRelationSearch({ relationType: field.relationType, query, signal }))
          : undefined,
      relationType: field.fieldType === "relation" ? field.relationType : undefined,
      seasonLanguage: normalizeSeasonLanguage(field.seasonLanguage),
      configured: true,
    });
  }

  for (const key of Object.keys(frontmatter)) {
    if (seen.has(key)) continue;
    seen.add(key);
    specs.push({
      key,
      label: humanizeField(key),
      kind: "text",
      options: [],
      relationOptions: [],
      loadRelationOptions: undefined,
      seasonLanguage: "zh",
      configured: false,
    });
  }

  return specs;
}

function fieldKind(field: FieldConfig, value: FrontmatterValue | undefined): FieldKind {
  if (field.fieldType === "season") return "season";
  if (field.fieldType === "date") return "date";
  if (field.fieldType === "relation") return "relation";
  if (isListFieldType(field.fieldType) || Array.isArray(value)) return "list";
  if (field.fieldType === "enum") return "select";
  if (field.fieldType === "bool") return "boolean";
  if (field.fieldType === "progress") return "progress";
  if (field.fieldType === "totalProgress" || field.fieldType === "rating") {
    return "number";
  }
  return "text";
}

function optionsForConfiguredField(field: FieldConfig, value: FrontmatterValue | undefined) {
  const options = new Set(field.enumOptions ?? []);
  for (const current of currentOptions(value)) {
    options.add(current);
  }
  return [...options];
}

function relationOptionsForField(field: FieldConfig, suggestions: EntitySummary[]) {
  const relationType = normalizeRelationType(field.relationType);
  return suggestions
    .filter((item) => !relationType || entityMatchesRelationType(item, relationType))
    .map((item) => ({
      value: item.basename,
      label: item.title,
      detail: `${item.typeLabel} - ${item.path}`,
    }))
    .filter((item) => item.value);
}

function entityMatchesRelationType(item: EntitySummary, relationType: string) {
  return normalizeRelationType(item.type) === relationType || normalizeRelationType(item.typeLabel) === relationType;
}

function normalizeRelationType(value: string | null | undefined) {
  return (value ?? "").trim().toLowerCase().replace(/[\s_-]+/g, "");
}

function relationPlaceholder(relationType: string | null | undefined) {
  const normalized = (relationType ?? "").trim();
  return normalized ? `Search or add ${normalized}` : "Search or add relation";
}

function parseDateValue(value: string) {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(value.trim());
  if (!match) return undefined;
  const year = Number(match[1]);
  const month = Number(match[2]);
  const day = Number(match[3]);
  const date = new Date(year, month - 1, day);
  if (date.getFullYear() !== year || date.getMonth() !== month - 1 || date.getDate() !== day) {
    return undefined;
  }
  return date;
}

function formatDateValue(date: Date) {
  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  return `${year}-${month}-${day}`;
}

function currentOptions(value: FrontmatterValue | undefined) {
  const options = new Set<string>();
  const currentValues = Array.isArray(value) ? value.map(valueToText) : [valueToText(value)];
  for (const current of currentValues) {
    if (current) options.add(current);
  }
  return [...options];
}

function normalizeSeasonLanguage(language: FieldConfig["seasonLanguage"] | undefined): SeasonLanguage {
  return language === "en" || language === "ja" ? language : "zh";
}

export function normalizeFrontmatter(value: Record<string, unknown>) {
  return Object.fromEntries(
    Object.entries(value).map(([key, item]) => [key, normalizeFrontmatterValue(item)]),
  ) as FrontmatterDraft;
}

function normalizeFrontmatterValue(value: unknown): FrontmatterValue {
  if (value === null || ["boolean", "number", "string"].includes(typeof value)) {
    return value as FrontmatterValue;
  }
  if (Array.isArray(value)) {
    return value.map(normalizeFrontmatterValue);
  }
  if (typeof value === "object" && value) {
    return Object.fromEntries(
      Object.entries(value).map(([key, item]) => [key, normalizeFrontmatterValue(item)]),
    );
  }
  return String(value ?? "");
}

function valueToText(value: FrontmatterValue | undefined): string {
  if (value === null || value === undefined) return "";
  if (typeof value === "string") return value;
  if (typeof value === "number" || typeof value === "boolean") return String(value);
  return JSON.stringify(value);
}

function isFrontmatterObject(value: FrontmatterValue | undefined): value is FrontmatterObject {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isScalarFrontmatterValue(value: FrontmatterValue | undefined): value is null | boolean | number | string {
  return value === null || ["boolean", "number", "string"].includes(typeof value);
}

function parseScalarValue(value: string): FrontmatterValue {
  const trimmed = value.trim();
  if (!trimmed) return null;
  if (trimmed === "true") return true;
  if (trimmed === "false") return false;
  const number = Number(trimmed);
  if (Number.isFinite(number) && String(number) === trimmed) return number;
  return value;
}

export function frontmatterPatch(original: Record<string, unknown>, next: FrontmatterDraft) {
  const patch: Record<string, unknown> = { ...next };
  for (const key of Object.keys(original)) {
    if (!(key in next)) patch[key] = null;
  }
  return patch;
}

export function numberOrString(value: string) {
  if (!value) return null;
  const number = Number(value);
  return Number.isFinite(number) && String(number) === value ? number : value;
}

function withCurrentOption(options: string[], current: string) {
  if (!current || options.includes(current)) return options;
  return [current, ...options];
}

function uniqueStrings(values: string[]) {
  return values.filter((value, index) => value && values.indexOf(value) === index);
}

function uniqueOptions(options: MultiValueOption[], wikilinks: boolean) {
  const seen = new Set<string>();
  const normalized: MultiValueOption[] = [];
  for (const option of options) {
    const value = normalizeListItem(option.value, wikilinks);
    if (!value || seen.has(value)) continue;
    seen.add(value);
    normalized.push({ ...option, value });
  }
  return normalized;
}

function listDisplayValues(value: FrontmatterValue | undefined, wikilinks: boolean) {
  const values = Array.isArray(value) ? value : value === null || value === undefined || value === "" ? [] : [value];
  return values
    .map(valueToText)
    .map((item) => (wikilinks ? stripWikilink(item) : item))
    .map((item) => normalizeListItem(item, wikilinks))
    .filter(Boolean);
}

function parseSeasonValue(value: string): SeasonRow {
  const year = /(?:19|20)\d{2}/.exec(value)?.[0];
  const normalized = value.toLowerCase();
  let season: SeasonKey | undefined;
  if (value.includes("冬季") || normalized.includes("winter")) season = "winter";
  else if (value.includes("夏季") || normalized.includes("summer")) season = "summer";
  else if (value.includes("秋季") || normalized.includes("autumn") || normalized.includes("fall")) season = "autumn";
  else if (value.includes("春季") || normalized.includes("spring")) season = "spring";
  if (!year || !season) return { kind: "raw", value };
  return { kind: "season", year, season };
}

function formatSeasonValue(row: Extract<SeasonRow, { kind: "season" }>, language: SeasonLanguage) {
  const year = row.year.trim();
  if (language === "en") {
    const label = seasonOptions(language).find((season) => season.key === row.season)?.label ?? "Spring";
    return `${label} ${year}`;
  }
  const label = seasonOptions(language).find((season) => season.key === row.season)?.label ?? "春季";
  return `${year}年${label}`;
}

function seasonOptions(language: SeasonLanguage): Array<{ key: SeasonKey; label: string }> {
  if (language === "en") {
    return [
      { key: "winter", label: "Winter" },
      { key: "spring", label: "Spring" },
      { key: "summer", label: "Summer" },
      { key: "autumn", label: "Autumn" },
    ];
  }
  return [
    { key: "winter", label: "冬季" },
    { key: "spring", label: "春季" },
    { key: "summer", label: "夏季" },
    { key: "autumn", label: "秋季" },
  ];
}

function normalizeListItem(value: string, wikilinks: boolean) {
  const trimmed = value.trim();
  if (!trimmed) return "";
  return wikilinks ? stripWikilink(trimmed) : trimmed;
}

function toWikilink(value: string) {
  const target = stripWikilink(value).trim();
  return target ? `[[${target}]]` : "";
}

function stripWikilink(value: string) {
  const trimmed = value.trim();
  const match = /^\[\[(.*?)(?:\|.*?)?\]\]$/.exec(trimmed);
  return match?.[1]?.trim() ?? trimmed;
}

function isAbortError(error: unknown) {
  return error instanceof DOMException && error.name === "AbortError";
}

function humanizeField(key: string) {
  return key.replace(/[_-]+/g, " ");
}

function isVirtualTitleField(key: string) {
  return ["filename", "basename", "$filename", "$basename"].includes(key);
}
