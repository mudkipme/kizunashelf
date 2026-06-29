import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "react-router-dom";

import { errorMessage } from "@/api/client";
import { languagesQuery, providerCatalogQuery, settingsConfigQuery, vaultTemplatesQuery } from "@/api/queries";
import { SettingsEditor } from "@/components/settings/settings-editor";
import { VaultSwitcher } from "@/components/settings/vault-switcher";
import { Alert } from "@/components/ui/alert";
import { Placeholder } from "@/components/ui/placeholder";
import { isDesktopRuntime } from "@/lib/desktop";

/**
 * Onboarding. On the desktop app with no open vault, choose/create one via the
 * native vault switcher. Otherwise (a vault is open but has no `KizunaShelf/
 * config.yaml` yet) configure its schema. The self-hosted web app has a single,
 * env-configured vault, so it lands straight on schema configuration.
 */
export function OnboardingPage() {
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const settings = useQuery(settingsConfigQuery());
  const providerCatalog = useQuery(providerCatalogQuery());
  const templates = useQuery(vaultTemplatesQuery());
  const languages = useQuery(languagesQuery());
  const desktop = isDesktopRuntime();

  const refresh = () => void queryClient.invalidateQueries();

  let body;
  if (desktop && settings.error) {
    // Desktop replies 503 (query error) until a vault is open.
    body = <VaultSwitcher onboarding onChanged={refresh} />;
  } else if (
    settings.isPending ||
    providerCatalog.isPending ||
    templates.isPending ||
    languages.isPending
  ) {
    body = (
      <Placeholder>Loading</Placeholder>
    );
  } else if (settings.error || providerCatalog.error || templates.error || languages.error) {
    body = (
      <Alert>
        {errorMessage(settings.error ?? providerCatalog.error ?? templates.error ?? languages.error)}
      </Alert>
    );
  } else {
    body = (
      <SettingsEditor
        vaultConfigPath={settings.data?.vaultConfigPath ?? undefined}
        initialApp={settings.data?.app ?? { vaultRoot: "" }}
        initialVault={settings.data?.vault}
        providerCatalog={providerCatalog.data}
        templates={templates.data?.templates ?? []}
        languages={languages.data?.languages ?? []}
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
