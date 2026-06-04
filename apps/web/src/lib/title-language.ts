import { defaultTitleLanguage } from "@/lib/constants";
import type { EntitySummary } from "@/types/api";

const labels: Record<string, string> = {
  [defaultTitleLanguage]: "Default title",
  primary: "Primary",
  original: "Original",
  zh: "Chinese",
  "zh-Hans": "Chinese (Simplified)",
  "zh-Hant": "Chinese (Traditional)",
  ja: "Japanese",
  jp: "Japanese",
  en: "English",
  ko: "Korean",
};

export function entityTitle(entity: EntitySummary, titleLanguage: string) {
  if (titleLanguage === defaultTitleLanguage) return entity.title;
  return entity.titles[titleLanguage] ?? entity.title;
}

export function titleLanguageLabel(titleLanguage: string) {
  return labels[titleLanguage] ?? titleLanguage;
}
