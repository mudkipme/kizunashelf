import { useMemo, useState } from "react";
import { Trans, useLingui } from "@lingui/react/macro";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "react-router-dom";
import { toast } from "sonner";

import { errorMessage } from "@/api/client";
import { languagesQuery, providerCatalogQuery, settingsConfigQuery, typePresetsQuery } from "@/api/queries";
import { resolveTypePresets, saveSettingsConfig } from "@/api/settings";
import { defaultLanguage, PresetGallery } from "@/components/settings/preset-picker";
import { cleanVaultConfig, defaultDailyNotes, defaultVaultConfig } from "@/components/settings/settings-model";
import { SettingsEditor } from "@/components/settings/settings-editor";
import { VaultSwitcher } from "@/components/settings/vault-switcher";
import { Alert } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Placeholder } from "@/components/ui/placeholder";
import { Select } from "@/components/ui/select";
import { isDesktopRuntime } from "@/lib/desktop";
import type { VaultConfig } from "@/types/api";

/**
 * Onboarding. On desktop with no open vault, choose/create one via the native
 * vault switcher. Otherwise (a vault is open but has no `KizunaShelf/config.yaml`
 * yet) run the type-picker wizard: choose what to track, pick a title language,
 * and the vault is created from those built-in types. An "Advanced" escape hatch
 * opens the full schema editor for power users.
 */
export function OnboardingPage() {
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const settings = useQuery(settingsConfigQuery());
  const providerCatalog = useQuery(providerCatalogQuery());
  const presets = useQuery(typePresetsQuery());
  const languages = useQuery(languagesQuery());
  const desktop = isDesktopRuntime();

  // `null` = the wizard; a config = the advanced full-schema editor seeded with it.
  const [advancedSeed, setAdvancedSeed] = useState<VaultConfig | null>(null);

  const refresh = () => void queryClient.invalidateQueries();
  const landHome = () => {
    refresh();
    navigate("/", { replace: true });
  };

  let body;
  if (desktop && settings.error) {
    // Desktop replies 503 (query error) until a vault is open.
    body = <VaultSwitcher onboarding onChanged={refresh} />;
  } else if (settings.isPending || providerCatalog.isPending || presets.isPending || languages.isPending) {
    body = (
      <Placeholder>
        <Trans>Loading…</Trans>
      </Placeholder>
    );
  } else if (settings.error || providerCatalog.error || presets.error || languages.error) {
    body = (
      <Alert>
        {errorMessage(settings.error ?? providerCatalog.error ?? presets.error ?? languages.error)}
      </Alert>
    );
  } else if (advancedSeed) {
    body = (
      <SettingsEditor
        vaultConfigPath={settings.data?.vaultConfigPath ?? undefined}
        initialApp={settings.data?.app ?? { vaultRoot: "" }}
        initialVault={advancedSeed}
        providerCatalog={providerCatalog.data}
        languages={languages.data?.languages ?? []}
        onboarding
        onBack={() => setAdvancedSeed(null)}
        onSaved={landHome}
      />
    );
  } else {
    body = (
      <OnboardingWizard
        languages={languages.data?.languages ?? []}
        presetCatalog={presets.data}
        providerCatalog={providerCatalog.data}
        onAdvanced={setAdvancedSeed}
        onCreated={landHome}
      />
    );
  }

  return (
    <main className="h-dvh overflow-auto overscroll-contain bg-background text-foreground">
      <div className="mx-auto flex w-full max-w-5xl flex-col gap-4 p-4">{body}</div>
    </main>
  );
}

function OnboardingWizard({
  languages,
  presetCatalog,
  providerCatalog,
  onAdvanced,
  onCreated,
}: {
  languages: { code: string; label: string }[];
  presetCatalog: Parameters<typeof PresetGallery>[0]["catalog"];
  providerCatalog: Parameters<typeof SettingsEditor>[0]["providerCatalog"];
  onAdvanced: (seed: VaultConfig) => void;
  onCreated: () => void;
}) {
  const { t } = useLingui();
  const [selected, setSelected] = useState<Set<string>>(() => new Set());
  const [titleLanguage, setTitleLanguage] = useState(() => defaultLanguage(languages));
  const [creating, setCreating] = useState(false);
  const noExisting = useMemo(() => new Set<string>(), []);

  function toggle(id: string) {
    setSelected((current) => {
      const next = new Set(current);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }

  // Build the vault config from the current selection: taxonomy/asset defaults,
  // daily notes on, the resolved types, and a home section per type.
  async function buildConfig(): Promise<VaultConfig> {
    const base = defaultVaultConfig();
    if (selected.size === 0) {
      return { ...base, types: [], home: { title: "Home", sections: [] } };
    }
    const resolved = await resolveTypePresets({
      currentTypes: [],
      presetIds: [...selected],
      titleLanguage,
    });
    return {
      taxonomyRoot: base.taxonomyRoot,
      assetRoot: base.assetRoot,
      dailyNotes: defaultDailyNotes(),
      home: { title: "Home", sections: resolved.homeSections ?? [] },
      types: resolved.types ?? [],
    };
  }

  async function create() {
    setCreating(true);
    try {
      const vault = await buildConfig();
      await saveSettingsConfig(cleanVaultConfig(vault, providerCatalog));
      toast.success(t`Vault created`);
      window.dispatchEvent(new Event("kizunashelf-config-saved"));
      onCreated();
    } catch (error) {
      toast.error(errorMessage(error));
    } finally {
      setCreating(false);
    }
  }

  async function openAdvanced() {
    try {
      onAdvanced(selected.size === 0 ? defaultVaultConfig() : await buildConfig());
    } catch (error) {
      toast.error(errorMessage(error));
    }
  }

  return (
    <div className="flex flex-col gap-5">
      <header className="flex flex-col gap-1">
        <h1 className="text-xl font-semibold">
          <Trans>What do you want to track?</Trans>
        </h1>
        <p className="text-sm text-muted-foreground">
          <Trans>
            Pick a few — you can add more anytime. Your library is plain Markdown files in a folder
            you own, readable even without this app.
          </Trans>
        </p>
      </header>

      <PresetGallery catalog={presetCatalog} selected={selected} existingIds={noExisting} onToggle={toggle} />

      <div className="sticky bottom-0 flex flex-col gap-3 border-t bg-background/95 py-3 backdrop-blur sm:flex-row sm:items-center sm:justify-between">
        <div className="flex items-center gap-3">
          <label className="flex items-center gap-2 text-sm">
            <span className="text-muted-foreground">
              <Trans>Title language</Trans>
            </span>
            <Select
              value={titleLanguage}
              onChange={(event) => setTitleLanguage(event.target.value)}
              className="h-9 text-base md:text-sm"
            >
              {languages.map((language) => (
                <option key={language.code} value={language.code}>
                  {language.label}
                </option>
              ))}
            </Select>
          </label>
          <Button type="button" variant="ghost" size="sm" onClick={openAdvanced}>
            <Trans>Advanced: edit full schema</Trans>
          </Button>
        </div>
        <Button type="button" onClick={create} disabled={creating || selected.size === 0}>
          {creating
            ? t`Creating…`
            : selected.size > 0
              ? t`Create vault (${selected.size})`
              : t`Create vault`}
        </Button>
      </div>
    </div>
  );
}
