import { useMemo } from "react";
import { useQuery } from "@tanstack/react-query";
import { PlusIcon } from "lucide-react";
import { Link } from "react-router-dom";

import { errorMessage } from "@/api/client";
import { configQuery, homeQuery } from "@/api/queries";
import { ComingUpSection } from "@/components/home/coming-up-section";
import { HomeSection } from "@/components/home/home-section";
import { AppFrame } from "@/components/layout/app-frame";
import { Button } from "@/components/ui/button";
import { CONTENT_WRITES_DISABLED, useCapabilities } from "@/lib/capabilities";
import { fieldLabelsByType } from "@/lib/type-config";

export function HomePage() {
  const home = useQuery(homeQuery());
  const config = useQuery(configQuery());
  const capabilities = useCapabilities();
  const loading = home.isPending || config.isPending || capabilities.isPending;
  const error = home.error ?? config.error ?? capabilities.error;
  const labelsByType = useMemo(() => fieldLabelsByType(config.data?.types), [config.data]);

  return (
    <AppFrame error={error ? errorMessage(error) : undefined}>
      <div className="flex min-h-full flex-col">
        <header className="flex flex-wrap items-center justify-between gap-3 border-b px-4 py-3">
          <div className="min-w-0">
            <h1 className="truncate text-base font-semibold">{home.data?.title ?? "Home"}</h1>
            <p className="mt-1 text-xs text-muted-foreground">
              {loading ? "Loading" : `${home.data?.sections.length ?? 0} sections`}
            </p>
          </div>
          <Button
            type="button"
            disabled={!capabilities.contentWritable}
            title={!capabilities.contentWritable ? CONTENT_WRITES_DISABLED : "Add entity"}
            asChild={capabilities.contentWritable}
          >
            {!capabilities.contentWritable ? (
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

        {loading ? (
          <div className="p-8 text-center text-sm text-muted-foreground">Loading</div>
        ) : (
          <div className="flex flex-1 flex-col gap-6 p-4">
            <ComingUpSection />
            {home.data?.sections.length ? (
              home.data.sections.map((section) => (
                <HomeSection key={section.id} section={section} labelsByType={labelsByType} />
              ))
            ) : (
              <div className="py-8 text-center text-sm text-muted-foreground">
                No home sections configured
              </div>
            )}
          </div>
        )}
      </div>
    </AppFrame>
  );
}
