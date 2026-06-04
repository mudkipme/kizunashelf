import { useEffect, useState } from "react";
import { getHome } from "@kizunashelf/api-contract";

import { apiFetch, errorMessage } from "@/api/client";
import { HomeSection } from "@/components/home/home-section";
import { AppFrame } from "@/components/layout/app-frame";
import type { HomeResponse } from "@/types/api";

type HomeState = {
  data?: HomeResponse;
  loading: boolean;
  error?: string;
};

export function HomePage() {
  const [home, setHome] = useState<HomeState>({ loading: true });

  useEffect(() => {
    void loadHome();
  }, []);

  async function loadHome() {
    setHome({ loading: true });
    try {
      const data = await getHome(undefined, apiFetch);
      setHome({ data, loading: false });
    } catch (error) {
      setHome({ loading: false, error: errorMessage(error) });
    }
  }

  return (
    <AppFrame error={home.error}>
      <div className="flex min-h-full flex-col">
        {home.loading ? (
          <div className="p-8 text-center text-sm text-muted-foreground">Loading</div>
        ) : null}

        {!home.loading && home.data?.sections.length === 0 ? (
          <div className="p-8 text-center text-sm text-muted-foreground">
            No home sections configured
          </div>
        ) : null}

        {home.data?.sections.length ? (
          <div className="flex flex-1 flex-col gap-6 p-4">
            {home.data.sections.map((section) => (
              <HomeSection key={section.id} section={section} />
            ))}
          </div>
        ) : null}
      </div>
    </AppFrame>
  );
}
