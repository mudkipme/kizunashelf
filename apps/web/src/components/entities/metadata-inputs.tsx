import type { DragEvent, KeyboardEvent } from "react";
import { useEffect, useRef, useState } from "react";
import { Trans, useLingui } from "@lingui/react/macro";
import { CalendarIcon, ImageIcon, Loader2Icon, MinusIcon, PlusIcon, Trash2Icon, UploadIcon, XIcon } from "lucide-react";
import { toast } from "sonner";

import { uploadAsset } from "@/api/entities";
import { AssetImage } from "@/components/assets/asset-image";
import { Button } from "@/components/ui/button";
import { Calendar } from "@/components/ui/calendar";
import { Input } from "@/components/ui/input";
import { MultiValueCombobox } from "@/components/ui/multi-value-combobox";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { Select } from "@/components/ui/select";
import { cn } from "@/lib/utils";

import {
  formatDateValue,
  formatSeasonValue,
  isAbortError,
  isFrontmatterObject,
  isScalarFrontmatterValue,
  listDisplayValues,
  normalizeListItem,
  numberOrString,
  parseDateValue,
  parseScalarValue,
  parseSeasonValue,
  seasonOptions,
  toWikilink,
  uniqueStrings,
  valueToText,
  withCurrentOption,
} from "./frontmatter-utils";
import type {
  EditableFieldSpec,
  FrontmatterObject,
  FrontmatterValue,
  MultiValueOption,
  SeasonLanguage,
  SeasonKey,
  SeasonRow,
} from "./metadata-types";

