import { useQuery, useQueryClient } from "@tanstack/react-query";

import { errorMessage } from "@/api/client";
import { capabilitiesQuery, languagesQuery, providerCatalogQuery, settingsConfigQuery } from "@/api/queries";
import { AppFrame } from "@/components/layout/app-frame";
import { ProviderCredentials } from "@/components/settings/provider-credentials";
import { SettingsEditor } from "@/components/settings/settings-editor";
import { VaultSwitcher } from "@/components/settings/vault-switcher";
import { isDesktopRuntime } from "@/lib/desktop";

export function SettingsPage() {
  const queryClient = useQueryClient();
  const settings = useQuery(settingsConfigQuery());
  const providerCatalog = useQuery(providerCatalogQuery());
  const capabilities = useQuery(capabilitiesQuery());
  const languages = useQuery(languagesQuery());
  const loading =
    settings.isPending || providerCatalog.isPending || capabilities.isPending || languages.isPending;
  const error = settings.error ?? providerCatalog.error ?? capabilities.error ?? languages.error;
  const desktop = isDesktopRuntime();

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
        {loading ? (
          <div className="rounded-md border p-8 text-center text-sm text-muted-foreground">Loading</div>
        ) : settings.data ? (
          <SettingsEditor
            vaultConfigPath={settings.data.vaultConfigPath ?? undefined}
            initialApp={settings.data.app}
            initialVault={settings.data.vault}
            providerCatalog={providerCatalog.data}
            languages={languages.data?.languages ?? []}
            settingsWritable={capabilities.data?.settingsWritable !== false}
          />
        ) : null}
      </div>
    </AppFrame>
  );
}
