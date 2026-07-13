import { useMemo } from "react";
import { Trans, useLingui } from "@lingui/react/macro";
import { useQuery } from "@tanstack/react-query";
import { LayoutGridIcon, PlusIcon } from "lucide-react";
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
  const { t } = useLingui();
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
            <h1 className="truncate text-base font-semibold">{home.data?.title ?? t`Home`}</h1>
          </div>
          <Button
            type="button"
            disabled={!capabilities.contentWritable}
            title={!capabilities.contentWritable ? CONTENT_WRITES_DISABLED : t`Add entity`}
            asChild={capabilities.contentWritable}
          >
            {!capabilities.contentWritable ? (
              <span>
                <PlusIcon data-icon="inline-start" />
                <Trans>Add</Trans>
              </span>
            ) : (
              <Link to="/entities/new">
                <PlusIcon data-icon="inline-start" />
                <Trans>Add</Trans>
              </Link>
            )}
          </Button>
        </header>

        {loading ? (
          <div className="p-8 text-center text-sm text-muted-foreground">
            <Trans>Loading…</Trans>
          </div>
        ) : (
          <div className="flex flex-1 flex-col gap-6 p-4">
            <ComingUpSection />
            {home.data?.sections.length ? (
              home.data.sections.map((section) => (
                <HomeSection key={section.id} section={section} labelsByType={labelsByType} />
              ))
            ) : (
              <div className="flex flex-col items-center gap-3 rounded-xl border border-dashed py-16 text-center">
                <LayoutGridIcon className="size-8 text-muted-foreground" aria-hidden />
                <div className="space-y-1">
                  <p className="text-sm font-medium">
                    <Trans>No Home sections yet</Trans>
                  </p>
                  <p className="mx-auto max-w-sm px-4 text-xs text-muted-foreground">
                    <Trans>
                      Sections are configurable shelves of your library. Add one in Settings, or
                      jump straight into your Library.
                    </Trans>
                  </p>
                </div>
                <div className="flex flex-wrap justify-center gap-2">
                  <Button asChild variant="outline" size="sm">
                    <Link to="/library">
                      <Trans>Browse library</Trans>
                    </Link>
                  </Button>
                  <Button asChild variant="outline" size="sm">
                    <Link to="/settings">
                      <Trans>Configure Home</Trans>
                    </Link>
                  </Button>
                </div>
              </div>
            )}
          </div>
        )}
      </div>
    </AppFrame>
  );
}
