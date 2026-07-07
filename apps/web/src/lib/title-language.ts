const languageNamesCache = new Map<string, Intl.DisplayNames | undefined>();

function languageNames(displayLocale: string): Intl.DisplayNames | undefined {
  if (!languageNamesCache.has(displayLocale)) {
    languageNamesCache.set(
      displayLocale,
      typeof Intl.DisplayNames === "function"
        ? new Intl.DisplayNames([displayLocale, "en"], { type: "language" })
        : undefined,
    );
  }
  return languageNamesCache.get(displayLocale);
}

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

/** A language code's display name in `displayLocale` (the viewer's UI locale). */
export function titleLanguageLabel(titleLanguage: string, displayLocale = "en") {
  try {
    return languageNames(displayLocale)?.of(titleLanguage) ?? titleLanguage;
  } catch {
    // `of` throws on structurally invalid codes; free-form config values are legal here.
    return titleLanguage;
  }
}

/** The bare primary subtag of a language preference (`zh-Hans` → `zh`). Mirrors the core's `primary_language`. */
export function primaryLanguage(code: string): string {
  return code.trim().split(/[-_]/)[0]?.toLowerCase() ?? "";
}

export function isIso639TitleLanguage(value: string | null | undefined) {
  return /^[a-z]{2,3}$/.test(value ?? "");
}

export function iso639TitleLanguage(value: string | null | undefined) {
  return isIso639TitleLanguage(value) ? value ?? undefined : undefined;
}
