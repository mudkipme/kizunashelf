//! The value editor for one rule row — the part that changes shape with the
//! field's kind and the chosen operator: a date picker, an amount + unit pair, a
//! multi-value combobox, an entity search, or nothing at all for the operators
//! that take no value.

import { useEffect, useRef, useState } from "react";
import { useLingui } from "@lingui/react/macro";

import { isAbortError } from "@/api/client";
import { useRelationSearch } from "@/api/use-relation-search";
import type { EditorState } from "@/components/smart-lists/rule-editor-state";
import { builderWords, type RuleFieldMeta } from "@/components/smart-lists/rule-field-meta";
import { Input } from "@/components/ui/input";
import { MultiValueCombobox } from "@/components/ui/multi-value-combobox";
import type { MultiValueComboboxOption } from "@/components/ui/multi-value-combobox";
import { Select } from "@/components/ui/select";
import { useDebouncedAbortableCallback } from "@/hooks/use-debounce";
import { useTitleLanguage } from "@/lib/language";
import { entityTitle } from "@/lib/title-language";

export function RuleValueInput({
  meta,
  op,
  inputs,
  disabled,
  onChange,
}: {
  meta: RuleFieldMeta;
  op: string;
  inputs: EditorState["inputs"];
  disabled: boolean;
  onChange: (inputs: EditorState["inputs"]) => void;
}) {
  const { t } = useLingui();
  if (op === "isEmpty" || op === "hasValue" || op === "isTrue" || op === "isFalse") return null;

  if (op === "inLast" || op === "olderThan" || op === "withinNext") {
    return (
      <>
        <Input
          type="number"
          min={1}
          value={inputs.amount}
          disabled={disabled}
          aria-label={t`Amount`}
          className="w-20"
          onChange={(event) => onChange({ ...inputs, amount: event.target.value })}
        />
        <Select
          value={inputs.unit}
          disabled={disabled}
          aria-label={t`Unit`}
          className="w-fit min-w-0"
          onChange={(event) => onChange({ ...inputs, unit: event.target.value })}
        >
          <option value="days">{t(builderWords.days)}</option>
          <option value="weeks">{t(builderWords.weeks)}</option>
          <option value="months">{t(builderWords.months)}</option>
          <option value="years">{t(builderWords.years)}</option>
        </Select>
      </>
    );
  }

  if (meta.kind === "date" && ["on", "before", "onOrBefore", "after", "onOrAfter"].includes(op)) {
    return (
      <Input
        type="date"
        value={inputs.date}
        disabled={disabled}
        aria-label={t`Date`}
        className="w-40"
        onChange={(event) => onChange({ ...inputs, date: event.target.value })}
      />
    );
  }

  if (meta.kind === "number") {
    return (
      <Input
        type="number"
        step="any"
        value={inputs.number}
        disabled={disabled}
        aria-label={t`Value`}
        className="w-24"
        onChange={(event) => onChange({ ...inputs, number: event.target.value })}
      />
    );
  }

  if (meta.kind === "season" && (op === "isAnyOf" || op === "yearIs")) {
    const options = (op === "yearIs" ? meta.yearOptions : meta.options) ?? [];
    return (
      <MultiValueCombobox
        values={inputs.values}
        options={options.map((option) => ({ value: option }))}
        placeholder={op === "yearIs" ? t`Add year` : t`Add season`}
        ariaLabel={t`Values`}
        disabled={disabled}
        // A season written some other way — by hand, or in another language —
        // still filters: the engine matches the season a value names.
        allowCustomValue
        className="min-h-8 w-64 px-2 py-1 text-xs"
        onChange={(values) => onChange({ ...inputs, values })}
      />
    );
  }

  if ((meta.kind === "enum" || meta.kind === "season") && (op === "is" || op === "isNot")) {
    const options = meta.options ?? [];
    return (
      <Select
        value={inputs.text}
        disabled={disabled}
        aria-label={t`Value`}
        className="w-40 min-w-0"
        onChange={(event) => onChange({ ...inputs, text: event.target.value })}
      >
        {options.includes(inputs.text) ? null : <option value={inputs.text}>{inputs.text}</option>}
        {options.map((option) => (
          <option key={option} value={option}>
            {option}
          </option>
        ))}
      </Select>
    );
  }

  if (meta.kind === "list" || meta.kind === "tags") {
    return (
      <MultiValueCombobox
        values={inputs.values}
        options={(meta.options ?? []).map((option) => ({ value: option }))}
        placeholder={t`Add value`}
        ariaLabel={t`Values`}
        disabled={disabled}
        allowCustomValue={meta.allowCustomValues || meta.kind === "tags"}
        className="min-h-8 w-64 px-2 py-1 text-xs"
        onChange={(values) => onChange({ ...inputs, values })}
      />
    );
  }

  if (meta.kind === "relation") {
    return (
      <RelationTargetPicker
        meta={meta}
        values={inputs.values}
        disabled={disabled}
        onChange={(values) => onChange({ ...inputs, values })}
      />
    );
  }

  return (
    <Input
      value={inputs.text}
      disabled={disabled}
      aria-label={t`Value`}
      className="w-48"
      onChange={(event) => onChange({ ...inputs, text: event.target.value })}
    />
  );
}

