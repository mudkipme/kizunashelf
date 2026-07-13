import { useMemo } from "react";

import { parseIsoDateLocal } from "@/lib/date";
import { useUiLocale } from "@/lib/language";

/**
 * Locale-bound formatters so numbers and dates follow the app language rather
 * than the OS locale (a zh-Hant UI shouldn't show en-US dates because the OS
 * is English). Prefer these over bare `toLocaleString()`/`toLocaleDateString()`.
 */

export function useNumberFormat(): (value: number) => string {
  const locale = useUiLocale();
  return useMemo(() => {
    const format = new Intl.NumberFormat(locale);
    return (value: number) => format.format(value);
  }, [locale]);
}

export function useDateFormat(options: Intl.DateTimeFormatOptions): (date: Date) => string {
  const locale = useUiLocale();
  // Key the memo on the options' shape, not identity — callers pass inline objects.
  const optionsKey = JSON.stringify(options);
  return useMemo(() => {
    const format = new Intl.DateTimeFormat(locale, JSON.parse(optionsKey) as Intl.DateTimeFormatOptions);
    return (date: Date) => format.format(date);
  }, [locale, optionsKey]);
}

const DEFAULT_ISO_DATE_OPTIONS: Intl.DateTimeFormatOptions = {
  year: "numeric",
  month: "short",
  day: "numeric",
};

/**
 * Formats a stored `YYYY-MM-DD` string in the UI locale — the display counterpart
 * of the ISO storage form. Empty values render as "" and non-date/partial values
 * (a bare year, free text) pass through verbatim, so this is safe to point at any
 * date-field value. Storage stays ISO; only the displayed text is localized.
 */
export function useIsoDateFormat(
  options: Intl.DateTimeFormatOptions = DEFAULT_ISO_DATE_OPTIONS,
): (value: string | null | undefined) => string {
  const format = useDateFormat(options);
  return useMemo(() => {
    return (value: string | null | undefined) => {
      if (!value) return "";
      const date = parseIsoDateLocal(value);
      return date ? format(date) : value;
    };
  }, [format]);
}
