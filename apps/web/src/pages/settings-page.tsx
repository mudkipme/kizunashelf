import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";

import { errorMessage } from "@/api/client";
import { capabilitiesQuery, languagesQuery, providerCatalogQuery, settingsConfigQuery } from "@/api/queries";
import { AppFrame } from "@/components/layout/app-frame";
import { ProviderCredentials } from "@/components/settings/provider-credentials";
import { RawConfigEditor } from "@/components/settings/raw-config-editor";
import { SettingsEditor } from "@/components/settings/settings-editor";
import { VaultSwitcher } from "@/components/settings/vault-switcher";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Button } from "@/components/ui/button";
import { useUnsavedChangesWarning } from "@/hooks/use-unsaved-changes-warning";
import { isDesktopRuntime } from "@/lib/desktop";

type EditorMode = "form" | "yaml";

export function SettingsPage() {
  const queryClient = useQueryClient();
  const settings = useQuery(settingsConfigQuery());
  const providerCatalog = useQuery(providerCatalogQuery());
  const capabilities = useQuery(capabilitiesQuery());
  const languages = useQuery(languagesQuery());
  const [mode, setMode] = useState<EditorMode>("form");
  // The active editor reports unsaved edits here; `pendingMode` holds a requested
  // switch that's waiting on the discard confirmation.
  const [dirty, setDirty] = useState(false);
  const [pendingMode, setPendingMode] = useState<EditorMode | null>(null);
  const loading =
    settings.isPending || providerCatalog.isPending || capabilities.isPending || languages.isPending;
  const error = settings.error ?? providerCatalog.error ?? capabilities.error ?? languages.error;
  const desktop = isDesktopRuntime();
  const settingsWritable = capabilities.data?.settingsWritable !== false;

  // Warn on tab close/reload while edits are unsaved.
  useUnsavedChangesWarning(dirty);

  // Switching editors discards the active draft, so confirm first when dirty.
  function requestMode(next: EditorMode) {
    if (next === mode) return;
    if (dirty) {
      setPendingMode(next);
      return;
    }
    setMode(next);
  }

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
            onClick={() => requestMode("form")}
          >
            Form
          </Button>
          <Button
            type="button"
            size="sm"
            variant={mode === "yaml" ? "secondary" : "ghost"}
            onClick={() => requestMode("yaml")}
          >
            YAML
          </Button>
        </div>

        {mode === "yaml" ? (
          <RawConfigEditor settingsWritable={settingsWritable} onDirtyChange={setDirty} />
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
            onDirtyChange={setDirty}
          />
        ) : null}
      </div>

      <AlertDialog
        open={pendingMode !== null}
        onOpenChange={(open) => {
          if (!open) setPendingMode(null);
        }}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Discard unsaved changes?</AlertDialogTitle>
            <AlertDialogDescription>
              You have unsaved changes in the {mode === "form" ? "form" : "YAML"} editor. Switching
              editors will discard them.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Keep editing</AlertDialogCancel>
            <AlertDialogAction
              onClick={() => {
                if (pendingMode) setMode(pendingMode);
                setPendingMode(null);
              }}
            >
              Discard
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </AppFrame>
  );
}
