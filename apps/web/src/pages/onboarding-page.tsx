import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "react-router-dom";

import { errorMessage } from "@/api/client";
import { providerCatalogQuery, settingsConfigQuery } from "@/api/queries";
import { SettingsEditor } from "@/components/settings/settings-editor";

export function OnboardingPage() {
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const settings = useQuery(settingsConfigQuery());
  const providerCatalog = useQuery(providerCatalogQuery());
  const loading = settings.isPending || providerCatalog.isPending;
  const error = settings.error ?? providerCatalog.error;

  return (
    <main className="h-dvh overflow-auto overscroll-contain bg-background text-foreground">
      <div className="mx-auto flex w-full max-w-6xl flex-col gap-4 p-4">
        {error ? (
          <div className="rounded-md border border-destructive/40 bg-destructive/10 p-3 text-sm text-destructive">
            {errorMessage(error)}
          </div>
        ) : null}
        {loading ? (
          <div className="rounded-md border p-8 text-center text-sm text-muted-foreground">Loading</div>
        ) : settings.data ? (
          <SettingsEditor
            configPath={settings.data.configPath}
            initialConfig={settings.data.config}
            providerCatalog={providerCatalog.data}
            onboarding={!settings.data.exists}
            onSaved={() => {
              void queryClient.invalidateQueries();
              navigate("/", { replace: true });
            }}
          />
        ) : null}
      </div>
    </main>
  );
}
