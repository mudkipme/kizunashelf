import { useEffect, useState } from "react";
import { useLingui } from "@lingui/react/macro";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { getCredentials, setCredentials, type Credentials } from "@/lib/desktop";
import type { ExternalProviderCatalogItem } from "@/types/api";

import { Field, SettingsSection } from "./settings-controls";

/**
 * Desktop-only provider credentials editor. The field list is rendered from the
 * provider catalog (`credentials` declared per provider in the Rust core), so it
 * stays in sync with the registry — no per-provider inputs are hard-coded here.
 * Reads/writes the OS keychain via the Tauri commands (the self-hosted web app
 * reads these from env vars instead, so this screen is not shown there).
 */
export function ProviderCredentials({ providers }: { providers: ExternalProviderCatalogItem[] }) {
  const { t } = useLingui();
  const [credentials, setCredentialsState] = useState<Credentials>({});
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string>();
  const [error, setError] = useState<string>();

  useEffect(() => {
    getCredentials()
      .then((value) => setCredentialsState(value ?? {}))
      .catch(() => {});
  }, []);

  // Only providers that declare credentials need an editor; keyless ones (Bangumi,
  // Google Books, …) are configured purely through the schema.
  const credentialed = providers.filter((provider) => (provider.credentials ?? []).length > 0);

  function update(key: string) {
    return (value: string) => setCredentialsState((current) => ({ ...current, [key]: value }));
  }

  async function save() {
    setBusy(true);
    setError(undefined);
    setMessage(undefined);
    try {
      await setCredentials(credentials);
      setMessage(t`Saved`);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setBusy(false);
    }
  }

  if (credentialed.length === 0) return null;

  return (
    <SettingsSection
      title={t`Provider Credentials`}
      description={t`Stored in your system keychain. Providers listed here require credentials; others (e.g. Bangumi, Google Books) need none.`}
    >
      <div className="flex flex-col gap-4">
        {credentialed.map((provider) => (
          <div key={provider.id} className="flex flex-col gap-2">
            <div className="text-sm font-medium">{provider.label}</div>
            <div className="grid gap-3 lg:grid-cols-2">
              {(provider.credentials ?? []).map((credential) => (
                <Field key={credential.key} label={credential.label}>
                  <Input
                    type={credential.secret ? "password" : "text"}
                    value={credentials[credential.key] ?? ""}
                    autoComplete="off"
                    onChange={(event) => update(credential.key)(event.target.value)}
                  />
                </Field>
              ))}
            </div>
          </div>
        ))}
      </div>
      <div className="mt-3 flex items-center justify-end gap-2">
        {message ? <span className="text-xs text-muted-foreground">{message}</span> : null}
        {error ? <span className="text-xs text-destructive">{error}</span> : null}
        <Button type="button" onClick={save} disabled={busy}>
          {busy ? t`Saving…` : t`Save credentials`}
        </Button>
      </div>
    </SettingsSection>
  );
}