/// Single-target entity picker for `links to` rules: an autocomplete over the
/// field's relation type, keeping at most one chip (a rule links to exactly
/// one target; add more rules — or an "any" group — for several).
function RelationTargetPicker({
  meta,
  values,
  disabled,
  onChange,
}: {
  meta: RuleFieldMeta;
  values: string[];
  disabled: boolean;
  onChange: (values: string[]) => void;
}) {
  const { t } = useLingui();
  const language = useTitleLanguage();
  const onRelationSearch = useRelationSearch();
  const [inputValue, setInputValue] = useState("");
  const [open, setOpen] = useState(false);
  const [options, setOptions] = useState<MultiValueComboboxOption[]>([]);
  const [loading, setLoading] = useState(false);
  const labels = useRef(new Map<string, string>());
  for (const option of options) {
    if (option.label) labels.current.set(option.value, option.label);
  }

  const relationType = meta.relationType;
  const { schedule: scheduleRelationSearch, cancel: cancelRelationSearch } =
    useDebouncedAbortableCallback(
      (signal, targetType: string, query: string) => {
        setLoading(true);
        onRelationSearch({ relationType: targetType, query, signal })
          .then((items) => {
            if (signal.aborted) return;
            setOptions(
              items
                .map((item) => ({ value: item.basename, label: entityTitle(item, language) }))
                .filter((option) => option.value),
            );
          })
          .catch((caught) => {
            if (!isAbortError(caught) && !signal.aborted) setOptions([]);
          })
          .finally(() => {
            if (!signal.aborted) setLoading(false);
          });
      },
      200,
    );

  useEffect(() => {
    if (!open || !relationType) {
      cancelRelationSearch();
      return;
    }
    scheduleRelationSearch(relationType, inputValue.trim());
    return cancelRelationSearch;
  }, [
    open,
    inputValue,
    relationType,
    onRelationSearch,
    language,
    scheduleRelationSearch,
    cancelRelationSearch,
  ]);

  return (
    <MultiValueCombobox
      values={values.slice(0, 1)}
      options={options}
      placeholder={t`Entity name`}
      ariaLabel={t`Link target`}
      disabled={disabled}
      allowCustomValue
      // Sized like the builder's other value pickers so the target sits on the
      // rule's row; the default is full width, which wraps it onto its own line.
      className="min-h-8 w-64 px-2 py-1 text-xs"
      inputValue={inputValue}
      onInputValueChange={setInputValue}
      loading={loading}
      formatChipLabel={(value) => labels.current.get(value) ?? value}
      onOpenChange={setOpen}
      onChange={(next) => onChange(next.slice(-1))}
    />
  );
}
