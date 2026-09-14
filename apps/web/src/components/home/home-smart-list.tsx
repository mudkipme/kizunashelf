import { Trans } from "@lingui/react/macro";

import { HomeEntityCard } from "@/components/home/home-entity-card";
import { SectionHeader } from "@/components/home/section-header";
import type { HomeListResponse } from "@/types/api";

export function HomeSmartList({
  section,
  labelsByType,
}: {
  section: HomeListResponse;
  labelsByType?: ReadonlyMap<string, ReadonlyMap<string, string>>;
}) {
  return (
    <section className="min-w-0">
      <SectionHeader
        title={section.name}
        count={section.total}
        viewHref={`/lists/smart/${encodeURIComponent(section.id)}${section.view ? `?view=${encodeURIComponent(section.view)}` : ""}`}
      />

      {section.items.length ? (
        <div className="grid grid-cols-[repeat(auto-fill,minmax(144px,1fr))] gap-3 sm:grid-cols-[repeat(auto-fill,minmax(156px,1fr))] xl:grid-cols-[repeat(auto-fill,minmax(168px,1fr))]">
          {section.items.map((entity) => (
            <HomeEntityCard key={entity.id} entity={entity} labelsByType={labelsByType} />
          ))}
        </div>
      ) : (
        <div className="rounded-lg border border-dashed px-3 py-10 text-center text-sm text-muted-foreground">
          <Trans>Nothing here yet</Trans>
        </div>
      )}
    </section>
  );
}
