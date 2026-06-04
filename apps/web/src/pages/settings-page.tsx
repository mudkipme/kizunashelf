import { useEffect, useState } from "react";

import { errorMessage } from "@/api/client";
import { getSettingsConfig } from "@/api/settings";
import { AppFrame } from "@/components/layout/app-frame";
import { SettingsEditor } from "@/components/settings/settings-editor";
import type { SettingsConfigResponse } from "@/types/config";

type SettingsState = {
  data?: SettingsConfigResponse;
  loading: boolean;
  error?: string;
};

export function SettingsPage() {
  const [state, setState] = useState<SettingsState>({ loading: true });

  useEffect(() => {
    void load();
  }, []);

  async function load() {
    setState({ loading: true });
    try {
      const data = await getSettingsConfig();
      setState({ data, loading: false });
    } catch (error) {
      setState({ loading: false, error: errorMessage(error) });
    }
  }

  return (
    <AppFrame error={state.error ?? state.data?.error}>
      <div className="mx-auto flex w-full max-w-7xl flex-col gap-4 p-4">
        {state.loading ? (
          <div className="rounded-md border p-8 text-center text-sm text-muted-foreground">Loading</div>
        ) : state.data ? (
          <SettingsEditor
            configPath={state.data.configPath}
            initialConfig={state.data.config}
            onSaved={load}
          />
        ) : null}
      </div>
    </AppFrame>
  );
}
