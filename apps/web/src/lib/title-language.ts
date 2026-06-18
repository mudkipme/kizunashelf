import { defaultTitleOptionId } from "@/lib/constants";
import type { EntitySummary } from "@/types/api";

const languageNames =
  typeof Intl.DisplayNames === "function"
    ? new Intl.DisplayNames(["en"], { type: "language" })
    : undefined;

export function entityTitle(entity: EntitySummary, titleLanguage: string) {
  if (titleLanguage === defaultTitleOptionId) return entity.title;
  return entity.titles[titleLanguage] ?? entity.title;
}

export function titleLanguageLabel(titleLanguage: string) {
  if (titleLanguage === defaultTitleOptionId) return "Default title";
  return languageNames?.of(titleLanguage) ?? titleLanguage;
}

export function isIso639TitleLanguage(value: string | null | undefined) {
  return /^[a-z]{2,3}$/.test(value ?? "");
}

export function iso639TitleLanguage(value: string | null | undefined) {
  return isIso639TitleLanguage(value) ? value ?? undefined : undefined;
}
