//! The scalar value editors: the nested-object editor for map-shaped
//! frontmatter, the +/- stepper used by progress and rating fields, and the date
//! picker.

import type { KeyboardEvent } from "react";
import { useState } from "react";
import { Trans, useLingui } from "@lingui/react/macro";
import { CalendarIcon, MinusIcon, PlusIcon, Trash2Icon, XIcon } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Calendar } from "@/components/ui/calendar";
import { Input } from "@/components/ui/input";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { useIsoDateFormat } from "@/lib/locale";

import {
  formatDateValue,
  isFrontmatterObject,
  isScalarFrontmatterValue,
  parseDateValue,
  parseScalarValue,
  valueToText,
} from "./frontmatter-utils";
import type { FrontmatterObject, FrontmatterValue } from "./metadata-types";

export function ObjectValueInput({
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
          className="border-input bg-muted text-foreground flex h-(--control-height) min-w-0 items-center justify-center rounded-md border px-3 text-center text-sm tabular-nums"
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

export function DatePickerInput({
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
  const { t } = useLingui();
  const formatDate = useIsoDateFormat();
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
            <span className="truncate">{value ? formatDate(value) : t`Pick a date`}</span>
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
