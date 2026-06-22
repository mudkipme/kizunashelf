import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";

import { errorMessage } from "@/api/client";
import { capabilitiesQuery, languagesQuery, providerCatalogQuery, settingsConfigQuery } from "@/api/queries";
import { AppFrame } from "@/components/layout/app-frame";
import { ProviderCredentials } from "@/components/settings/provider-credentials";
import { RawConfigEditor } from "@/components/settings/raw-config-editor";
import { SettingsEditor } from "@/components/settings/settings-editor";
import { VaultSwitcher } from "@/components/settings/vault-switcher";
import { Button } from "@/components/ui/button";
import { isDesktopRuntime } from "@/lib/desktop";

type EditorMode = "form" | "yaml";

export function SettingsPage() {
  const queryClient = useQueryClient();
  const settings = useQuery(settingsConfigQuery());
  const providerCatalog = useQuery(providerCatalogQuery());
  const capabilities = useQuery(capabilitiesQuery());
  const languages = useQuery(languagesQuery());
  const [mode, setMode] = useState<EditorMode>("form");
  const loading =
    settings.isPending || providerCatalog.isPending || capabilities.isPending || languages.isPending;
  const error = settings.error ?? providerCatalog.error ?? capabilities.error ?? languages.error;
  const desktop = isDesktopRuntime();
  const settingsWritable = capabilities.data?.settingsWritable !== false;

  return (
    <AppFrame error={(error ? errorMessage(error) : undefined) ?? settings.data?.error ?? undefined}>
      <div className="mx-auto flex w-full max-w-7xl flex-col gap-4 p-4">
        {/* Desktop manages vaults + credentials natively (multi-vault, OS keychain). */}
        {desktop ? (
          <>
            <VaultSwitcher onChanged={() => void queryClient.invalidateQueries()} />
            <ProviderCredentials />
          </>
        ) : null}

        {/* Switch between the structured schema form and raw YAML editing. */}
        <div className="flex w-fit gap-1 rounded-md border p-1">
          <Button
            type="button"
            size="sm"
            variant={mode === "form" ? "secondary" : "ghost"}
            onClick={() => setMode("form")}
          >
            Form
          </Button>
          <Button
            type="button"
            size="sm"
            variant={mode === "yaml" ? "secondary" : "ghost"}
            onClick={() => setMode("yaml")}
          >
            YAML
          </Button>
        </div>

        {mode === "yaml" ? (
          <RawConfigEditor settingsWritable={settingsWritable} />
        ) : loading ? (
          <div className="rounded-md border p-8 text-center text-sm text-muted-foreground">Loading</div>
        ) : settings.data ? (
          <SettingsEditor
            vaultConfigPath={settings.data.vaultConfigPath ?? undefined}
            initialApp={settings.data.app}
            initialVault={settings.data.vault}
            providerCatalog={providerCatalog.data}
            languages={languages.data?.languages ?? []}
            settingsWritable={settingsWritable}
          />
        ) : null}
      </div>
    </AppFrame>
  );
}
