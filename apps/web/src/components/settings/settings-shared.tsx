//! Small pieces the settings editors share: the title-language options passed
//! down to nested field editors, and the subsection heading used throughout the
//! type/field dialogs.

import { useLingui } from "@lingui/react/macro";
import { createContext, type ReactNode, useCallback, useRef, useState } from "react";

import { Select } from "@/components/ui/select";
import { titleLanguageLabel } from "@/lib/title-language";
import type { Language } from "@/types/api";

import { UnknownValueOption } from "./settings-controls";

export type SettingsEditorHandle = {
  save: () => Promise<{ saved: boolean; error?: unknown }>;
  canSave: boolean;
};

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
    <Select value={value} onChange={(event) => onChange(event.target.value)} className="w-full">
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
      <h4 className="text-xs font-semibold text-muted-foreground uppercase">{title}</h4>
      {children}
    </div>
  );
}

/**
 * Reconciles incoming query snapshots with an editor's draft without mistaking
 * our own saves (before query invalidation settles) for on-disk changes, and
 * without letting stale query data overwrite the saved draft when `dirty` resets.
 */
export function useServerDraftSync<T>(onApplyServer: (value: T) => void) {
  const seededRef = useRef(false);
  const seenServerSignatureRef = useRef<string | undefined>(undefined);
  const savedSignaturesRef = useRef<string[]>([]);
  const loadedRevisionRef = useRef<string | undefined>(undefined);
  const [externalChange, setExternalChange] = useState(false);

  const applyBaseline = useCallback(
    (value: T, revision?: string | null) => {
      onApplyServer(value);
      loadedRevisionRef.current = revision ?? undefined;
      seededRef.current = true;
      setExternalChange(false);
    },
    [onApplyServer],
  );

  const acceptSave = useCallback(
    (value: T, signature: string, revision?: string | null) => {
      if (signature !== seenServerSignatureRef.current) {
        savedSignaturesRef.current.push(signature);
      }
      applyBaseline(value, revision);
    },
    [applyBaseline],
  );

  const reloadServer = useCallback(
    (value: T, revision?: string | null) => {
      savedSignaturesRef.current = [];
      applyBaseline(value, revision);
    },
    [applyBaseline],
  );

  const observeServer = useCallback(
    (value: T, signature: string, revision: string | null | undefined, dirty: boolean) => {
      if (seenServerSignatureRef.current === signature) return;
      seenServerSignatureRef.current = signature;
      const savedIndex = savedSignaturesRef.current.indexOf(signature);
      if (savedIndex !== -1) {
        savedSignaturesRef.current.splice(0, savedIndex + 1);
        return;
      }
      savedSignaturesRef.current = [];
      if (!seededRef.current || !dirty) {
        applyBaseline(value, revision);
        return;
      }
      setExternalChange(true);
    },
    [applyBaseline],
  );

  return {
    loadedRevisionRef,
    externalChange,
    setExternalChange,
    acceptSave,
    reloadServer,
    observeServer,
  };
}
