import { useEffect, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "react-router-dom";

import { errorMessage } from "@/api/client";
import { providerCatalogQuery, settingsConfigQuery } from "@/api/queries";
import { saveSettingsConfig } from "@/api/settings";
import { SettingsEditor } from "@/components/settings/settings-editor";
import { VaultPicker } from "@/components/settings/vault-picker";
import type { VaultConfig } from "@/types/config";

type Step = "vault" | "configure";

export function OnboardingPage() {
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const settings = useQuery(settingsConfigQuery());
  const providerCatalog = useQuery(providerCatalogQuery());

  const [step, setStep] = useState<Step>();
  const [vaultRoot, setVaultRoot] = useState<string>();
  const [vaultConfigPath, setVaultConfigPath] = useState<string>();
  const [initialVault, setInitialVault] = useState<VaultConfig>();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();

  // Pick the starting step once settings load: resume at configure when a vault
  // root is already chosen but the vault has no config yet, otherwise start the
  // picker.
  useEffect(() => {
    if (step || !settings.data) return;
    const data = settings.data;
    if (data.appExists && !data.vaultExists && data.app?.vaultRoot) {
      setVaultRoot(data.app.vaultRoot);
      setVaultConfigPath(data.vaultConfigPath);
      setInitialVault(data.vault);
      setStep("configure");
    } else {
      setStep("vault");
    }
  }, [settings.data, step]);

  async function continueWithVault(root: string) {
    if (!root) return;
    setBusy(true);
    setError(undefined);
    try {
      // Persist the chosen vault root only; never overwrite an existing,
      // possibly synced, vault config.
      const result = await saveSettingsConfig({ app: { vaultRoot: root }, vault: null });
      if (result.vaultExists) {
        // The vault already carries a config — skip configuration entirely.
        window.dispatchEvent(new Event("kizunashelf-config-saved"));
        await queryClient.invalidateQueries();
        navigate("/", { replace: true });
        return;
      }
      setVaultRoot(root);
      setVaultConfigPath(result.vaultConfigPath);
      setInitialVault(result.vault);
      setStep("configure");
    } catch (saveError) {
      setError(saveError instanceof Error ? saveError.message : "Failed to open vault");
    } finally {
      setBusy(false);
    }
  }

  const loading = settings.isPending || providerCatalog.isPending || !step;
  const queryError = settings.error ?? providerCatalog.error;

  return (
    <main className="h-dvh overflow-auto overscroll-contain bg-background text-foreground">
      <div className="mx-auto flex w-full max-w-6xl flex-col gap-4 p-4">
        {queryError ? (
          <div className="rounded-md border border-destructive/40 bg-destructive/10 p-3 text-sm text-destructive">
            {errorMessage(queryError)}
          </div>
        ) : null}
        {loading ? (
          <div className="rounded-md border p-8 text-center text-sm text-muted-foreground">Loading</div>
        ) : step === "vault" ? (
          <VaultPicker
            initialVaultRoot={vaultRoot}
            busy={busy}
            error={error}
            onContinue={continueWithVault}
          />
        ) : (
          <SettingsEditor
            appConfigPath={settings.data?.appConfigPath ?? ""}
            vaultConfigPath={vaultConfigPath}
            initialApp={{ vaultRoot: vaultRoot ?? "" }}
            initialVault={initialVault}
            providerCatalog={providerCatalog.data}
            onboarding
            onBack={() => {
              setError(undefined);
              setStep("vault");
            }}
            onSaved={() => {
              void queryClient.invalidateQueries();
              navigate("/", { replace: true });
            }}
          />
        )}
      </div>
    </main>
  );
}
