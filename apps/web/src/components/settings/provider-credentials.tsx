import { useEffect, useState } from "react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { getCredentials, setCredentials, type Credentials } from "@/lib/desktop";

import { Field, SettingsSection } from "./settings-controls";

const EMPTY: Credentials = {
  igdbClientId: "",
  igdbClientSecret: "",
  tvdbApiKey: "",
  tvdbPin: "",
};

/**
 * Desktop-only provider credentials editor. Reads/writes the OS keychain via the
 * Tauri commands (the self-hosted web app reads these from env vars instead, so
 * this screen is not shown there).
 */
export function ProviderCredentials() {
  const [credentials, setCredentialsState] = useState<Credentials>(EMPTY);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string>();
  const [error, setError] = useState<string>();

  useEffect(() => {
    getCredentials()
      .then((value) => setCredentialsState({ ...EMPTY, ...value }))
      .catch(() => {});
  }, []);

  function update(key: keyof Credentials) {
    return (value: string) => setCredentialsState((current) => ({ ...current, [key]: value }));
  }

  async function save() {
    setBusy(true);
    setError(undefined);
    setMessage(undefined);
    try {
      await setCredentials(credentials);
      setMessage("Saved");
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setBusy(false);
    }
  }

  return (
    <SettingsSection
      title="Provider Credentials"
      description="Stored in your system keychain. IGDB (games) and TheTVDB (TV) require credentials; Bangumi needs none."
    >
      <div className="grid gap-3 lg:grid-cols-2">
        <Field label="IGDB Client ID">
          <Input
            value={credentials.igdbClientId}
            autoComplete="off"
            onChange={(event) => update("igdbClientId")(event.target.value)}
          />
        </Field>
        <Field label="IGDB Client Secret">
          <Input
            type="password"
            value={credentials.igdbClientSecret}
            autoComplete="off"
            onChange={(event) => update("igdbClientSecret")(event.target.value)}
          />
        </Field>
        <Field label="TheTVDB API Key">
          <Input
            type="password"
            value={credentials.tvdbApiKey}
            autoComplete="off"
            onChange={(event) => update("tvdbApiKey")(event.target.value)}
          />
        </Field>
        <Field label="TheTVDB PIN (optional)">
          <Input
            value={credentials.tvdbPin}
            autoComplete="off"
            onChange={(event) => update("tvdbPin")(event.target.value)}
          />
        </Field>
      </div>
      <div className="mt-3 flex items-center justify-end gap-2">
        {message ? <span className="text-xs text-muted-foreground">{message}</span> : null}
        {error ? <span className="text-xs text-destructive">{error}</span> : null}
        <Button type="button" onClick={save} disabled={busy}>
          {busy ? "Saving" : "Save credentials"}
        </Button>
      </div>
    </SettingsSection>
  );
}
