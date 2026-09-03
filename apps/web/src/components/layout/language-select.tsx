import { Trans, useLingui } from "@lingui/react/macro";
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
import { isUiSupported, useLanguageStore, useUiLocale } from "@/lib/language";
import { titleLanguageLabel } from "@/lib/title-language";
import type { UserLanguage } from "@/types/api";

/**
 * Global language selector (app header): the single preference everything
 * derives from — UI locale (with English fallback), title resolution, and the
 * language sent to providers. Options come from the core's user-language list
 * (endonym labels; `zh` split into 简体/繁體). Rendered as a compact
 * icon-button dropdown so it stays consistent across viewports.
 */
export function LanguageSelect() {
  const { t } = useLingui();
  const language = useLanguageStore((state) => state.language);
  const setLanguage = useLanguageStore((state) => state.setLanguage);
  const uiLocale = useUiLocale();
  const userLanguages = useQuery(languagesQuery()).data?.userLanguages ?? [];
  // Keep the current value selectable even if it isn't in the supported set
  // (e.g. an unusual browser language), so the control never shows blank.
  const hasCurrent = userLanguages.some((item) => item.code === language);
  const options: UserLanguage[] = hasCurrent
    ? userLanguages
    : [
        {
          code: language,
          label: titleLanguageLabel(language, uiLocale),
          titleLanguage: language,
        },
        ...userLanguages,
      ];

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          type="button"
          variant="ghost"
          size="icon"
          aria-label={t`Language`}
          title={t`Language`}
        >
          <LanguagesIcon />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="max-h-72">
        <DropdownMenuRadioGroup value={language} onValueChange={setLanguage}>
          {options.map((item) => (
            <DropdownMenuRadioItem key={item.code} value={item.code}>
              <span className="flex items-baseline gap-2">
                {item.label}
                {!isUiSupported(item.code) && (
                  <span className="text-xs text-muted-foreground">
                    <Trans>UI in English</Trans>
                  </span>
                )}
              </span>
            </DropdownMenuRadioItem>
          ))}
        </DropdownMenuRadioGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
