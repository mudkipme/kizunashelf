const languageNames =
  typeof Intl.DisplayNames === "function"
    ? new Intl.DisplayNames(["en"], { type: "language" })
    : undefined;

/**
 * The title to display for a viewer language: the language's title if present,
 * otherwise the core's language-agnostic fallback (`entity.title`, which is the
 * original-role title, then any title). Mirrors the core's `resolve_title`.
 */
export function entityTitle(
  entity: { title: string; titles: Record<string, string> },
  language: string,
) {
  return entity.titles[language] ?? entity.title;
}

export function titleLanguageLabel(titleLanguage: string) {
  return languageNames?.of(titleLanguage) ?? titleLanguage;
}

export function isIso639TitleLanguage(value: string | null | undefined) {
  return /^[a-z]{2,3}$/.test(value ?? "");
}

export function iso639TitleLanguage(value: string | null | undefined) {
  return isIso639TitleLanguage(value) ? value ?? undefined : undefined;
}
