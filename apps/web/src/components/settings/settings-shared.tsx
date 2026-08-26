//! Small pieces the settings editors share: the title-language options passed
//! down to nested field editors, and the subsection heading used throughout the
//! type/field dialogs.

import { createContext, type ReactNode } from "react";
import { useLingui } from "@lingui/react/macro";

import { Select } from "@/components/ui/select";
import { titleLanguageLabel } from "@/lib/title-language";
import type { Language } from "@/types/api";

import { UnknownValueOption } from "./settings-controls";

// Title-language options (from `GET /api/languages`) made available to the
// nested field editors without drilling through every intermediate component.
export const TitleLanguagesContext = createContext<Language[]>([]);

/// A title-language picker over the supported languages. `value` is an empty
/// string for "None"; an unrecognized configured code is preserved as its own
/// option so editing never silently drops it.
export function LanguageSelect({
  value,
  languages,
  onChange,
}: {
  value: string;
  languages: Language[];
  onChange: (value: string) => void;
}) {
  const { t, i18n } = useLingui();
  return (
    <Select
      value={value}
      onChange={(event) => onChange(event.target.value)}
      className="w-full"
    >
      <option value="">{t`None`}</option>
      {languages.map((language) => (
        <option key={language.code} value={language.code}>
          {titleLanguageLabel(language.code, i18n.locale)} ({language.code})
        </option>
      ))}
      <UnknownValueOption value={value} known={languages.map((language) => language.code)} />
    </Select>
  );
}

export function ConfigSubsection({ title, children }: { title: string; children: ReactNode }) {
  return (
    <div className="flex flex-col gap-2">
      <h4 className="text-xs font-semibold uppercase text-muted-foreground">{title}</h4>
      {children}
    </div>
  );
}
