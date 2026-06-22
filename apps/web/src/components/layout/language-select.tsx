import { useQuery } from "@tanstack/react-query";

import { languagesQuery } from "@/api/queries";
import { Select } from "@/components/ui/select";
import { useLanguageStore } from "@/lib/language";
import { titleLanguageLabel } from "@/lib/title-language";

/**
 * Global display-language selector (app header). Drives entity-title resolution
 * everywhere and is the seam for future UI i18n. Defaults to the browser
 * language; the choice persists in the language store.
 */
export function LanguageSelect() {
  const language = useLanguageStore((state) => state.language);
  const setLanguage = useLanguageStore((state) => state.setLanguage);
  const languages = useQuery(languagesQuery()).data?.languages ?? [];
  // Keep the current value selectable even if it isn't in the supported set
  // (e.g. an unusual browser language), so the control never shows blank.
  const hasCurrent = languages.some((item) => item.code === language);

  return (
    <Select
      value={language}
      onChange={(event) => setLanguage(event.target.value)}
      aria-label="Display language"
      title="Display language"
      className="h-8 w-auto"
    >
      {hasCurrent ? null : <option value={language}>{titleLanguageLabel(language)}</option>}
      {languages.map((item) => (
        <option key={item.code} value={item.code}>
          {item.label}
        </option>
      ))}
    </Select>
  );
}
