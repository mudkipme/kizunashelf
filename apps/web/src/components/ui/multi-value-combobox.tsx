import type { KeyboardEvent } from "react";
import { useMemo, useRef, useState } from "react";
import { useLingui } from "@lingui/react/macro";

import {
  Combobox,
  ComboboxChip,
  ComboboxChips,
  ComboboxChipsInput,
  ComboboxContent,
  ComboboxEmpty,
  ComboboxItem,
  ComboboxList,
  ComboboxValue,
} from "@/components/ui/combobox";
import { cn } from "@/lib/utils";

export type MultiValueComboboxOption = {
  value: string;
  label?: string;
  detail?: string;
};

export function MultiValueCombobox({
  values,
  options,
  placeholder = "Add value",
  ariaLabel,
  disabled = false,
  allowCustomValue = false,
  inputValue,
  onInputValueChange,
  loading = false,
  error,
  emptyText = "No values found.",
  className,
  normalizeValue = defaultNormalizeValue,
  formatChipLabel,
  onOpenChange,
  onChange,
}: {
  values: string[];
  options: MultiValueComboboxOption[];
  placeholder?: string;
  ariaLabel: string;
  disabled?: boolean;
  allowCustomValue?: boolean;
  inputValue?: string;
  onInputValueChange?: (value: string) => void;
  loading?: boolean;
  error?: string;
  emptyText?: string;
  className?: string;
  normalizeValue?: (value: string) => string;
  formatChipLabel?: (value: string) => string;
  onOpenChange?: (open: boolean) => void;
  onChange: (values: string[]) => void;
}) {
  const { t } = useLingui();
  // The popup anchors to the chips box, not to the bare caret input inside it.
  // Left to itself the positioner takes the input as the anchor — and that input
  // is only the sliver of space after the last chip, so the list opened partway
  // across the field, at whatever width the caret happened to have, and moved
  // every time a chip was added. Anchoring here also feeds `--anchor-width` the
  // field's real width, which is what `data-chips` sizes the popup from.
  const chipsRef = useRef<HTMLDivElement>(null);
  const [uncontrolledInputValue, setUncontrolledInputValue] = useState("");
  const currentInputValue = inputValue ?? uncontrolledInputValue;
  const selectedValues = useMemo(
    () => uniqueStrings(values.map(normalizeValue).filter(Boolean)),
    [normalizeValue, values],
  );
  const normalizedOptions = useMemo(
    () =>
      uniqueOptions(
        [
          ...options.map((option) => ({
            ...option,
            value: normalizeValue(option.value),
          })),
          ...selectedValues.map((value) => ({ value })),
        ],
        normalizeValue,
      ),
    [normalizeValue, options, selectedValues],
  );
  const labels = useMemo(() => new Map(normalizedOptions.map((option) => [option.value, option.label || option.value])), [
    normalizedOptions,
  ]);
  const details = useMemo(() => new Map(normalizedOptions.map((option) => [option.value, option.detail])), [
    normalizedOptions,
  ]);
  const query = currentInputValue.trim().toLowerCase();
  const customValue = normalizeValue(currentInputValue);
  const optionValues = normalizedOptions.map((option) => option.value);
  const matchingOptions = normalizedOptions
    .filter((option) => !query || optionMatchesQuery(option, query))
    .map((option) => option.value);
  const customItem =
    allowCustomValue && customValue && !selectedValues.includes(customValue) && !optionValues.includes(customValue)
      ? customValue
      : undefined;
  const items = uniqueStrings([customItem, ...matchingOptions].filter((item): item is string => Boolean(item)));

  function setCurrentInputValue(value: string) {
    if (inputValue === undefined) setUncontrolledInputValue(value);
    onInputValueChange?.(value);
  }

  function updateValues(nextValues: string[]) {
    if (disabled) return;
    onChange(uniqueStrings(nextValues.map(normalizeValue).filter(Boolean)));
    setCurrentInputValue("");
  }

  function commitInput() {
    if (!allowCustomValue || disabled || !customValue || selectedValues.includes(customValue)) return;
    updateValues([...selectedValues, customValue]);
  }

  function handleKeyDown(event: KeyboardEvent<HTMLInputElement>) {
    if (event.key !== ",") return;
    event.preventDefault();
    commitInput();
  }

  return (
    <Combobox
      items={items}
      multiple
      value={selectedValues}
      onValueChange={updateValues}
      inputValue={currentInputValue}
      onInputValueChange={setCurrentInputValue}
      onOpenChange={onOpenChange}
      filter={null}
      disabled={disabled}
    >
      <ComboboxChips ref={chipsRef} className={cn("w-full", className)}>
        <ComboboxValue>
          {selectedValues.map((item) => (
            <ComboboxChip key={item}>{formatChipLabel?.(item) ?? labels.get(item) ?? item}</ComboboxChip>
          ))}
        </ComboboxValue>
        <ComboboxChipsInput
          aria-label={ariaLabel}
          placeholder={selectedValues.length === 0 ? placeholder : ""}
          onKeyDown={handleKeyDown}
          disabled={disabled}
        />
      </ComboboxChips>
      <ComboboxContent anchor={chipsRef}>
        <ComboboxEmpty>{loading ? t`Searching…` : error ? error : emptyText}</ComboboxEmpty>
        <ComboboxList>
          {(item) => (
            <ComboboxItem key={item} value={item}>
              <span className="min-w-0 flex-1 truncate">
                {customItem && item === customItem ? t`Add "${item}"` : (labels.get(item) ?? item)}
              </span>
              {details.get(item) ? <span className="truncate text-xs text-muted-foreground">{details.get(item)}</span> : null}
            </ComboboxItem>
          )}
        </ComboboxList>
      </ComboboxContent>
    </Combobox>
  );
}

function defaultNormalizeValue(value: string) {
  return value.trim();
}

function uniqueStrings(values: string[]) {
  return values.filter((value, index) => value && values.indexOf(value) === index);
}

function uniqueOptions(
  options: MultiValueComboboxOption[],
  normalizeValue: (value: string) => string,
) {
  const seen = new Set<string>();
  const unique: MultiValueComboboxOption[] = [];
  for (const option of options) {
    const value = normalizeValue(option.value);
    if (!value || seen.has(value)) continue;
    seen.add(value);
    unique.push({ ...option, value });
  }
  return unique;
}

function optionMatchesQuery(option: MultiValueComboboxOption, query: string) {
  return (
    option.value.toLowerCase().includes(query) ||
    option.label?.toLowerCase().includes(query) ||
    option.detail?.toLowerCase().includes(query)
  );
}
