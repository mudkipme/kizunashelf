import { Trans } from "@lingui/react/macro";

import { HomeEntityCard } from "@/components/home/home-entity-card";
import { SectionHeader } from "@/components/home/section-header";
import { criteriaParam, encodeCriteria } from "@/components/smart-lists/criteria-url";
import { defaultSort } from "@/lib/constants";
import type { HomeSectionResponse } from "@/types/api";

export function HomeSection({
  section,
  labelsByType,
}: {
  section: HomeSectionResponse;
  labelsByType?: ReadonlyMap<string, ReadonlyMap<string, string>>;
}) {
  return (
    <section className="min-w-0">
      <SectionHeader
        title={section.title}
        count={section.total}
        subtitle={section.typeLabel}
        viewHref={libraryHref(section)}
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

/// The section's own definition as a browse URL. A section and the library
/// browser speak the same criteria model and run through the same evaluator, so
/// this is an exact link: "See all" shows precisely the section's matches, just
/// unpaged and unlimited.
function libraryHref(section: HomeSectionResponse) {
  const params = new URLSearchParams({
    type: section.type,
    sort: sortPropertyFor(section.sort),
    direction: section.direction,
  });
  const criteria = encodeCriteria(section.criteria ?? undefined);
  if (criteria) params.set(criteriaParam, criteria);
  return `/library?${params}`;
}

/// A section declares its sort in vault config, in the entity-list vocabulary
/// (`title`, `recentlyUpdated`, `date:<field>`); browsing sorts by Bases
/// property reference. Anything without a property form (`relationCount`)
/// falls back to the title order.
function sortPropertyFor(sort: string) {
  if (sort === "recentlyUpdated") return "file.mtime";
  if (sort.startsWith("date:")) return `note.${sort.slice("date:".length)}`;
  return defaultSort;
}
