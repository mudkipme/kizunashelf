import { Trans, useLingui } from "@lingui/react/macro";
import { useQuery } from "@tanstack/react-query";
import { LayoutGridIcon, PlusIcon } from "lucide-react";
import { useMemo } from "react";
import { Link } from "react-router-dom";

import { errorMessage } from "@/api/client";
import { configQuery, homeQuery } from "@/api/queries";
import { ComingUpSection } from "@/components/home/coming-up-section";
import { HomeSmartList } from "@/components/home/home-smart-list";
import { AppFrame } from "@/components/layout/app-frame";
import { SuggestedListsButton } from "@/components/smart-lists/suggested-lists-dialog";
import { Button } from "@/components/ui/button";
import { CONTENT_WRITES_DISABLED, useCapabilities } from "@/lib/capabilities";
import { useTitleLanguage } from "@/lib/language";
import { fieldLabelsByType } from "@/lib/type-config";

export function HomePage() {
  const { t } = useLingui();
  const language = useTitleLanguage();
  const home = useQuery(homeQuery(language));
  const config = useQuery(configQuery());
  const capabilities = useCapabilities();
  const loading = home.isPending || config.isPending || capabilities.isPending;
  const error = home.error ?? config.error ?? capabilities.error;
  const labelsByType = useMemo(() => fieldLabelsByType(config.data?.types), [config.data]);

  return (
    <AppFrame error={error ? errorMessage(error) : undefined}>
      <div className="flex min-h-full flex-col">
        <header className="flex flex-wrap items-center justify-end gap-2 border-b px-4 py-2">
          <SuggestedListsButton disabled={!capabilities.contentWritable} />
          <Button
            type="button"
            disabled={!capabilities.contentWritable}
            title={!capabilities.contentWritable ? CONTENT_WRITES_DISABLED : t`Add`}
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
            {home.data?.lists.length ? (
              home.data.lists.map((section) => (
                <HomeSmartList key={section.id} section={section} labelsByType={labelsByType} />
              ))
            ) : (
              <div className="flex flex-col items-center gap-3 rounded-xl border border-dashed py-16 text-center">
                <LayoutGridIcon className="size-8 text-muted-foreground" aria-hidden />
                <div className="flex flex-col gap-1">
                  <p className="text-sm font-medium">
                    <Trans>No smart lists on Home yet</Trans>
                  </p>
                  <p className="mx-auto max-w-sm px-4 text-xs text-muted-foreground">
                    <Trans>Add suggested lists, or open a smart list and choose Add to Home.</Trans>
                  </p>
                </div>
                <div className="flex flex-wrap justify-center gap-2">
                  <Button asChild variant="outline" size="sm">
                    <Link to="/library">
                      <Trans>Browse library</Trans>
                    </Link>
                  </Button>
                  <Button asChild variant="outline" size="sm">
                    <Link to="/lists">
                      <Trans>Browse lists</Trans>
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
