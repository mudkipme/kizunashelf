import { useMemo } from "react";

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
