import { useEffect, useState } from "react";
import { getHome } from "@kizunashelf/api-contract";
import { PlusIcon } from "lucide-react";
import { Link } from "react-router-dom";

import { apiFetch, errorMessage } from "@/api/client";
import { getAppCapabilities } from "@/api/entities";
import { HomeSection } from "@/components/home/home-section";
import { AppFrame } from "@/components/layout/app-frame";
import { Button } from "@/components/ui/button";
import type { Capabilities, HomeResponse } from "@/types/api";

type HomeState = {
  data?: HomeResponse;
  capabilities?: Capabilities;
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
      const [data, capabilities] = await Promise.all([
        getHome(undefined, apiFetch),
        getAppCapabilities(),
      ]);
      setHome({ data, capabilities, loading: false });
    } catch (error) {
      setHome({ loading: false, error: errorMessage(error) });
    }
  }

  return (
    <AppFrame error={home.error}>
      <div className="flex min-h-full flex-col">
        <header className="flex flex-wrap items-center justify-between gap-3 border-b px-4 py-3">
          <div className="min-w-0">
            <h1 className="truncate text-base font-semibold">{home.data?.title ?? "Home"}</h1>
            <p className="mt-1 text-xs text-muted-foreground">
              {home.loading ? "Loading" : `${home.data?.sections.length ?? 0} sections`}
            </p>
          </div>
          <Button
            type="button"
            disabled={home.capabilities?.contentWritable === false}
            title={
              home.capabilities?.contentWritable === false
                ? "Content writes are disabled"
                : "Add entity"
            }
            asChild={home.capabilities?.contentWritable !== false}
          >
            {home.capabilities?.contentWritable === false ? (
              <span>
                <PlusIcon data-icon="inline-start" />
                Add
              </span>
            ) : (
              <Link to="/entities/new">
                <PlusIcon data-icon="inline-start" />
                Add
              </Link>
            )}
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