export function FieldValueInput({
  field,
  value,
  disabled,
  entityId,
  onChange,
}: {
  field: EditableFieldSpec;
  value: FrontmatterValue | undefined;
  disabled: boolean;
  entityId?: string;
  onChange: (value: FrontmatterValue) => void;
}) {
  const { t } = useLingui();

  if (field.kind === "image" || field.kind === "imageList") {
    return (
      <ImageFieldInput
        field={field}
        value={value}
        disabled={disabled}
        entityId={entityId}
        onChange={onChange}
      />
    );
  }

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
        <option value="">{t`Empty`}</option>
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
      <NumberStepper
        value={valueToText(value)}
        onChange={(next) => onChange(numberOrString(next))}
        ariaLabel={field.label}
        disabled={disabled}
        display="output"
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
        onChange={(event) => onChange(event.target.value === "" ? null : event.target.value === "true")}
        className="w-full"
        aria-label={field.label}
        disabled={disabled}
      >
        <option value="">{t`Empty`}</option>
        <option value="true">{t`Yes`}</option>
        <option value="false">{t`No`}</option>
      </Select>
    );
  }

  if (field.kind === "list" || field.kind === "relation") {
    const relation = field.kind === "relation";
    const relationType = (field.relationType ?? "").trim();
    return (
      <MultiValueInput
        values={listDisplayValues(value, relation)}
        options={relation ? field.relationOptions : field.options.map((option) => ({ value: option }))}
        loadOptions={relation ? field.loadRelationOptions : undefined}
        placeholder={
          relation
            ? relationType
              ? t`Search or add ${relationType}`
              : t`Search or add relation`
            : t`Add value`
        }
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

const IMAGE_ACCEPT = "image/*";
const IMAGE_EXTENSION = /\.(png|jpe?g|gif|webp|avif|bmp|svg|tiff?)$/i;

function isImageFile(file: File): boolean {
  return file.type.startsWith("image/") || IMAGE_EXTENSION.test(file.name);
}

/** Encode file bytes as base64 for the upload endpoint, chunked to keep large
 * images off the call stack (`String.fromCharCode(...bytes)` overflows). */
async function fileToBase64(file: File): Promise<string> {
  const bytes = new Uint8Array(await file.arrayBuffer());
  let binary = "";
  const chunk = 0x8000;
  for (let offset = 0; offset < bytes.length; offset += chunk) {
    binary += String.fromCharCode(...bytes.subarray(offset, offset + chunk));
  }
  return btoa(binary);
}

/**
 * Editor for `image` / `imageList` fields: shows thumbnails of the current
 * value(s), keeps the raw path/URL entry (so vault assets and remote URLs still
 * work), and — for an existing entity with writes enabled — adds a file picker /
 * drag-and-drop that uploads bytes into the vault and stages the returned path
 * into the draft (persisted on the normal Save).
 */
function ImageFieldInput({
  field,
  value,
  disabled,
  entityId,
  onChange,
}: {
  field: EditableFieldSpec;
  value: FrontmatterValue | undefined;
  disabled: boolean;
  entityId?: string;
  onChange: (value: FrontmatterValue) => void;
}) {
  const { t } = useLingui();
  const multiple = field.kind === "imageList";
  const values = listDisplayValues(value, false);
  const single = multiple ? "" : valueToText(value);
  const [uploading, setUploading] = useState(false);
  const [dragging, setDragging] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);
  const canUpload = Boolean(entityId) && !disabled;

  async function uploadFiles(files: File[]) {
    if (!entityId || uploading) return;
    const images = files.filter(isImageFile);
    if (images.length === 0) {
      if (files.length > 0) toast.error(t`Only image files can be uploaded.`);
      return;
    }
    setUploading(true);
    try {
      const added: string[] = [];
      for (const file of multiple ? images : images.slice(0, 1)) {
        const result = await uploadAsset(entityId, {
          field: field.key,
          dataBase64: await fileToBase64(file),
          contentType: file.type || undefined,
          filename: file.name || undefined,
        });
        added.push(result.path);
      }
      if (multiple) onChange(uniqueStrings([...values, ...added]));
      else if (added[0]) onChange(added[0]);
    } catch (error) {
      toast.error(error instanceof Error ? error.message : t`Upload failed.`);
    } finally {
      setUploading(false);
    }
  }

  function onDrop(event: DragEvent<HTMLDivElement>) {
    event.preventDefault();
    setDragging(false);
    if (canUpload) void uploadFiles(Array.from(event.dataTransfer.files));
  }

  return (
    <div className="flex flex-col gap-2">
      {multiple
        ? values.length > 0 && (
            <div className="flex flex-wrap gap-2">
              {values.map((item, index) => (
                <div key={`${item}-${index}`} className="group relative size-16 overflow-hidden rounded-md border">
                  <AssetImage src={item} alt="" className="size-full object-cover" fallback={<ImageThumbFallback />} lightbox />
                  {!disabled ? (
                    <button
                      type="button"
                      onClick={() => onChange(values.filter((_, itemIndex) => itemIndex !== index))}
                      aria-label={t`Remove image`}
                      className="bg-background text-muted-foreground absolute right-0.5 top-0.5 rounded-full border p-0.5 opacity-0 shadow-sm transition-opacity group-hover:opacity-100"
                    >
                      <XIcon className="size-3" />
                    </button>
                  ) : null}
                </div>
              ))}
            </div>
          )
        : single && (
            <div className="size-24 overflow-hidden rounded-md border">
              <AssetImage src={single} alt="" className="size-full object-cover" fallback={<ImageThumbFallback />} lightbox />
            </div>
          )}

      {multiple ? (
        <MultiValueInput
          values={values}
          options={[]}
          placeholder={t`Add path or URL`}
          ariaLabel={field.label}
          wikilinks={false}
          disabled={disabled}
          onChange={onChange}
        />
      ) : (
        <Input
          value={single}
          onChange={(event) => onChange(event.target.value || null)}
          placeholder={t`Vault path or URL`}
          aria-label={field.label}
          disabled={disabled}
        />
      )}

      {canUpload ? (
        <div
          onDragOver={(event) => {
            event.preventDefault();
            setDragging(true);
          }}
          onDragLeave={() => setDragging(false)}
          onDrop={onDrop}
          className={cn(
            "text-muted-foreground flex items-center justify-between gap-2 rounded-md border border-dashed px-3 py-2 text-xs transition-colors",
            dragging && "border-primary bg-primary/5",
          )}
        >
          <span className="truncate">
            {uploading ? "Uploading…" : "Drop an image or upload from your device"}
          </span>
          <input
            ref={inputRef}
            type="file"
            accept={IMAGE_ACCEPT}
            multiple={multiple}
            className="hidden"
            onChange={(event) => {
              void uploadFiles(Array.from(event.target.files ?? []));
              event.target.value = "";
            }}
          />
          <Button type="button" variant="outline" size="sm" disabled={uploading} onClick={() => inputRef.current?.click()}>
            {uploading ? (
              <Loader2Icon data-icon="inline-start" className="animate-spin" />
            ) : (
              <UploadIcon data-icon="inline-start" />
            )}
            Upload
          </Button>
        </div>
      ) : null}
    </div>
  );
}

function ImageThumbFallback() {
  return (
    <div className="bg-muted text-muted-foreground flex size-full items-center justify-center">
      <ImageIcon className="size-5" />
    </div>
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
  const selectedValues = uniqueStrings(values.map((value) => normalizeListItem(value, wikilinks)).filter(Boolean));
  const normalizedOptions = uniqueOptions([...options, ...remoteOptions], wikilinks);
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

  function updateValues(nextValues: string[]) {
    if (disabled) return;
    onChange(uniqueStrings(nextValues.map((value) => normalizeListItem(value, wikilinks)).filter(Boolean)));
    setInputValue("");
  }

  return (
    <MultiValueCombobox
      values={selectedValues}
      options={normalizedOptions}
      placeholder={placeholder}
      ariaLabel={ariaLabel}
      disabled={disabled}
      allowCustomValue
      inputValue={inputValue}
      onInputValueChange={setInputValue}
      loading={loadingOptions}
      error={optionsError}
      normalizeValue={(value) => normalizeListItem(value, wikilinks)}
      formatChipLabel={(value) => labels.get(value) ?? value}
      onOpenChange={setOpen}
      onChange={updateValues}
    />
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
  const { t } = useLingui();
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
          placeholder={t`property`}
          className="font-mono text-xs"
          disabled={disabled}
          aria-label={t`${ariaLabel} new property`}
        />
        <Button type="button" variant="outline" onClick={addKey} disabled={disabled || !newKey.trim()}>
          <PlusIcon data-icon="inline-start" />
          <Trans>Add</Trans>
        </Button>
      </div>
    </div>
  );
}

/// A +/- stepper over a non-negative integer held as a string. `display="input"`
/// (the default) renders an editable number field; `display="output"` renders a
/// read-only value (used for progress, where the count is only nudged via the
/// buttons). Only the editable variant maps Enter/Space on the buttons to a step.
export function NumberStepper({
  value,
  onChange,
  disabled = false,
  ariaLabel,
  display = "input",
}: {
  value: string;
  onChange: (value: string) => void;
  disabled?: boolean;
  ariaLabel: string;
  display?: "input" | "output";
}) {
  const number = Number(value || 0);
  const current = Number.isFinite(number) ? number : 0;
  const step = (delta: number) => onChange(String(Math.max(0, current + delta)));
  const keyStep = (event: KeyboardEvent<HTMLButtonElement>, delta: number) => {
    if (event.key !== "Enter" && event.key !== " ") return;
    event.preventDefault();
    step(delta);
  };
  const isOutput = display === "output";
  return (
    <div
      className={
        isOutput
          ? "grid min-w-0 grid-cols-[auto_minmax(4rem,1fr)_auto] items-center gap-1"
          : "flex min-w-0 items-center gap-1"
      }
    >
      <Button
        type="button"
        variant="outline"
        size="icon"
        disabled={disabled}
        onClick={() => step(-1)}
        onKeyDown={isOutput ? undefined : (event) => keyStep(event, -1)}
        aria-label={`Decrease ${ariaLabel}`}
      >
        <MinusIcon />
      </Button>
      {isOutput ? (
        <output
          className="border-input bg-muted text-foreground flex h-9 min-w-0 items-center justify-center rounded-md border px-3 text-center text-sm tabular-nums"
          aria-label={ariaLabel}
        >
          {value || "0"}
        </output>
      ) : (
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
      )}
      <Button
        type="button"
        variant="outline"
        size="icon"
        disabled={disabled}
        onClick={() => step(1)}
        onKeyDown={isOutput ? undefined : (event) => keyStep(event, 1)}
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

