import { Trans } from "@lingui/react/macro";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useRef, useState } from "react";
import { useBlocker } from "react-router-dom";

import { errorMessage } from "@/api/client";
import { languagesQuery, providerCatalogQuery, settingsConfigQuery } from "@/api/queries";
import { AppFrame } from "@/components/layout/app-frame";
import { PageContainer } from "@/components/layout/page-container";
import { SaveFailure } from "@/components/save-failure";
import { ProviderCredentials } from "@/components/settings/provider-credentials";
import { RawConfigEditor } from "@/components/settings/raw-config-editor";
import { SettingsEditor } from "@/components/settings/settings-editor";
import type { SettingsEditorHandle } from "@/components/settings/settings-shared";
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
import { Placeholder } from "@/components/ui/placeholder";
import { useUnsavedChangesWarning } from "@/hooks/use-unsaved-changes-warning";
import { useCapabilities } from "@/lib/capabilities";
import { isDesktopRuntime } from "@/lib/desktop";

type EditorMode = "form" | "yaml";

export function SettingsPage() {
  const queryClient = useQueryClient();
  const settings = useQuery(settingsConfigQuery());
  const providerCatalog = useQuery(providerCatalogQuery());
  const capabilities = useCapabilities();
  const languages = useQuery(languagesQuery());
  const [mode, setMode] = useState<EditorMode>("form");
  // The active editor reports unsaved edits here; `pendingMode` holds a requested
  // switch that's waiting on the discard confirmation.
  const [dirty, setDirty] = useState(false);
  const [saving, setSaving] = useState(false);
  const [leaving, setLeaving] = useState(false);
  const [saveError, setSaveError] = useState<unknown>();
  const editor = useRef<SettingsEditorHandle>(null);
  const busy = saving || leaving;
  const [pendingMode, setPendingMode] = useState<EditorMode | null>(null);
  const loading =
    settings.isPending ||
    providerCatalog.isPending ||
    capabilities.isPending ||
    languages.isPending;
  const error = settings.error ?? providerCatalog.error ?? capabilities.error ?? languages.error;
  const desktop = isDesktopRuntime();
  const settingsWritable = capabilities.settingsWritable;

  useUnsavedChangesWarning(dirty || busy);
  const blocker = useBlocker(
    ({ currentLocation, nextLocation }) =>
      (dirty || busy) &&
      (currentLocation.pathname !== nextLocation.pathname ||
        currentLocation.search !== nextLocation.search),
  );
  const blockerRef = useRef(blocker);
  blockerRef.current = blocker;

  async function saveAndLeave() {
    if (busy || !editor.current?.canSave) return;
    setLeaving(true);
    setSaveError(undefined);
    try {
      const result = await editor.current.save();
      if (result.saved && blockerRef.current.state === "blocked") blockerRef.current.proceed();
      else setSaveError(result.error);
    } finally {
      setLeaving(false);
    }
  }

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
    <AppFrame
      error={(error ? errorMessage(error) : undefined) ?? settings.data?.error ?? undefined}
    >
      <PageContainer width="wide">
        {/* Desktop manages vaults + credentials natively (multi-vault, OS keychain). */}
        {desktop ? (
          <>
            <VaultSwitcher
              hasUnsavedChanges={dirty || busy}
              onChanged={() => void queryClient.invalidateQueries()}
            />
            <ProviderCredentials providers={providerCatalog.data?.providers ?? []} />
          </>
        ) : null}

        {/* Switch between the structured schema form and raw YAML editing. */}
        <div className="flex w-fit gap-1 rounded-md border p-1">
          <Button
            type="button"
            size="sm"
            variant={mode === "form" ? "secondary" : "ghost"}
            disabled={busy}
            onClick={() => requestMode("form")}
          >
            <Trans>Form</Trans>
          </Button>
          <Button
            type="button"
            size="sm"
            variant={mode === "yaml" ? "secondary" : "ghost"}
            disabled={busy}
            onClick={() => requestMode("yaml")}
          >
            <Trans>YAML</Trans>
          </Button>
        </div>

        {mode === "yaml" ? (
          <RawConfigEditor
            ref={editor}
            onSavingChange={setSaving}
            settingsWritable={settingsWritable}
            onDirtyChange={setDirty}
          />
        ) : loading ? (
          <Placeholder>
            <Trans>Loading…</Trans>
          </Placeholder>
        ) : settings.data ? (
          <SettingsEditor
            ref={editor}
            onSavingChange={setSaving}
            vaultConfigPath={settings.data.vaultConfigPath ?? undefined}
            initialApp={settings.data.app}
            initialVault={settings.data.vault}
            initialRevision={settings.data.revision}
            providerCatalog={providerCatalog.data}
            languages={languages.data?.languages ?? []}
            settingsWritable={settingsWritable}
            onDirtyChange={setDirty}
          />
        ) : null}
      </PageContainer>

      <AlertDialog
        open={blocker.state === "blocked"}
        onOpenChange={(open) => {
          if (!open && !busy && blocker.state === "blocked") {
            blocker.reset();
            setSaveError(undefined);
          }
        }}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>
              <Trans>Unsaved changes</Trans>
            </AlertDialogTitle>
            <AlertDialogDescription>
              <Trans>Save your edits before leaving, or discard them.</Trans>
            </AlertDialogDescription>
          </AlertDialogHeader>
          {saveError ? <SaveFailure error={saveError} /> : null}
          <AlertDialogFooter>
            <AlertDialogCancel disabled={busy}>
              <Trans>Keep editing</Trans>
            </AlertDialogCancel>
            <Button
              variant="outline"
              disabled={busy}
              onClick={() => {
                if (blocker.state === "blocked") blocker.proceed();
              }}
            >
              <Trans>Discard</Trans>
            </Button>
            <Button
              disabled={busy || !settingsWritable || !editor.current?.canSave}
              onClick={() => void saveAndLeave()}
            >
              {busy ? <Trans>Saving…</Trans> : <Trans>Save and leave</Trans>}
            </Button>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      <AlertDialog
        open={pendingMode !== null}
        onOpenChange={(open) => {
          if (!open) setPendingMode(null);
        }}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>
              <Trans>Discard unsaved changes?</Trans>
            </AlertDialogTitle>
            <AlertDialogDescription>
              {mode === "form" ? (
                <Trans>
                  You have unsaved changes in the form editor. Switching editors will discard them.
                </Trans>
              ) : (
                <Trans>
                  You have unsaved changes in the YAML editor. Switching editors will discard them.
                </Trans>
              )}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>
              <Trans>Keep editing</Trans>
            </AlertDialogCancel>
            <AlertDialogAction
              onClick={() => {
                if (pendingMode) setMode(pendingMode);
                setPendingMode(null);
              }}
            >
              <Trans>Discard</Trans>
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </AppFrame>
  );
}
