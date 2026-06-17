import { useEffect, useState } from "react";
import { CheckIcon, FolderOpenIcon, FolderPlusIcon, Trash2Icon } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  addVault,
  createVault,
  listVaults,
  removeVault,
  selectDirectory,
  switchVault,
  type VaultInfo,
} from "@/lib/desktop";

import { Field, SettingsSection } from "./settings-controls";
import { joinPath } from "./settings-model";

/**
 * Desktop-only multi-vault manager (Obsidian-style). Lists remembered vaults for
 * quick switching, opens an existing folder, or creates a new vault (seeded with
 * the starter template). All actions go through the Tauri commands, which rebuild
 * the in-process API for the chosen vault; `onChanged` lets the caller refetch.
 */
export function VaultSwitcher({
  onboarding = false,
  onChanged,
}: {
  onboarding?: boolean;
  onChanged?: () => void;
}) {
  const [vaults, setVaults] = useState<VaultInfo[]>([]);
  const [parent, setParent] = useState("");
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();

  useEffect(() => {
    listVaults().then(setVaults).catch((reason) => setError(message(reason)));
  }, []);

  async function run(action: () => Promise<VaultInfo[]>) {
    setBusy(true);
    setError(undefined);
    try {
      setVaults(await action());
      onChanged?.();
    } catch (reason) {
      setError(message(reason));
    } finally {
      setBusy(false);
    }
  }

  async function openExisting() {
    const dir = await selectDirectory().catch(() => undefined);
    if (dir) await run(() => addVault(dir));
  }

  async function chooseParent() {
    const dir = await selectDirectory(parent || undefined).catch(() => undefined);
    if (dir) setParent(dir);
  }

  const newRoot = parent && name.trim() ? joinPath(parent, name.trim()) : "";

  return (
    <SettingsSection
      title="Vaults"
      description={
        onboarding
          ? "Open or create a vault to get started."
          : "Switch between vaults or manage your list. Removing a vault only forgets it here — its folder is left on disk."
      }
    >
      <div className="flex flex-col gap-3">
        {error ? (
          <div className="rounded-md border border-destructive/40 bg-destructive/10 p-3 text-sm text-destructive">
            {error}
          </div>
        ) : null}

        <div className="flex flex-col gap-1">
          {vaults.map((vault) => (
            <div key={vault.path} className="flex items-center gap-2 rounded-md border px-3 py-2">
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-1.5 text-sm">
                  {vault.active ? <CheckIcon className="size-4 text-primary" /> : null}
                  <span className="truncate font-medium">{vault.name}</span>
                </div>
                <div className="truncate text-xs text-muted-foreground">{vault.path}</div>
              </div>
              {!vault.active ? (
                <Button
                  type="button"
                  size="sm"
                  variant="outline"
                  disabled={busy}
                  onClick={() => void run(() => switchVault(vault.path))}
                >
                  Open
                </Button>
              ) : null}
              <Button
                type="button"
                size="icon-sm"
                variant="ghost"
                aria-label={`Remove ${vault.name}`}
                disabled={busy}
                onClick={() => void run(() => removeVault(vault.path))}
              >
                <Trash2Icon />
              </Button>
            </div>
          ))}
          {vaults.length === 0 ? (
            <div className="rounded-md border border-dashed px-3 py-4 text-center text-xs text-muted-foreground">
              No vaults yet.
            </div>
          ) : null}
        </div>

        <div className="grid gap-3 lg:grid-cols-2">
          <div className="flex flex-col gap-2 rounded-md border p-3">
            <div className="text-sm font-medium">Open existing vault</div>
            <Button type="button" variant="outline" disabled={busy} onClick={openExisting}>
              <FolderOpenIcon data-icon="inline-start" />
              Open folder
            </Button>
          </div>
          <div className="flex flex-col gap-2 rounded-md border p-3">
            <div className="text-sm font-medium">Create new vault</div>
            <Field label="Parent folder">
              <div className="flex items-center gap-2">
                <Input value={parent} readOnly placeholder="No folder selected" />
                <Button
                  type="button"
                  variant="outline"
                  size="icon"
                  aria-label="Select parent folder"
                  onClick={chooseParent}
                >
                  <FolderPlusIcon />
                </Button>
              </div>
            </Field>
            <Field label="Vault name">
              <Input
                value={name}
                placeholder="My Vault"
                onChange={(event) => setName(event.target.value)}
              />
            </Field>
            <Button
              type="button"
              disabled={busy || !newRoot}
              onClick={() => {
                const vaultName = name.trim();
                setName("");
                void run(() => createVault(parent, vaultName));
              }}
            >
              Create vault
            </Button>
          </div>
        </div>
      </div>
    </SettingsSection>
  );
}

function message(reason: unknown) {
  if (reason instanceof Error) return reason.message;
  if (typeof reason === "string") return reason;
  return "Something went wrong";
}
