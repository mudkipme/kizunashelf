import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";

import { errorMessage } from "@/api/client";
import { getSettingsConfig } from "@/api/settings";
import { SettingsEditor } from "@/components/settings/settings-editor";
import type { SettingsConfigResponse } from "@/types/config";

type OnboardingState = {
  data?: SettingsConfigResponse;
  loading: boolean;
  error?: string;
};

export function OnboardingPage() {
  const navigate = useNavigate();
  const [state, setState] = useState<OnboardingState>({ loading: true });

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
    <main className="min-h-screen bg-background text-foreground">
      <div className="mx-auto flex w-full max-w-6xl flex-col gap-4 p-4">
        {state.error ? (
          <div className="rounded-md border border-destructive/40 bg-destructive/10 p-3 text-sm text-destructive">
            {state.error}
          </div>
        ) : null}
        {state.loading ? (
          <div className="rounded-md border p-8 text-center text-sm text-muted-foreground">Loading</div>
        ) : state.data ? (
          <SettingsEditor
            configPath={state.data.configPath}
            initialConfig={state.data.config}
            onboarding={!state.data.exists}
            onSaved={() => navigate("/", { replace: true })}
          />
        ) : null}
      </div>
    </main>
  );
}
