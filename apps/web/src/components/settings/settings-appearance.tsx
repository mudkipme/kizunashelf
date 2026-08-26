//! Per-device preferences: how the app looks and which language it speaks.
//!
//! These are deliberately *not* vault config — they belong to the screen you
//! are reading on, not to the library — so they save straight to local storage
//! and never take part in the settings form's dirty/save cycle.

import type { MessageDescriptor } from "@lingui/core";
import { msg } from "@lingui/core/macro";
import { Trans, useLingui } from "@lingui/react/macro";
import { useQuery } from "@tanstack/react-query";

import { languagesQuery } from "@/api/queries";
import { Field, SettingsSection } from "@/components/settings/settings-controls";
import { Select } from "@/components/ui/select";
import { isUiSupported, useLanguageStore, useUiLocale } from "@/lib/language";
import { useThemeStore, type ThemeMode } from "@/lib/theme";
import { titleLanguageLabel } from "@/lib/title-language";
import type { UserLanguage } from "@/types/api";

const modeLabels: Record<ThemeMode, MessageDescriptor> = {
  system: msg`System theme`,
  light: msg`Light theme`,
  dark: msg`Dark theme`,
};

export function AppearanceSettings() {
  const { t, i18n } = useLingui();
  const mode = useThemeStore((state) => state.mode);
  const setMode = useThemeStore((state) => state.setMode);
  const language = useLanguageStore((state) => state.language);
  const setLanguage = useLanguageStore((state) => state.setLanguage);
  const uiLocale = useUiLocale();
  const userLanguages = useQuery(languagesQuery()).data?.userLanguages ?? [];

  // An unusual browser language may not be in the supported set; keep it
  // selectable so the control never renders blank.
  const hasCurrent = userLanguages.some((item) => item.code === language);
  const options: UserLanguage[] = hasCurrent
    ? userLanguages
    : [
        { code: language, label: titleLanguageLabel(language, uiLocale), titleLanguage: language },
        ...userLanguages,
      ];

  return (
    <SettingsSection
      id="appearance"
      title={t`Appearance`}
      description={t`Stored on this device, not in the vault.`}
    >
      <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
        <Field label={t`Theme`}>
          <Select
            className="w-full"
            value={mode}
            onChange={(event) => setMode(event.target.value as ThemeMode)}
          >
            {(["system", "light", "dark"] as const).map((value) => (
              <option key={value} value={value}>
                {i18n._(modeLabels[value])}
              </option>
            ))}
          </Select>
        </Field>
        <Field label={t`Language`}>
          <Select
            className="w-full"
            value={language}
            onChange={(event) => setLanguage(event.target.value)}
          >
            {options.map((item) => (
              <option key={item.code} value={item.code}>
                {item.label}
              </option>
            ))}
          </Select>
        </Field>
      </div>
      {isUiSupported(language) ? null : (
        <p className="text-xs text-muted-foreground">
          <Trans>UI in English</Trans>
        </p>
      )}
    </SettingsSection>
  );
}
