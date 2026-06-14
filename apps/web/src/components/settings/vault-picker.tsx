import { useState } from "react";
import { FolderOpenIcon, FolderPlusIcon } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { isDesktopRuntime, selectDirectory } from "@/lib/desktop";

import { Field, PathField, SettingsSection } from "./settings-controls";
import { joinPath } from "./settings-model";

type VaultPickerProps = {
  /** Pre-fills the picker (e.g. a vault root chosen earlier in the session). */
  initialVaultRoot?: string;
  busy?: boolean;
  error?: string;
  /** Receives the absolute vault root to open or create. */
  onContinue: (vaultRoot: string) => void;
};

/**
 * First onboarding step: choose where the vault lives. Desktop mirrors Obsidian
 * with separate Open / Create actions (the native picker can only choose
 * existing folders); web uses a single path field and creates the folder on the
 * server if it does not exist.
 */
export function VaultPicker({ initialVaultRoot, busy = false, error, onContinue }: VaultPickerProps) {
  return (
    <div className="flex flex-col gap-4">
      <header className="min-w-0">
        <h1 className="text-base font-semibold">Set Up KizunaShelf</h1>
        <p className="mt-1 text-xs text-muted-foreground">
          Open an existing vault or create a new one to get started.
        </p>
      </header>

      {error ? (
        <div className="rounded-md border border-destructive/40 bg-destructive/10 p-3 text-sm text-destructive">
          {error}
        </div>
      ) : null}

      {isDesktopRuntime() ? (
        <DesktopVaultPicker busy={busy} onContinue={onContinue} />
      ) : (
        <WebVaultPicker initialVaultRoot={initialVaultRoot} busy={busy} onContinue={onContinue} />
      )}
    </div>
  );
}

function WebVaultPicker({
  initialVaultRoot,
  busy,
  onContinue,
}: {
  initialVaultRoot?: string;
  busy: boolean;
  onContinue: (vaultRoot: string) => void;
}) {
  const [vaultRoot, setVaultRoot] = useState(initialVaultRoot ?? "");
  const trimmed = vaultRoot.trim();

  return (
    <SettingsSection
      title="Vault folder"
      description="Absolute path to the vault. The folder is created if it does not exist yet."
    >
      <div className="flex flex-col gap-3">
        <PathField label="Vault root" value={vaultRoot} onChange={setVaultRoot} absolute />
        <div className="flex justify-end">
          <Button type="button" onClick={() => onContinue(trimmed)} disabled={busy || !trimmed}>
            {busy ? "Opening" : "Continue"}
          </Button>
        </div>
      </div>
    </SettingsSection>
  );
}

function DesktopVaultPicker({
  busy,
  onContinue,
}: {
  busy: boolean;
  onContinue: (vaultRoot: string) => void;
}) {
  const [parent, setParent] = useState("");
  const [name, setName] = useState("");
  const newVaultRoot = parent && name.trim() ? joinPath(parent, name.trim()) : "";

  async function openExisting() {
    const selected = await selectDirectory().catch(() => undefined);
    if (selected) onContinue(selected);
  }

  async function chooseParent() {
    const selected = await selectDirectory(parent || undefined).catch(() => undefined);
    if (selected) setParent(selected);
  }

  return (
    <div className="grid gap-3 lg:grid-cols-2">
      <SettingsSection title="Open existing vault" description="Choose a folder that already holds your notes.">
        <div className="flex justify-start">
          <Button type="button" variant="outline" onClick={openExisting} disabled={busy}>
            <FolderOpenIcon data-icon="inline-start" />
            Open folder
          </Button>
        </div>
      </SettingsSection>

      <SettingsSection title="Create new vault" description="Pick a parent folder and name the new vault folder.">
        <div className="flex flex-col gap-3">
          <Field label="Parent folder">
            <div className="flex items-center gap-2">
              <Input value={parent} readOnly placeholder="No folder selected" />
              <Button type="button" variant="outline" size="icon" onClick={chooseParent} aria-label="Select parent folder">
                <FolderPlusIcon />
              </Button>
            </div>
          </Field>
          <Field label="Vault name">
            <Input value={name} placeholder="My Vault" onChange={(event) => setName(event.target.value)} />
          </Field>
          <div className="flex justify-end">
            <Button
              type="button"
              onClick={() => onContinue(newVaultRoot)}
              disabled={busy || !newVaultRoot}
            >
              {busy ? "Creating" : "Create vault"}
            </Button>
          </div>
        </div>
      </SettingsSection>
    </div>
  );
}
