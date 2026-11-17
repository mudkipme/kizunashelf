import { Trans, useLingui } from "@lingui/react/macro";
import { useQuery } from "@tanstack/react-query";
import {
  DownloadIcon,
  LayoutGridIcon,
  LibraryIcon,
  PencilLineIcon,
  PlusIcon,
  SearchIcon,
} from "lucide-react";
import { useMemo } from "react";
import { Link } from "react-router-dom";

import { errorMessage } from "@/api/client";
import { configQuery, homeQuery, statsQuery } from "@/api/queries";
import { ComingUpSection } from "@/components/home/coming-up-section";
import { HomeSmartList } from "@/components/home/home-smart-list";
import { AppFrame } from "@/components/layout/app-frame";
import { SuggestedListsButton } from "@/components/smart-lists/suggested-lists-dialog";
import { Button } from "@/components/ui/button";
import { contentWritesDisabled, useCapabilities } from "@/lib/capabilities";
import { useTitleLanguage } from "@/lib/language";
import { fieldLabelsByType } from "@/lib/type-config";

export function HomePage() {
  const { t } = useLingui();
  const language = useTitleLanguage();
  const home = useQuery(homeQuery(language));
  const config = useQuery(configQuery());
  const capabilities = useCapabilities();
  // Shared with the sidebar's per-type counts, so this is normally cached.
  const stats = useQuery(statsQuery());
  const loading = home.isPending || config.isPending || capabilities.isPending || stats.isPending;
  // A brand-new vault: every shelf would be empty, so lead with how to add
  // something instead of a stack of "Nothing here yet" boxes.
  const libraryEmpty = stats.data?.total === 0;
  const error = home.error ?? config.error ?? capabilities.error;
  const labelsByType = useMemo(() => fieldLabelsByType(config.data?.types), [config.data]);

  return (
    <AppFrame error={error ? errorMessage(error) : undefined}>
      <h1 className="sr-only">
        <Trans>Home</Trans>
      </h1>
      <div className="flex min-h-full flex-col">
        <header className="flex flex-wrap items-center justify-end gap-2 border-b px-4 py-2">
          <SuggestedListsButton disabled={!capabilities.contentWritable} />
          <Button
            type="button"
            disabled={!capabilities.contentWritable}
            title={!capabilities.contentWritable ? contentWritesDisabled() : t`Add`}
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
        ) : libraryEmpty ? (
          <EmptyLibraryWelcome contentWritable={capabilities.contentWritable} />
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

/// First-run Home: the library has no entries yet. Offers the three ways in —
/// search a catalog, import from another service, or write an entry by hand.
function EmptyLibraryWelcome({ contentWritable }: { contentWritable: boolean }) {
  return (
    <div className="flex flex-1 items-start justify-center p-4 sm:items-center">
      <section
        aria-labelledby="empty-library-title"
        className="flex w-full max-w-lg flex-col items-center gap-5 rounded-xl border border-dashed px-6 py-12 text-center"
      >
        <LibraryIcon className="size-10 text-muted-foreground" aria-hidden />
        <div className="flex flex-col gap-2">
          <h2 id="empty-library-title" className="text-lg font-semibold">
            <Trans>Start your library</Trans>
          </h2>
          <p className="text-sm text-muted-foreground">
            {contentWritable ? (
              <Trans>
                Search a catalog to add your first entry with its cover and details, or bring in
                what you already track elsewhere.
              </Trans>
            ) : (
              contentWritesDisabled()
            )}
          </p>
        </div>
        {contentWritable ? (
          <div className="flex w-full flex-col gap-2 sm:w-auto sm:flex-row sm:flex-wrap sm:justify-center">
            <Button asChild>
              <Link to="/entities/new">
                <SearchIcon data-icon="inline-start" />
                <Trans>Search and add</Trans>
              </Link>
            </Button>
            <Button asChild variant="outline">
              <Link to="/entities/import">
                <DownloadIcon data-icon="inline-start" />
                <Trans>Import from another service</Trans>
              </Link>
            </Button>
            <Button asChild variant="ghost">
              <Link to="/entities/new/manual">
                <PencilLineIcon data-icon="inline-start" />
                <Trans>Add manually</Trans>
              </Link>
            </Button>
          </div>
        ) : null}
      </section>
    </div>
  );
}
