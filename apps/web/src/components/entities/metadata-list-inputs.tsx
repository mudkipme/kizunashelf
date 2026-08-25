//! The list-shaped value editors: a generic multi-value combobox (tags, enum
//! lists, relations — the options come from the schema, never from the field's
//! name) and the season-list editor, whose rows are (year, season) pairs written
//! back in the field's own season language.

import { useEffect, useState } from "react";
import { PlusIcon, Trash2Icon } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { MultiValueCombobox } from "@/components/ui/multi-value-combobox";
import { Select } from "@/components/ui/select";
import { useDebouncedAbortableCallback } from "@/hooks/use-debounce";

import {
  formatSeasonValue,
  isAbortError,
  normalizeListItem,
  parseSeasonValue,
  seasonOptions,
  uniqueStrings,
} from "./frontmatter-utils";
import type {
  FrontmatterValue,
  MultiValueOption,
  SeasonKey,
  SeasonLanguage,
  SeasonRow,
} from "./metadata-types";

export function MultiValueInput({
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

  const { schedule: scheduleOptionsLoad, cancel: cancelOptionsLoad } =
    useDebouncedAbortableCallback(
      (
        signal,
        loader: (query: string, signal: AbortSignal) => Promise<MultiValueOption[]>,
        query: string,
      ) => {
        setLoadingOptions(true);
        setOptionsError(undefined);
        loader(query, signal)
          .then((items) => {
            if (!signal.aborted) setRemoteOptions(items);
          })
          .catch((error) => {
            if (isAbortError(error) || signal.aborted) return;
            setRemoteOptions([]);
            setOptionsError(error instanceof Error ? error.message : "Could not load options.");
          })
          .finally(() => {
            if (!signal.aborted) setLoadingOptions(false);
          });
      },
      200,
    );

  useEffect(() => {
    if (!open || disabled || !loadOptions) {
      cancelOptionsLoad();
      return;
    }
    scheduleOptionsLoad(loadOptions, inputValue.trim());
    return cancelOptionsLoad;
  }, [disabled, inputValue, loadOptions, open, scheduleOptionsLoad, cancelOptionsLoad]);

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

export function SeasonListInput({
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
