//! The scalar value editors: the nested-object editor for map-shaped
//! frontmatter, the +/- stepper used by number and rating fields, and the date
//! picker.

import { Trans, useLingui } from "@lingui/react/macro";
import { CalendarIcon, MinusIcon, PlusIcon, XIcon } from "lucide-react";
import type { KeyboardEvent } from "react";
import { useState } from "react";

import { Button } from "@/components/ui/button";
import { Calendar } from "@/components/ui/calendar";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { Textarea } from "@/components/ui/textarea";
import { useIsoDateFormat } from "@/lib/locale";

import { formatDateValue, parseDateValue } from "./frontmatter-utils";
import type { FrontmatterValue } from "./metadata-types";

/** Structured values stay typed; incomplete JSON is confined to this dialog. */
export function StructuredValueInput({
  value,
  disabled,
  onChange,
  ariaLabel,
}: {
  value: FrontmatterValue;
  disabled: boolean;
  onChange: (value: FrontmatterValue) => void;
  ariaLabel: string;
}) {
  const { t } = useLingui();
  const [open, setOpen] = useState(false);
  const [text, setText] = useState("");
  const [error, setError] = useState<string>();

  function apply() {
    try {
      const parsed: FrontmatterValue = JSON.parse(text, (_key, item) => {
        if (typeof item === "number" && !Number.isFinite(item)) throw new Error();
        return item;
      });
      if (parsed === null || typeof parsed !== "object") throw new Error();
      onChange(parsed);
      setOpen(false);
    } catch {
      setError(t`Enter a valid JSON object or array.`);
    }
  }

  return (
    <>
      <pre
        className="max-h-40 overflow-auto rounded-md border bg-muted p-3 text-xs"
        aria-label={ariaLabel}
      >
        {JSON.stringify(value, null, 2)}
      </pre>
      <Button
        type="button"
        variant="outline"
        disabled={disabled}
        onClick={() => {
          setText(JSON.stringify(value, null, 2));
          setError(undefined);
          setOpen(true);
        }}
      >
        <Trans>Edit structured value</Trans>
      </Button>
      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{ariaLabel}</DialogTitle>
            <DialogDescription>
              <Trans>Edit as JSON to preserve nested values and their types.</Trans>
            </DialogDescription>
          </DialogHeader>
          <Textarea
            value={text}
            onChange={(event) => {
              setText(event.target.value);
              setError(undefined);
            }}
            aria-label={t`JSON value`}
            aria-invalid={Boolean(error)}
            className="min-h-48 font-mono"
            spellCheck={false}
          />
          {error ? (
            <p role="alert" className="text-sm text-destructive">
              {error}
            </p>
          ) : null}
          <DialogFooter>
            <Button type="button" variant="outline" onClick={() => setOpen(false)}>
              <Trans>Cancel</Trans>
            </Button>
            <Button type="button" onClick={apply} disabled={disabled}>
              <Trans>Apply</Trans>
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}

/// A +/- stepper over a non-negative integer held as a string: an editable
/// number field flanked by buttons that also step on Enter/Space.
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
