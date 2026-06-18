import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "react-router-dom";

import { errorMessage } from "@/api/client";
import { providerCatalogQuery, settingsConfigQuery, vaultTemplatesQuery } from "@/api/queries";
import { SettingsEditor } from "@/components/settings/settings-editor";
import { VaultSwitcher } from "@/components/settings/vault-switcher";
import { isDesktopRuntime } from "@/lib/desktop";

/**
 * Onboarding. On the desktop app with no open vault, choose/create one via the
 * native vault switcher. Otherwise (a vault is open but has no `.kizunashelf/
 * config.yaml` yet) configure its schema. The self-hosted web app has a single,
 * env-configured vault, so it lands straight on schema configuration.
 */
export function OnboardingPage() {
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const settings = useQuery(settingsConfigQuery());
  const providerCatalog = useQuery(providerCatalogQuery());
  const templates = useQuery(vaultTemplatesQuery());
  const desktop = isDesktopRuntime();

  const refresh = () => void queryClient.invalidateQueries();

  let body;
  if (desktop && settings.error) {
    // Desktop replies 503 (query error) until a vault is open.
    body = <VaultSwitcher onboarding onChanged={refresh} />;
  } else if (settings.isPending || providerCatalog.isPending || templates.isPending) {
    body = (
      <div className="rounded-md border p-8 text-center text-sm text-muted-foreground">Loading</div>
    );
  } else if (settings.error || providerCatalog.error || templates.error) {
    body = (
      <div className="rounded-md border border-destructive/40 bg-destructive/10 p-3 text-sm text-destructive">
        {errorMessage(settings.error ?? providerCatalog.error ?? templates.error)}
      </div>
    );
  } else {
    body = (
      <SettingsEditor
        appConfigPath={settings.data?.appConfigPath ?? ""}
        vaultConfigPath={settings.data?.vaultConfigPath}
        initialApp={settings.data?.app ?? { vaultRoot: "" }}
        initialVault={settings.data?.vault}
        providerCatalog={providerCatalog.data}
        templates={templates.data?.templates ?? []}
        onboarding
        onSaved={() => {
          refresh();
          navigate("/", { replace: true });
        }}
      />
    );
  }

  return (
    <main className="h-dvh overflow-auto overscroll-contain bg-background text-foreground">
      <div className="mx-auto flex w-full max-w-6xl flex-col gap-4 p-4">{body}</div>
    </main>
  );
}
