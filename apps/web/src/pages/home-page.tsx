import { useEffect, useState } from "react";
import { Link } from "react-router-dom";

import { fetchJson, errorMessage } from "@/api/client";
import { HomeSection } from "@/components/home/home-section";
import { AppFrame } from "@/components/layout/app-frame";
import { Button } from "@/components/ui/button";
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
      const data = await fetchJson<HomeResponse>("/api/home");
      setHome({ data, loading: false });
    } catch (error) {
      setHome({ loading: false, error: errorMessage(error) });
    }
  }

  return (
    <AppFrame error={home.error}>
      <div className="flex min-h-[calc(100vh-3rem)] flex-col">
        <header className="flex flex-wrap items-center justify-between gap-3 border-b px-4 py-3">
          <div className="min-w-0">
            <h1 className="truncate text-base font-semibold">{home.data?.title ?? "Home"}</h1>
            <p className="mt-1 text-xs text-muted-foreground">
              {home.loading
                ? "Loading"
                : home.data
                  ? `Updated ${home.data.generatedAt.slice(0, 10)}`
                  : "No home data"}
            </p>
          </div>
          <Button asChild variant="outline" size="sm">
            <Link to="/library">Open Library</Link>
          </Button>
        </header>

        {home.loading ? (
          <div className="p-8 text-center text-sm text-muted-foreground">Loading</div>
        ) : null}

        {!home.loading && home.data?.sections.length === 0 ? (
          <div className="p-8 text-center text-sm text-muted-foreground">
            No home sections configured
          </div>
        ) : null}

        {home.data?.sections.length ? (
          <div className="grid flex-1 grid-cols-1 lg:grid-cols-3">
            {home.data.sections.map((section) => (
              <HomeSection key={section.id} section={section} />
            ))}
          </div>
        ) : null}
      </div>
    </AppFrame>
  );
}
