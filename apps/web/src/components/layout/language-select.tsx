import { useQuery } from "@tanstack/react-query";
import { LanguagesIcon } from "lucide-react";

import { languagesQuery } from "@/api/queries";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { useLanguageStore } from "@/lib/language";
import { titleLanguageLabel } from "@/lib/title-language";

/**
 * Global display-language selector (app header). Drives entity-title resolution
 * everywhere and is the seam for future UI i18n. Defaults to the browser
 * language; the choice persists in the language store. Rendered as a compact
 * icon-button dropdown so it stays consistent across viewports.
 */
export function LanguageSelect() {
  const language = useLanguageStore((state) => state.language);
  const setLanguage = useLanguageStore((state) => state.setLanguage);
  const languages = useQuery(languagesQuery()).data?.languages ?? [];
  // Keep the current value selectable even if it isn't in the supported set
  // (e.g. an unusual browser language), so the control never shows blank.
  const hasCurrent = languages.some((item) => item.code === language);
  const options = hasCurrent
    ? languages
    : [{ code: language, label: titleLanguageLabel(language) }, ...languages];

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          type="button"
          variant="ghost"
          size="icon"
          aria-label="Display language"
          title="Display language"
        >
          <LanguagesIcon />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="max-h-72">
        <DropdownMenuRadioGroup value={language} onValueChange={setLanguage}>
          {options.map((item) => (
            <DropdownMenuRadioItem key={item.code} value={item.code}>
              {item.label}
            </DropdownMenuRadioItem>
          ))}
        </DropdownMenuRadioGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
