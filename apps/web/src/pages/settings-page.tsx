import { useQuery } from "@tanstack/react-query";

import { errorMessage } from "@/api/client";
import { providerCatalogQuery, settingsConfigQuery } from "@/api/queries";
import { AppFrame } from "@/components/layout/app-frame";
import { SettingsEditor } from "@/components/settings/settings-editor";

export function SettingsPage() {
  const settings = useQuery(settingsConfigQuery());
  const providerCatalog = useQuery(providerCatalogQuery());
  const loading = settings.isPending || providerCatalog.isPending;
  const error = settings.error ?? providerCatalog.error;

  return (
    <AppFrame error={(error ? errorMessage(error) : undefined) ?? settings.data?.error}>
      <div className="mx-auto flex w-full max-w-7xl flex-col gap-4 p-4">
        {loading ? (
          <div className="rounded-md border p-8 text-center text-sm text-muted-foreground">Loading</div>
        ) : settings.data ? (
          <SettingsEditor
            appConfigPath={settings.data.appConfigPath}
            vaultConfigPath={settings.data.vaultConfigPath}
            initialApp={settings.data.app}
            initialVault={settings.data.vault}
            providerCatalog={providerCatalog.data}
          />
        ) : null}
      </div>
    </AppFrame>
  );
}
